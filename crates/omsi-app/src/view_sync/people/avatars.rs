//! "Character models: High quality": the people drawn with the Microsoft Rocketbox avatars
//! (`omsi_platform::avatars`) in place of the content's own, on this computer only. The
//! simulation keeps its people as they are - where they go, sit and pay, what a LAN session
//! shares -; only the view gives each person an avatar of the same kind (a woman, a man, a
//! child), the same one every time for the same person, and poses it with the avatar's own
//! rig. Avatars have living faces (`omsi_sim::human::face`): within a few metres of the eye
//! their blend shapes are posed every frame, and the lips move with what the person says.

use glam::Vec3;
use omsi_sim::human::avatar::AvatarInfo;
use omsi_sim::human::HumanType;
use parking_lot::Mutex;
use std::sync::{Arc, OnceLock};

/// Beyond this distance (m) a face is not posed (nobody sees a blink there).
pub(super) const FACE_DISTANCE: f64 = 14.0;

/// Whether this game session draws the avatars (read once per session, as the animation
/// choice: `OMSI_CHARACTER_MODELS` or the setting).
pub(super) fn enabled() -> bool {
    let mode = omsi_cfg::flags::OMSI_CHARACTER_MODELS.live_var().unwrap_or_else(|_| crate::settings::Settings::load().character_models);
    let on = mode.eq_ignore_ascii_case("improved") || mode.eq_ignore_ascii_case("high") || mode == "1";
    if on {
        // (loaded in the background; until then the people are the content's own)
        start_loading();
    }
    on
}

/// The avatars by kind: adult women, adult men, girls, boys.
pub(super) struct Pool {
    kinds: [Vec<Arc<HumanType>>; 4],
}

enum State {
    Idle,
    Loading,
    Ready(Arc<Pool>),
    /// No pack yet (being downloaded) or none could be loaded.
    Missing,
}

static STATE: Mutex<State> = Mutex::new(State::Idle);

fn start_loading() {
    {
        let mut s = STATE.lock();
        match *s {
            State::Idle => *s = State::Loading,
            // (a pack downloaded since the last session is taken now)
            State::Missing if omsi_platform::avatars::find_dir().is_some() => *s = State::Loading,
            _ => return,
        }
    }
    let Some(dir) = omsi_platform::avatars::find_dir() else {
        log::info!("avatars: the passenger avatars are not here yet; downloading them (the people stay the content's own until they are)");
        omsi_platform::avatars::fetch_if_missing();
        *STATE.lock() = State::Missing;
        return;
    };
    std::thread::Builder::new()
        .name("avatars load".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let list = omsi_platform::avatars::load(&dir);
            let loaded: Vec<(AvatarInfo, Arc<HumanType>)> = {
                use rayon::prelude::*;
                list.par_iter()
                    .filter_map(|a| {
                        let info = AvatarInfo { id: a.id.clone(), female: a.female, child: a.child };
                        match HumanType::load_avatar(&a.file, &info) {
                            Ok(t) => Some((info, Arc::new(t))),
                            Err(e) => {
                                log::warn!("avatar {}: {e:#}", a.id);
                                None
                            }
                        }
                    })
                    .collect()
            };
            let mut kinds: [Vec<Arc<HumanType>>; 4] = Default::default();
            for (info, t) in loaded {
                kinds[kind_index(info.female, info.child)].push(t);
            }
            let n: usize = kinds.iter().map(|k| k.len()).sum();
            log::info!(
                "avatars: {n} loaded from {} in {:.1} s ({} women, {} men, {} girls, {} boys)",
                dir.display(),
                started.elapsed().as_secs_f32(),
                kinds[0].len(),
                kinds[1].len(),
                kinds[2].len(),
                kinds[3].len()
            );
            *STATE.lock() = if n == 0 { State::Missing } else { State::Ready(Arc::new(Pool { kinds })) };
        })
        .ok();
}

fn kind_index(female: bool, child: bool) -> usize {
    match (child, female) {
        (false, true) => 0,
        (false, false) => 1,
        (true, true) => 2,
        (true, false) => 3,
    }
}

/// The avatars, once they are loaded.
pub(super) fn pool() -> Option<Arc<Pool>> {
    match &*STATE.lock() {
        State::Ready(p) => Some(p.clone()),
        _ => None,
    }
}

/// Whether a content human is a woman: her voice set (`F…`), else her file's name.
fn is_female(t: &HumanType) -> bool {
    let v = t.def.voice.trim().to_ascii_uppercase();
    if v.starts_with('F') || v.starts_with('W') {
        return true;
    }
    if v.starts_with('M') {
        return false;
    }
    let name = t.def.path.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    ["woman", "women", "frau", "female", "girl", "lady", "madchen", "mädchen", "dame"].iter().any(|w| name.contains(w))
}

fn is_child(t: &HumanType) -> bool {
    t.def.age.is_some_and(|a| a < 15) || (t.def.height > 0.5 && t.def.height < 1.45)
}

impl Pool {
    /// The avatar person `id` (of the content's type `ty`) is drawn with: one of the same
    /// kind, the same for the same person.
    pub(super) fn pick(&self, id: u32, ty: &HumanType) -> Option<Arc<HumanType>> {
        let (female, child) = (is_female(ty), is_child(ty));
        let list = [kind_index(female, child), kind_index(female, false), kind_index(!female, child)]
            .into_iter()
            .map(|k| &self.kinds[k])
            .find(|l| !l.is_empty())?;
        // (a hash of the id: neighbours in the id order are not neighbours in the list)
        let h = (id as u64 ^ 0x5bd1_e995).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
        Some(list[(h as usize) % list.len()].clone())
    }
}

/// What people are saying: by person id, when the line began and its loudness every
/// 20 ms (0 … 1).
static TALK: OnceLock<Mutex<hashbrown::HashMap<u32, (std::time::Instant, Arc<Vec<f32>>)>>> = OnceLock::new();

/// Person `speaker` starts saying `clip`: their lips follow its loudness.
pub(crate) fn speak(speaker: u32, clip: &omsi_audio::mixer::Clip) {
    let ch = clip.channels.max(1) as usize;
    let step = (clip.sample_rate as usize / 50).max(1) * ch;
    let mut env: Vec<f32> = clip
        .samples
        .chunks(step)
        .map(|c| {
            let s: f64 = c.iter().map(|v| (*v as f64 / 32768.0).powi(2)).sum();
            (s / c.len().max(1) as f64).sqrt() as f32
        })
        .collect();
    let peak = env.iter().copied().fold(0.0f32, f32::max).max(1e-4);
    for v in &mut env {
        *v = (*v / peak).sqrt();
    }
    TALK.get_or_init(Default::default).lock().insert(speaker, (std::time::Instant::now(), Arc::new(env)));
}

/// How loud person `id` speaks now, if they do.
pub(super) fn talking(id: u32) -> Option<f32> {
    let talk = TALK.get()?;
    let mut t = talk.lock();
    let (start, env) = t.get(&id)?;
    let k = (start.elapsed().as_secs_f32() * 50.0) as usize;
    match env.get(k) {
        Some(v) => Some(*v),
        None => {
            t.remove(&id);
            None
        }
    }
}

/// The hour of the day (for the yawns late in the evening), as the clock last said.
static HOUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(12 << 16);

pub(crate) fn set_hour(h: f32) {
    HOUR.store((h.clamp(0.0, 24.0) * 65536.0) as u32, std::sync::atomic::Ordering::Relaxed);
}

pub(super) fn hour() -> f32 {
    HOUR.load(std::sync::atomic::Ordering::Relaxed) as f32 / 65536.0
}

/// A person's face as drawn: its state, and the rest pose its expression made.
pub(crate) struct FaceView {
    pub(super) face: omsi_sim::human::face::Face,
    pub(super) rest: (Vec<Vec3>, Vec<Vec3>),
    /// The face was posed last frame (a face posed back to rest once more when it leaves).
    pub(super) live: bool,
}

impl FaceView {
    pub(super) fn new(seed: u32) -> FaceView {
        FaceView { face: omsi_sim::human::face::Face::new(seed as u64), rest: (Vec::new(), Vec::new()), live: false }
    }
}

/// The file a body material's texture is read from: the clothing variant's own folder
/// first, else the type's folders (as `PeopleView::make_meshes` looks it up).
pub(super) fn material_texture(ty: &HumanType, dirs: &[std::path::PathBuf], texture: &str, variant: usize) -> Option<std::path::PathBuf> {
    let (name, first) = ty.variant_texture(texture, variant);
    let mut look: Vec<&std::path::Path> = first.into_iter().collect();
    look.extend(dirs.iter().map(|p| p.as_path()));
    omsi_texture::find_texture(name, &look).or_else(|| omsi_texture::find_texture(texture, &look))
}

/// A texture of an avatar read ahead on a worker: the picture for the GPU and the PBR maps
/// beside it.
pub(super) type Prepared = (Option<Arc<omsi_texture::TextureData>>, Option<omsi_texture::pbr::PbrImages>);

/// The avatars whose textures are read on the workers before anybody is drawn with them.
/// Read on the frame, an avatar that came into view for the first time cost 25-70 ms
/// (colour, normal and opacity pictures of up to 2048 pixels decoded), and walking or
/// driving past new people was a string of such frames (#2463). Until its pictures are
/// there, a person is drawn as the content's own human and changes into the avatar then.
#[derive(Default)]
pub(crate) struct Warm {
    /// Avatars (by `Arc` address) whose pictures are read.
    ready: hashbrown::HashSet<usize>,
    /// Avatars being read.
    reading: hashbrown::HashSet<usize>,
    /// What the workers finished: the avatar and its pictures by file.
    done: Arc<Mutex<Vec<(usize, Vec<(std::path::PathBuf, Prepared)>)>>>,
    /// Pictures read and not uploaded yet, by file.
    pub(super) prepared: hashbrown::HashMap<std::path::PathBuf, Prepared>,
    /// The map's own figures being read ahead (see `Warm::map_figures`).
    figures: Option<FigureRead>,
}

/// How much the read-ahead of a map's figures decodes in all (bytes): it stops there, and
/// the figures left are decoded on the frame of their first person, as before.
const FIGURES_AHEAD_BYTES: usize = 160 << 20;

/// The read-ahead of one map's figures: on a thread of its own, after nothing else (the
/// timetable fleet's read-ahead and the avatars keep the shared workers), one figure file
/// at a time.
struct FigureRead {
    /// The figures' files, the list it was started for.
    figures: Vec<std::path::PathBuf>,
    /// Set when the map changes: the thread stops before its next picture.
    stop: Arc<std::sync::atomic::AtomicBool>,
    /// The pictures it finished, not yet taken in.
    out: Arc<Mutex<Vec<(std::path::PathBuf, Prepared)>>>,
    /// The bytes decoded so far by its threads together (see `FIGURES_AHEAD_BYTES`).
    decoded: Arc<std::sync::atomic::AtomicUsize>,
    /// The pictures it put into `Warm::prepared` (taken out again if the map changes before
    /// anybody is drawn with them).
    given: Vec<std::path::PathBuf>,
}

impl Drop for FigureRead {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Warm {
    /// Whether avatar `a`'s pictures are read; starts reading them on a worker if not.
    pub(super) fn ready(&mut self, a: &Arc<HumanType>, root: &std::path::Path) -> bool {
        for (k, list) in self.done.lock().drain(..) {
            self.reading.remove(&k);
            self.ready.insert(k);
            self.prepared.extend(list);
        }
        let key = Arc::as_ptr(a) as usize;
        if self.ready.contains(&key) {
            return true;
        }
        if self.reading.insert(key) {
            let (a, root, done) = (a.clone(), root.to_path_buf(), self.done.clone());
            omsi_scene::threads::background_pool().spawn(move || {
                let dirs = a.texture_dirs(&root);
                let mut seen = hashbrown::HashSet::new();
                let mut list = Vec::new();
                for m in a.meshes.iter().flat_map(|hm| hm.materials.iter()) {
                    let Some(path) = material_texture(&a, &dirs, &m.texture, 0) else { continue };
                    if !seen.insert(path.clone()) {
                        continue;
                    }
                    let data = omsi_texture::gpu::load_gpu(&path).ok().map(|(t, _)| Arc::new(t));
                    let pbr = if a.face.is_some() { omsi_texture::pbr::load_set(&omsi_texture::pbr::find(&path)) } else { None };
                    list.push((path, (data, pbr)));
                }
                done.lock().push((key, list));
            });
        }
        false
    }

    /// Have the pictures of the map's own figures (in their default clothes) decoded ahead,
    /// up to `FIGURES_AHEAD_BYTES`: the first person of a figure otherwise read and decoded
    /// them on the frame - a busy stop with a big pack of figures stalled for up to half a
    /// second at a time. The person is drawn at once all the same; whatever is not there yet
    /// is read on the frame as before. A new list of figures (another map) stops the read of
    /// the old one and drops what it read that nobody used.
    pub(super) fn map_figures(&mut self, types: &[Arc<HumanType>], root: &std::path::Path) {
        // (the same figures, or the same with more at the end - the player's own figure joins
        // the list later: the read goes on, and the new ones get a thread of their own)
        let grown = |f: &FigureRead| f.figures.len() <= types.len() && f.figures.iter().zip(types).all(|(p, t)| *p == t.def.path);
        if self.figures.as_ref().is_some_and(|f| !grown(f)) {
            if let Some(old) = self.figures.take() {
                for p in &old.given {
                    self.prepared.remove(p);
                }
            }
        }
        let f = match self.figures.as_mut() {
            Some(f) => f,
            None if types.is_empty() => return,
            None => self.figures.insert(FigureRead {
                figures: Vec::new(),
                stop: Arc::default(),
                out: Arc::default(),
                decoded: Arc::default(),
                given: Vec::new(),
            }),
        };
        if f.figures.len() < types.len() {
            let fresh = &types[f.figures.len()..];
            f.figures.extend(fresh.iter().map(|t| t.def.path.clone()));
            read_figures_ahead(fresh, root, f);
        }
        let Some(f) = self.figures.as_mut() else { return };
        for (path, pictures) in f.out.lock().drain(..) {
            if let hashbrown::hash_map::Entry::Vacant(entry) = self.prepared.entry(path.clone()) {
                f.given.push(path);
                entry.insert(pictures);
            }
        }
    }
}

/// Read `types`' default pictures on a thread of their own, into `read`'s output and budget
/// (see `FigureRead`).
fn read_figures_ahead(types: &[Arc<HumanType>], root: &std::path::Path, read: &FigureRead) {
    use std::sync::atomic::Ordering;
    let (list, root) = (types.to_vec(), root.to_path_buf());
    let (halt, sink, decoded) = (read.stop.clone(), read.out.clone(), read.decoded.clone());
    let spawned = std::thread::Builder::new().name("figure pictures".into()).spawn(move || {
        let mut seen = hashbrown::HashSet::new();
        for t in &list {
            let dirs = t.texture_dirs(&root);
            for m in t.meshes.iter().flat_map(|hm| hm.materials.iter()) {
                if halt.load(Ordering::Relaxed) || decoded.load(Ordering::Relaxed) >= FIGURES_AHEAD_BYTES {
                    return;
                }
                let Some(path) = material_texture(t, &dirs, &m.texture, 0) else { continue };
                if !seen.insert(path.clone()) {
                    continue;
                }
                let Ok((data, _)) = omsi_texture::gpu::load_gpu(&path) else { continue };
                decoded.fetch_add(data.levels.iter().map(Vec::len).sum::<usize>(), Ordering::Relaxed);
                sink.lock().push((path, (Some(Arc::new(data)), None)));
            }
        }
    });
    if let Err(e) = spawned {
        log::warn!("people: no thread to read the figures' pictures ahead: {e}");
    }
}
