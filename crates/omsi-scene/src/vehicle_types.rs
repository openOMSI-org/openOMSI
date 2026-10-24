//! Vehicle render sets, slots, looks and the vehicle prefetch.
use super::*;


/// GPU-side representation of a vehicle instance: one render instance per mesh.
pub struct VehicleRender {
    pub window_wipers: Option<crate::window_wipers::WindowWipers>,
    pub instances: Vec<usize>,
    /// The instances of the lower levels of detail: they stand where the vehicle's body does
    /// (no animation) and are drawn while the vehicle is small on the screen.
    pub far: Vec<usize>,
    /// Materials made for this vehicle alone (its text and script texture slots).
    pub own_materials: Vec<MaterialId>,
    /// The shared set it is drawn with (None for the player's own).
    pub set: Option<VehicleKey>,
    /// `[matl_change]` / `[texchanges]` slots whose material a variable switches.
    pub variants: Vec<VariantSlot>,
    /// GPU texture per `[texttexture]` index.
    pub text_textures: Vec<Option<TextureId>>,
    /// GPU texture per `[scripttexture]` index.
    pub script_textures: Vec<Option<TextureId>>,
    /// `script_textures` belong to the vehicle this part is coupled to (`[scriptshare]`):
    /// they are not this render's to give back.
    pub shared_script: bool,
    /// The script textures (cockpit and passenger displays, 1024×512 pictures for a C2) wait
    /// with their upload this frame: the vehicle is far and it is not its half second.
    pub displays_far: bool,
    /// The half second a far vehicle's displays were last uploaded in.
    pub display_tick: u64,
    /// `[smoothskin]` meshes drawn from a copy of their own (the player's articulated bus's
    /// bellows): (mesh index, the copy, the bone transforms it was last shaped for).
    pub skinned: Vec<(usize, MeshId, Vec<Mat4>)>,
    /// Vehicle meshes with per-instance collision damage: (mesh index, GPU copy, CPU geometry).
    pub damaged: Vec<(usize, MeshId, omsi_geometry::MeshData)>,
    /// Colored, opaque crack overlays attached to damaged glass panes.
    pub glass_cracks: Vec<(usize, usize, MeshId)>,
    /// An AI vehicle out of sight: its instances are hidden and not updated (see
    /// `Traffic::sync`).
    pub hidden: bool,
    /// The renderer's slots for the vehicle's `[interiorlight]` lamps (first, count), taken
    /// the first time the vehicle is synced (see `sync_interior_lamps`).
    pub interior_lamps: std::cell::Cell<Option<(u32, u32)>>,
    /// The runs of lamp slots and which of the vehicle's lamps each holds.
    pub interior_blocks: std::cell::OnceCell<Vec<(u32, Vec<usize>)>>,
}

/// A material slot whose texture is generated per vehicle (text or script texture).
#[derive(Debug, Clone)]
pub struct DynSlot {
    pub mesh: usize,
    pub slot: usize,
    pub text: Option<usize>,
    pub script: Option<usize>,
    pub script_trans: Option<usize>,
    /// The script texture's page says `lit` (`Model::html_page_lit`): the material is
    /// made lit with the slot's others, not drawn as the display such a page is.
    pub lit: bool,
    pub tex: Option<TextureId>,
    pub alpha: AlphaMode,
    pub transmap: Option<(TextureId, bool)>,
    pub night: Option<TextureId>,
    pub lightmap: Option<TextureId>,
    pub envmap: Option<(TextureId, f32)>,
    /// `[matl_texadress_*]`: how the slot's textures read outside [0, 1].
    pub address: omsi_render::TexAddressing,
    /// Depth handling, reflection mask and specular term of the slot.
    pub extra: MaterialExtra,
    /// Diffuse and emissive colour of the slot's D3D material (see `d3d_material`); a
    /// script texture is drawn unlit and keeps its own colours.
    pub color: [f32; 4],
    pub emissive: [f32; 3],
}

/// Whether a `[matl_change]` variable at `x` shows the slot's `[matl_item]`: Omsi.exe
/// (0x5fd6xx) rounds the variable (to the nearest, ties to even) and shows item `n` for
/// 1 <= n <= the items there are, the plain material otherwise - a lamp's variable at 2
/// with one item is dark. A variable no script declares is registered by the model loader
/// at 0 (the stock MANs' spare buttons, `*Noch nicht belegt*`, and a mod's door lamps
/// were lit for good when it was taken as on, #231).
pub fn change_picks_item(x: f32) -> bool {
    x.is_finite() && x.round_ties_even() == 1.0
}

/// A material variant switched by a variable.
#[derive(Clone)]
pub struct VariantSlot {
    pub mesh: usize,
    pub slot: usize,
    pub base: MaterialId,
    pub item: MaterialId,
    /// Items 2, 3, ... of the first `[matl_change]`.
    pub more: Vec<MaterialId>,
    /// `[matl_change]` variable: at 1 (rounded) the item variant shows.
    pub var: String,
    /// The variables of the slot's further `[matl_change]`s: the item shows while any of
    /// them is on as well (Omsi.exe sub_7c2d80: each record picks its item by its own
    /// variable, and one at 0 leaves the material to the others).
    pub more_vars: Vec<String>,
    /// `[texchanges]`: the (base, item) pair of every entry of the master, in order.
    pub entries: Vec<(MaterialId, MaterialId)>,
    /// `[texchanges]` variable: its integer value picks the entry.
    pub tex_var: String,
    /// The slot's free textures, each on the halves that declare it.
    pub free: Vec<FreeTex>,
    /// The materials made for one combination of the free textures' pictures (and of the
    /// light maps switched on): plain, first item, further items. A half no free texture
    /// and no light map touches keeps the slot's own material (None).
    pub free_built: HashMap<String, FreeBuilt>,
    /// The light maps the current free materials were made with (None: none made yet).
    pub free_lights: Option<Option<u32>>,
    /// How to build a material of this slot for a texture loaded later.
    pub spec: SlotSpec,
    /// The textures `base`/`item` and each entry were made with (made again per vehicle
    /// when the slot shows the vehicle's own pictures, see `SlotSpec::per_vehicle`).
    pub base_tex: Option<TextureId>,
    pub entry_tex: Vec<Option<TextureId>>,
    /// Several `[matl_lightmap]`s on the slot (see `MultiLight`).
    pub lights: Option<MultiLight>,
}

/// A material slot with several `[matl_lightmap]`s, each a texture and a variable (the
/// LiAZ 5292's saloon: the cab lamp, saloon circuit 1 and circuit 2). OMSI keeps them
/// all; drawn with only the last one, the
/// saloon stayed unlit whenever circuit 2 was off. The slot's light map is the sum of the
/// maps switched on, made the first time that combination shows and shared by every
/// vehicle of the kind; the slot is as bright as its brightest variable.
#[derive(Clone)]
pub struct MultiLight {
    /// The maps' files and variables, in the order the model lists them.
    pub maps: Vec<(PathBuf, String)>,
    /// The set's own materials (the last map), shown while none is on.
    pub plain: (MaterialId, MaterialId),
    /// Materials made per switched-on combination (bit k: map k).
    pub cache: HashMap<u32, (MaterialId, MaterialId)>,
    pub current: u32,
    /// Composite maps are shared like the vehicles' pictures (see `FreeTex::shared`).
    pub shared: Arc<Mutex<HashMap<PathBuf, (TextureId, usize)>>>,
    pub held: Vec<PathBuf>,
}

impl MultiLight {
    /// The light map made of the maps in `mask`, from the shared store or read now.
    pub fn composite(&mut self, renderer: &Renderer, scene: &mut Scene, mask: u32) -> Option<TextureId> {
        let mut key = String::from("lightmap-sum:");
        for (k, (path, _)) in self.maps.iter().enumerate() {
            if mask & (1 << k) != 0 {
                key.push_str(&path.to_string_lossy());
                key.push('|');
            }
        }
        let key = PathBuf::from(key);
        let mut shared = self.shared.lock();
        if let Some(e) = shared.get_mut(&key) {
            e.1 += 1;
            self.held.push(key);
            return Some(e.0);
        }
        let mut sum: Option<omsi_texture::Image> = None;
        for (k, (path, _)) in self.maps.iter().enumerate() {
            if mask & (1 << k) == 0 {
                continue;
            }
            let Ok(img) = omsi_texture::decode_file(path) else { continue };
            match &mut sum {
                None => sum = Some(img),
                Some(acc) => {
                    // (maps of another size are sampled at the nearest texel)
                    let (w, h) = (acc.width as usize, acc.height as usize);
                    let (iw, ih) = (img.width as usize, img.height as usize);
                    for y in 0..h {
                        let sy = y * ih / h.max(1);
                        for x in 0..w {
                            let sx = x * iw / w.max(1);
                            let (d, s) = ((y * w + x) * 4, (sy * iw + sx) * 4);
                            for c in 0..3 {
                                // (ADDSMOOTH, as Omsi.exe chains a slot's maps in its
                                // texture stages, 0x7fe5ff: a + b - a b)
                                let (a, b) = (acc.rgba[d + c] as u32, img.rgba[s + c] as u32);
                                acc.rgba[d + c] = (a + b - a * b / 255).min(255) as u8;
                            }
                        }
                    }
                }
            }
        }
        let img = sum?;
        let id = renderer.add_texture(scene, &img, true);
        shared.insert(key.clone(), (id, 1));
        self.held.push(key);
        Some(id)
    }
}

/// `[matl_freetex]`: the slot shows the texture file named by a string variable - the
/// SD200's destination roller reads the terminus pictures of the map's `.hof` this way.
/// `halves` are the records of the slot's halves in [`SlotSpec`] order: the plain material,
/// its first item, then the further items. Each half shows the free textures its own records
/// list (an item's list starts as the slot's, see `MaterialDef::freetexes`); one (texture,
/// variable) pair is one free texture with the halves it is on as bits (bit 0 the plain
/// material, bit 1 the first item, bit 2 + k further item k).
pub(super) fn free_texture_defs(halves: &[Vec<&MaterialDef>]) -> Vec<(u32, String, String)> {
    let mut out: Vec<(u32, String, String)> = Vec::new();
    for (bit, records) in halves.iter().enumerate().take(32) {
        for (key, var) in records.iter().flat_map(|o| o.freetexes.iter()) {
            let same = |e: &&mut (u32, String, String)| e.1.eq_ignore_ascii_case(key) && e.2.eq_ignore_ascii_case(var);
            match out.iter_mut().find(same) {
                Some(e) => e.0 |= 1 << bit,
                None => out.push((1 << bit, key.clone(), var.clone())),
            }
        }
    }
    out
}

/// The materials of one combination of free pictures (see `VariantSlot::free_built`).
#[derive(Clone, Debug, Default)]
pub struct FreeBuilt {
    pub base: Option<MaterialId>,
    pub item: Option<MaterialId>,
    pub more: Vec<Option<MaterialId>>,
}

impl FreeBuilt {
    pub fn materials(&self) -> impl Iterator<Item = MaterialId> + '_ {
        self.base.into_iter().chain(self.item).chain(self.more.iter().flatten().copied())
    }
}

#[derive(Clone)]
pub struct FreeTex {
    pub reloads: Arc<crate::FreeTextureReloads>,
    pub var: String,
    /// The original named texture can be used by several stages (a display commonly
    /// names the same black texture as its diffuse and its switched night map).
    pub key: Option<TextureId>,
    pub diffuse: bool,
    /// The halves of the slot it is declared on (see `free_texture_defs`): one declared
    /// in a `[matl_item]` changes neither the plain material nor another item.
    pub halves: u32,
    /// Where the file name is looked up (the vehicle's texture folders).
    pub dirs: Vec<PathBuf>,
    pub textures: Arc<omsi_texture::TextureCache>,
    /// File name (lower case) → the picture found for it (None: no such file, the slot
    /// keeps its own texture there).
    pub pictures: HashMap<String, Option<TextureId>>,
    /// The name currently applied (lower case).
    pub current: Option<String>,
    /// The world's shared vehicle textures (a picture is one texture for every vehicle
    /// showing it; every bus had its own copy, half a gigabyte of destination pictures on
    /// Ahlheim), the pictures this vehicle holds, and where pictures uploaded as RGBA are
    /// sent to be compressed.
    pub shared: Arc<Mutex<HashMap<PathBuf, (TextureId, usize)>>>,
    pub held: Vec<PathBuf>,
    pub wants_upgrade: Arc<Mutex<Vec<PathBuf>>>,
}

impl VariantSlot {
    /// The material the slot shows now: `[texchanges]` picks the texture, `[matl_change]`
    /// then picks between the plain material and the `[matl_item]` variant.
    pub fn material(&self, var: impl Fn(&str) -> Option<f32>) -> MaterialId {
        let (base, item) = if self.entries.is_empty() {
            (self.base, self.item)
        } else {
            let v = var(&self.tex_var).unwrap_or(0.0);
            let i = if v.is_finite() { v.trunc() as i64 } else { 0 };
            self.entries[i.clamp(0, self.entries.len() as i64 - 1) as usize]
        };
        let x = self.var.trim().parse().ok().or_else(|| var(&self.var)).unwrap_or(0.0);
        if self.entries.is_empty() && x.is_finite() {
            let n = x.round_ties_even();
            if n >= 2.0 && ((n - 2.0) as usize) < self.more.len() {
                return self.more[(n - 2.0) as usize];
            }
        }
        if change_picks_item(x) || self.more_vars.iter().any(|v| var(v).is_some_and(change_picks_item)) {
            item
        } else {
            base
        }
    }
}

/// How one material slot is built, so that the same description can be applied to every
/// texture a `[texchanges]` master or a `[matl_freetex]` string switches between.
#[derive(Clone)]
pub struct SlotSpec {
    pub(super) base: Look,
    /// The `[matl_item]` half.
    pub(super) item: Option<Look>,
    /// The first `[matl_change]`'s items after its first (shown at 2, 3, ...).
    pub(super) more: Vec<Look>,
}

/// How one half of a material slot (the plain material or its `[matl_item]`) is drawn.
#[derive(Clone)]
pub struct Look {
    pub(super) alpha: AlphaMode,
    pub(super) color: [f32; 4],
    pub(super) emissive: [f32; 3],
    pub(super) unlit: bool,
    /// The script texture's page says `lit` (`Model::html_page_lit`): the half is
    /// shaded as a surface, not drawn as the display such a page is.
    pub(super) lit: bool,
    /// A picture of the vehicle's own in place of the diffuse texture (see `DynTex`).
    pub(super) diffuse: Option<TextureId>,
    pub(super) transmap: Option<(TextureId, bool)>,
    pub(super) night: Option<TextureId>,
    pub(super) lightmap: Option<TextureId>,
    pub(super) envmap: Option<(TextureId, f32)>,
    pub(super) extra: MaterialExtra,
    pub(super) dyn_tex: DynTex,
}

/// The pictures of its own vehicle a half of a slot shows: `[useTextTexture]`,
/// `[useScriptTexture]` and a script texture as the transparency map
/// (`[matl_transmap] \S:n`), and whether it is addressed without repeating.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DynTex {
    pub(super) text: Option<usize>,
    pub(super) script: Option<usize>,
    pub(super) script_trans: Option<usize>,
    pub(super) address: omsi_render::TexAddressing,
}

impl DynTex {
    pub(super) fn any(&self) -> bool {
        self.text.is_some() || self.script.is_some() || self.script_trans.is_some()
    }
}

impl Look {
    pub(super) fn add(&self, renderer: &Renderer, scene: &mut Scene, tex: Option<TextureId>) -> MaterialId {
        renderer.address_next.set(self.dyn_tex.address);
        renderer.add_material_extra(
            scene,
            self.diffuse.or(tex),
            self.alpha,
            self.color,
            self.unlit,
            self.transmap,
            self.night,
            self.lightmap,
            self.envmap,
            self.emissive,
            self.extra,
        )
    }

    /// This half with one vehicle's text and script textures, set up as
    /// `instantiate_vehicle` sets up a `DynSlot`.
    pub(super) fn for_vehicle(&self, text: &[Option<TextureId>], script: &[Option<TextureId>]) -> Look {
        let d = self.dyn_tex;
        let mut l = self.clone();
        l.dyn_tex = DynTex {
            address: d.address,
            ..DynTex::default()
        };
        if let Some(t) = d.text.and_then(|i| text.get(i).copied().flatten()) {
            // lit as the slot's own material is, as `instantiate_vehicle` makes a text slot
            // that is not switched: drawn unlit, a switched slot's fleet number or plate
            // shone at full brightness at night (#698)
            let mut extra = l.extra;
            extra.display = text_is_display(l.lightmap.is_some(), l.night.is_some());
            extra.screen = true;
            // (the slot's own emissive stays: a text field made self-lit in its .x - the
            // emissive colour 1, 1, 1 - shines as OMSI shows it, the New Lion's City's
            // number plate and setvar screen; it was taken away, #1384)
            return Look {
                diffuse: Some(t),
                alpha: AlphaMode::Blend,
                color: [1.0; 4],
                emissive: l.emissive,
                unlit: false,
                transmap: None,
                envmap: None,
                extra,
                ..l
            };
        }
        if let Some(t) = d.script.and_then(|i| script.get(i).copied().flatten()) {
            l.diffuse = Some(t);
        }
        if let Some(t) = d
            .script_trans
            .and_then(|i| script.get(i).copied().flatten())
        {
            l.transmap = Some((t, true));
        }
        // A transmap is not a reason by itself to force a material into blend mode; it only
        // carries the alpha for the chosen material mode. Keep any explicit alpha setting,
        // otherwise body slots stay solid.
        if d.script.is_some() {
            l.color = [1.0; 4];
            l.emissive = [0.0; 3];
            // ... unlit, unless the script is a page whose `[htmltexture]` block says
            // `lit`: the word is the page's own and rode down on the `Look`
            l.unlit = !l.lit;
        }
        l
    }
}

impl SlotSpec {
    pub fn with_freetex(
        &self,
        key: Option<TextureId>,
        tex: TextureId,
        diffuse: bool,
        halves: u32,
    ) -> Self {
        let mut spec = self.clone();
        let replace = |look: &mut Look| {
            // A per-vehicle text/script texture has already replaced the original
            // diffuse and is not the file named by this free-texture declaration.
            if (diffuse && look.diffuse.is_none()) || (key.is_some() && look.diffuse == key) {
                look.diffuse = Some(tex);
            }
            if let Some(key) = key {
                for stage in [&mut look.night, &mut look.lightmap] {
                    if *stage == Some(key) {
                        *stage = Some(tex);
                    }
                }
                if let Some((id, _)) = &mut look.transmap {
                    if *id == key {
                        *id = tex;
                    }
                }
                if let Some((id, _)) = &mut look.envmap {
                    if *id == key {
                        *id = tex;
                    }
                }
            }
        };
        for (bit, look) in spec.halves_mut().into_iter().enumerate() {
            if bit < 32 && halves & (1 << bit) != 0 {
                replace(look);
            }
        }
        spec
    }

    /// The halves in `free_texture_defs` order: plain, first item, further items.
    fn halves_mut(&mut self) -> Vec<&mut Look> {
        let mut out = vec![&mut self.base];
        out.extend(self.item.as_mut());
        out.extend(self.more.iter_mut());
        out
    }

    /// The materials of the halves in `halves` (bits as in `free_texture_defs`; a further
    /// item without a first item cannot be). Each is made last, so `recycle` can move it.
    pub fn build_halves(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        tex: Option<TextureId>,
        halves: u32,
        mut recycle: impl FnMut(&mut Scene, MaterialId) -> MaterialId,
    ) -> FreeBuilt {
        let on = |bit: usize| bit < 32 && halves & (1 << bit) != 0;
        let mut make = |scene: &mut Scene, l: &Look| {
            let m = l.add(renderer, scene, tex);
            recycle(scene, m)
        };
        FreeBuilt {
            base: on(0).then(|| make(scene, &self.base)),
            item: self.item.as_ref().filter(|_| on(1)).map(|l| make(scene, l)),
            more: self.more.iter().enumerate().map(|(k, l)| on(2 + k).then(|| make(scene, l))).collect(),
        }
    }

    /// (plain material, `[matl_item]` material) for one diffuse texture; without a
    /// `[matl_item]` both are the same material.
    pub fn build(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        tex: Option<TextureId>,
    ) -> (MaterialId, MaterialId) {
        let base = self.base.add(renderer, scene, tex);
        let item = match &self.item {
            Some(it) => it.add(renderer, scene, tex),
            None => base,
        };
        (base, item)
    }

    /// The slot shows pictures of its own vehicle: every vehicle makes its materials with
    /// `for_vehicle`.
    /// The same slot with another light map.
    pub fn set_lightmap(&mut self, tex: Option<TextureId>) {
        self.base.lightmap = tex;
        if let Some(it) = &mut self.item {
            it.lightmap = tex;
        }
        for it in &mut self.more {
            it.lightmap = tex;
        }
    }

    /// The further items' materials (each made last, so that `recycle` can move it).
    pub fn build_more(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        tex: Option<TextureId>,
        mut recycle: impl FnMut(&mut Scene, MaterialId) -> MaterialId,
    ) -> Vec<MaterialId> {
        self.more
            .iter()
            .map(|l| {
                let m = l.add(renderer, scene, tex);
                recycle(scene, m)
            })
            .collect()
    }

    pub fn per_vehicle(&self) -> bool {
        self.base.dyn_tex.any() || self.item.as_ref().is_some_and(|i| i.dyn_tex.any()) || self.more.iter().any(|i| i.dyn_tex.any())
    }

    pub fn for_vehicle(
        &self,
        text: &[Option<TextureId>],
        script: &[Option<TextureId>],
    ) -> SlotSpec {
        SlotSpec {
            base: self.base.for_vehicle(text, script),
            item: self.item.as_ref().map(|i| i.for_vehicle(text, script)),
            more: self.more.iter().map(|i| i.for_vehicle(text, script)).collect(),
        }
    }
}

/// A vehicle type with a paint scheme, as its GPU set is known by.
pub type VehicleKey = (PathBuf, Option<usize>);

/// The GPU side of a vehicle type in one paint scheme, shared by its AI copies: meshes and
/// materials, the slots every copy fills itself, and what the set holds of the shared
/// textures and meshes (given back when the set is trimmed).
/// A mesh of a lower level of detail of a vehicle on the GPU: plain materials, drawn in place
/// of the full model while the vehicle is small on the screen (see `lod_bands`).
#[derive(Clone)]
pub struct FarMesh {
    pub(super) level: usize,
    pub(super) mesh: MeshId,
    pub(super) mats: Vec<MaterialId>,
    pub(super) casts: bool,
}

#[derive(Clone)]
pub struct VehicleSet {
    pub(super) meshes: Vec<(MeshId, Vec<MaterialId>)>,
    /// The meshes of the lower levels of detail (none: the model has one level).
    pub(super) far: Vec<FarMesh>,
    pub(super) dyn_slots: Vec<DynSlot>,
    pub(super) variants: Vec<VariantSlot>,
    pub(super) textures: Vec<PathBuf>,
    pub(super) mesh_keys: Vec<(PathBuf, usize)>,
    pub(super) materials: Vec<MaterialId>,
    /// Vehicles drawn with it now, and since when nobody is.
    pub(super) users: usize,
    pub(super) idle_since: Option<std::time::Instant>,
}

/// What reads vehicle sets ahead on a worker thread: their textures and meshes, made on the
/// GPU right there (the device takes calls from any thread) and waiting in `ready` until the
/// set is uploaded - which then only puts them into the scene and makes the materials. A
/// C2's set took 60 ms on the thread that draws when it had to read and upload it all.
#[derive(Clone)]
pub struct VehiclePrefetch {
    pub(super) root: PathBuf,
    pub(super) textures: Arc<TextureCache>,
    pub(super) on_gpu: Arc<Mutex<HashMap<PathBuf, (TextureId, usize)>>>,
    pub(super) meshes_on_gpu: Arc<Mutex<HashMap<(PathBuf, usize), (MeshId, usize)>>>,
    pub(super) ready: Arc<Mutex<PreparedVehicles>>,
    pub(super) gpu: (wgpu::Device, wgpu::Queue),
    pub(super) mesh_pages: bool,
}

/// Vehicle meshes and textures made on a worker, by (bus file, mesh) and by file.
#[derive(Default)]
pub(super) struct PreparedVehicles {
    pub(super) meshes: HashMap<(PathBuf, usize), omsi_render::PreparedMesh>,
    pub(super) textures: HashMap<PathBuf, (omsi_render::PreparedTexture, omsi_texture::PixelFormat)>,
}

impl World {
    /// A reader for a vehicle being placed (see `spawn::PendingPlacement`): what it makes
    /// stays its own until the vehicle is put down (`accept_placement_prefetch`), so a
    /// placement that is dropped leaves no meshes or textures behind.
    pub fn placement_prefetch(&self, renderer: &Renderer) -> VehiclePrefetch {
        VehiclePrefetch {
            ready: Arc::new(Mutex::new(PreparedVehicles::default())),
            ..self.vehicle_prefetch(renderer)
        }
    }

    /// What a placement's reader made, given to the world's for the upload; what the
    /// fleet made meanwhile wins, and the copies are dropped.
    pub fn accept_placement_prefetch(&self, staged: VehiclePrefetch) {
        let PreparedVehicles { meshes, textures } = std::mem::take(&mut *staged.ready.lock());
        let mut ready = self.vehicle_ready.lock();
        for (key, value) in meshes {
            ready.meshes.entry(key).or_insert(value);
        }
        for (key, value) in textures {
            ready.textures.entry(key).or_insert(value);
        }
    }
}

impl VehiclePrefetch {
    /// Read what uploading `vt` in `scheme` will ask for and the GPU does not have.
    pub fn prefetch(&self, vt: &omsi_sim::VehicleType, scheme: Option<usize>) {
        // OpenGL has one adapter context; GPU uploads from this worker can time out while
        // the render thread holds it, so let the normal vehicle upload handle them.
        if omsi_render::gl_backend() {
            return;
        }
        for (name, dirs) in vehicle_texture_names(&self.root, vt, scheme) {
            let refs: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
            let Some(path) = omsi_texture::find_texture(&name, &refs) else {
                continue;
            };
            // what its alpha covers, which the upload reads to tell glass from bodywork
            // (`slot_alpha`): the whole picture decoded for it on the thread that draws was
            // most of the 0.1 to 1 s a coach's set took there
            let _ = crate::vehicle_materials::alpha_mask(&path);
            if self.on_gpu.lock().contains_key(&path)
                || self.ready.lock().textures.contains_key(&path)
            {
                continue;
            }
            let Some(data) = self.textures.get_gpu_path(&path) else {
                continue;
            };
            // on the GPU now, unless the GPU has to make its chain (the upload does that)
            if let Some(t) = omsi_render::prepare_texture(&self.gpu.0, &self.gpu.1, &data) {
                self.textures.release(&path);
                self.ready
                    .lock()
                    .textures
                    .entry(path)
                    .or_insert((t, data.format));
            }
        }
        // [matl_bumpmap] height maps, kept under their own key
        for (name, dirs) in vehicle_bump_names(&self.root, vt, scheme) {
            let refs: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
            let Some(path) = omsi_texture::find_texture(&name, &refs) else {
                continue;
            };
            let key = bump_key(&path);
            if self.on_gpu.lock().contains_key(&key)
                || self.ready.lock().textures.contains_key(&key)
            {
                continue;
            }
            let Some(data) = load_texture_key(&key, true) else {
                continue;
            };
            if let Some(t) = omsi_render::prepare_texture(&self.gpu.0, &self.gpu.1, &data) {
                self.ready
                    .lock()
                    .textures
                    .entry(key)
                    .or_insert((t, data.format));
            }
        }
        let mut todo = Vec::new();
        for i in 0..vt.meshes.len() {
            let key = (vt.def.path.clone(), i);
            if self.meshes_on_gpu.lock().contains_key(&key)
                || self.ready.lock().meshes.contains_key(&key)
            {
                continue;
            }
            if let Some(d) = crate::pane_uv::gpu_mesh(vt, i) {
                todo.push((key, d));
            }
        }
        let data: Vec<&omsi_geometry::MeshData> = todo.iter().map(|(_, d)| d.as_ref()).collect();
        let meshes = omsi_render::prepare_meshes(&self.gpu.0, &self.gpu.1, &data, self.mesh_pages);
        let mut ready = self.ready.lock();
        for ((key, _), m) in todo.into_iter().zip(meshes) {
            ready.meshes.entry(key).or_insert(m);
        }
    }
}

/// The texture names (with their folders) uploading a vehicle in a paint scheme looks up,
/// as `World::upload_vehicle` does.
pub(super) fn vehicle_texture_names(
    root: &Path,
    vt: &omsi_sim::VehicleType,
    scheme: Option<usize>,
) -> Vec<(String, Vec<PathBuf>)> {
    let dirs = vt.texture_dirs(root);
    let (subst, _scheme_dir) = match scheme {
        Some(i) => vt.scheme_substitutions(i, omsi_texture::find_texture),
        None => (vt.default_substitutions(root), None),
    };
    let subst = |name: &str| -> String {
        subst
            .get(&name.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| name.to_string())
    };
    let mut out: Vec<(String, Vec<PathBuf>)> = Vec::new();
    let mut push = |name: String, d: &Vec<PathBuf>| {
        let name = name.trim().to_string();
        if !name.is_empty() && !name.starts_with("\\S:") && !name.starts_with('!') && mirror_index(&name).is_none() {
            out.push((name, d.clone()));
        }
    };
    let lods = if crate::vehicles::lods::lods_misfit(vt).is_none() { vt.lod_meshes.as_slice() } else { &[] };
    for vm in vt.meshes.iter().chain(lods) {
        for m in &vm.materials {
            match vt.texchange(&m.texture) {
                Some(master) => {
                    let mut edirs = vec![master.dir.clone()];
                    edirs.extend(dirs.iter().cloned());
                    for e in &master.entries {
                        push(subst(e), &edirs);
                    }
                }
                None => push(subst(&m.texture), &dirs),
            }
        }
        for o in &vm.overrides {
            for name in [
                o.nightmap.clone().map(|t| subst(&t)),
                o.transmap.clone().map(|t| subst(&t)),
                o.lightmap.clone().map(|l| subst(&l.0)),
                o.envmap.clone().map(|e| e.0),
                o.envmap_mask
                    .clone()
                    .filter(|_| o.envmap.is_some())
                    .map(|t| subst(&t)),
            ]
            .into_iter()
            .flatten()
            {
                push(name, &dirs);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The `[matl_bumpmap]` files (with their folders) uploading a vehicle in a paint scheme
/// asks for (only where the slot reflects, as `World::upload_vehicle` does).
pub(super) fn vehicle_bump_names(
    root: &Path,
    vt: &omsi_sim::VehicleType,
    scheme: Option<usize>,
) -> Vec<(String, Vec<PathBuf>)> {
    if omsi_cfg::flags::OMSI_NO_BUMP.is_set() || omsi_cfg::flags::OMSI_NO_ENVMAP.is_set() {
        return Vec::new();
    }
    let dirs = vt.texture_dirs(root);
    let (subst, _scheme_dir) = match scheme {
        Some(i) => vt.scheme_substitutions(i, omsi_texture::find_texture),
        None => (vt.default_substitutions(root), None),
    };
    let mut out: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for vm in &vt.meshes {
        for o in vm.overrides.iter().filter(|o| o.envmap.is_some()) {
            if let Some((t, _)) = &o.bumpmap {
                let name = subst
                    .get(&t.to_ascii_lowercase())
                    .cloned()
                    .unwrap_or_else(|| t.clone());
                if !name.trim().is_empty() {
                    out.push((name.trim().to_string(), dirs.clone()));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The lamps (indices into the model's `[interiorlight]`s) a mesh's or a seat's
/// `[illumination_interior]` names: those that exist, each once, as many as a mesh may have.
/// A negative number is left out, as Omsi.exe's mesh lamps do: 0x5fa8f0 skips a byte
/// below 0 and sets Direct3D lights only for the lamps it keeps.
pub fn lamp_set(indices: &[i32], n: usize) -> Vec<usize> {
    let mut set: Vec<usize> = Vec::new();
    for &k in indices {
        if k >= 0 && (k as usize) < n && !set.contains(&(k as usize)) && set.len() < omsi_render::MAX_LAMPS_PER_MESH as usize {
            set.push(k as usize);
        }
    }
    set
}

impl VehicleRender {
    /// The lamp slots (first, count) for `set_interior_lamps` that light a person on a seat
    /// with these four lamps (`PassPos::illumination`) in a vehicle of `n` lamps; None
    /// before the vehicle's lamps are first synced, or when none of them exists.
    pub fn seat_lamps(&self, n: usize, lamps: &[i32; 4]) -> Option<(u32, u32)> {
        let set = lamp_set(lamps, n);
        let blocks = self.interior_blocks.get()?;
        blocks.iter().find(|b| b.1 == set).map(|b| (b.0, set.len() as u32))
    }
}
