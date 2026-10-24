use crate::gpu::{mip_count, GpuOptions, PixelFormat, Prepared, TextureData};
use parking_lot::Mutex;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAGIC: &[u8; 4] = b"OOTC";
pub const FORMAT_VERSION: u32 = 1;
const BUILD: &str = env!("TEXTURE_SOURCE_HASH");
pub const DEFAULT_CAP: u64 = if cfg!(any(target_os = "ios", target_os = "android")) { 512 << 20 } else { 2 << 30 };
const MAX_ENTRY: u64 = 1 << 30;
const TOUCH_AFTER: Duration = Duration::from_secs(3600);
const STALE_TEMP: Duration = Duration::from_secs(600);
const ENTRY_EXT: &str = "tex";
const TEMP_EXT: &str = "tmp";

pub struct DiskCache {
    dir: PathBuf,
    cap: u64,
    budget: Mutex<Budget>,
    started: SystemTime,
    full: AtomicBool,
    #[cfg(test)]
    hold_after_admission: Option<Duration>,
}

#[derive(Default)]
struct Budget {
    on_disk: Option<u64>,
    pending: u64,
}

pub(crate) struct Key {
    name: String,
    header: Vec<u8>,
}

pub(crate) enum Entry {
    Texture(TextureData, Prepared),
    Kept { width: u32, height: u32, info: Prepared },
}

impl DiskCache {
    pub fn new(dir: PathBuf, cap: u64) -> DiskCache {
        DiskCache {
            dir,
            cap,
            budget: Mutex::new(Budget::default()),
            started: SystemTime::now() - Duration::from_secs(2),
            full: AtomicBool::new(false),
            #[cfg(test)]
            hold_after_admission: None,
        }
    }

    pub fn global() -> Option<&'static DiskCache> {
        static CACHE: OnceLock<Option<DiskCache>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                if omsi_cfg::flags::OMSI_NO_TEXTURE_CACHE.is_set() {
                    return None;
                }
                if cfg!(target_os = "ios") {
                    let container = PathBuf::from(std::env::var_os("OPENOMSI_CONTAINER")?);
                    return container.is_absolute().then(|| DiskCache::new(container.join("Library").join("Caches").join("textures"), DEFAULT_CAP));
                }
                if !cfg!(target_os = "android") {
                    return Some(DiskCache::new(omsi_cfg::user_data::data_dir().join("cache/textures"), DEFAULT_CAP));
                }
                let home = PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?);
                home.is_absolute().then(|| DiskCache::new(home.join(".openomsi").join("cache").join("textures"), DEFAULT_CAP))
            })
            .as_ref()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn load(&self, key: &Key) -> Option<Entry> {
        let path = self.dir.join(&key.name);
        let file = File::open(&path).ok()?;
        let md = file.metadata().ok()?;
        if !md.is_file() || md.len() > MAX_ENTRY || md.len() < key.header.len() as u64 + 8 {
            return None;
        }
        let mut buf = Vec::with_capacity(md.len() as usize);
        file.take(MAX_ENTRY).read_to_end(&mut buf).ok()?;
        let entry = parse(&buf, key)?;
        if md.modified().ok().and_then(|m| m.elapsed().ok()).is_none_or(|age| age > TOUCH_AFTER) {
            let _ = File::options().write(true).open(&path).and_then(|f| f.set_modified(SystemTime::now()));
        }
        Some(entry)
    }

    pub(crate) fn store(&self, key: &Key, t: &TextureData, info: Prepared) {
        if self.full.load(Ordering::Relaxed) {
            return;
        }
        let Some(body) = serialise(key, t, info) else { return };
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let len = body.len() as u64;
        if !self.admit(len) {
            return;
        }
        #[cfg(test)]
        if let Some(hold) = self.hold_after_admission {
            std::thread::sleep(hold);
        }
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = self.dir.join(&key.name);
        let tmp = self.dir.join(format!("{}.{}.{}.{TEMP_EXT}", key.name, std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
        let written = File::create(&tmp).and_then(|mut f| f.write_all(&body)).is_ok();
        let mut budget = self.budget.lock();
        let replaced = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let stored = written && std::fs::rename(&tmp, &path).is_ok();
        if !stored {
            let _ = std::fs::remove_file(&tmp);
        }
        budget.pending -= len;
        if stored {
            budget.on_disk = Some((budget.on_disk.unwrap_or(0) + len).saturating_sub(replaced));
        }
    }

    fn admit(&self, len: u64) -> bool {
        let mut budget = self.budget.lock();
        if self.full.load(Ordering::Relaxed) {
            return false;
        }
        let mut on_disk = budget.on_disk.unwrap_or_else(|| self.trim(u64::MAX));
        if on_disk + budget.pending + len > self.cap {
            on_disk = self.trim((self.cap / 4 * 3).saturating_sub(budget.pending + len));
        }
        budget.on_disk = Some(on_disk);
        if on_disk + budget.pending + len > self.cap {
            self.full.store(true, Ordering::Relaxed);
            return false;
        }
        budget.pending += len;
        true
    }

    fn trim(&self, target: u64) -> u64 {
        let Ok(dir) = std::fs::read_dir(&self.dir) else { return 0 };
        let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for e in dir.flatten() {
            let path = e.path();
            let Ok(md) = e.metadata() else { continue };
            if !md.is_file() {
                continue;
            }
            let modified = md.modified().unwrap_or(UNIX_EPOCH);
            match path.extension().and_then(|x| x.to_str()) {
                Some(ENTRY_EXT) => entries.push((modified, md.len(), path)),
                Some(TEMP_EXT) if modified.elapsed().is_ok_and(|age| age > STALE_TEMP) => {
                    let _ = std::fs::remove_file(&path);
                }
                _ => {}
            }
        }
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        if total <= target {
            return total;
        }
        entries.sort();
        for (modified, len, path) in entries {
            if total <= target || modified >= self.started {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
        total
    }
}

impl Key {
    pub(crate) fn new(bytes: &[u8], path: &Path, o: GpuOptions) -> Option<Key> {
        if omsi_cfg::vfs::is_sealed(path) {
            return None;
        }
        let (source, stamped) = match omsi_cfg::vfs::archive_of(path) {
            Some(archive) => (path.to_path_buf(), archive),
            None => (std::fs::canonicalize(path).ok()?, path.to_path_buf()),
        };
        let mtime = std::fs::metadata(&stamped).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let mut header = Vec::with_capacity(96 + source.as_os_str().len());
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        put_bytes(&mut header, BUILD.as_bytes());
        header.push(o.bc as u8 | (o.compress as u8) << 1);
        put_bytes(&mut header, ext.as_bytes());
        put_bytes(&mut header, source.as_os_str().as_encoded_bytes());
        let name = format!("{:016x}.{ENTRY_EXT}", xxh64(&header, 0));
        header.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        header.extend_from_slice(&mtime.as_secs().to_le_bytes());
        header.extend_from_slice(&mtime.subsec_nanos().to_le_bytes());
        header.extend_from_slice(&xxh64(bytes, 0).to_le_bytes());
        Some(Key { name, header })
    }

    #[cfg(test)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

impl Entry {
    pub(crate) fn restore(self, bytes: &[u8], path: &Path) -> Option<(TextureData, Prepared)> {
        match self {
            Entry::Texture(t, info) => Some((t, info)),
            Entry::Kept { width, height, info } => {
                let img = crate::decode_bytes(bytes, path).ok()?;
                let img = crate::gpu::at_block_size(img, width, height)?;
                Some((TextureData::from_image(img), info))
            }
        }
    }
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b);
}

fn format_code(f: PixelFormat) -> u8 {
    match f {
        PixelFormat::Rgba8 => 0,
        PixelFormat::Bc1 => 1,
        PixelFormat::Bc2 => 2,
        PixelFormat::Bc3 => 3,
        PixelFormat::Bc7 => 7,
    }
}

fn format_of(code: u8) -> Option<PixelFormat> {
    [PixelFormat::Rgba8, PixelFormat::Bc1, PixelFormat::Bc2, PixelFormat::Bc3, PixelFormat::Bc7].into_iter().find(|&f| format_code(f) == code)
}

fn level_size(f: PixelFormat, w: u32, h: u32, l: usize) -> usize {
    f.level_bytes((w >> l).max(1), (h >> l).max(1))
}

fn serialise(key: &Key, t: &TextureData, info: Prepared) -> Option<Vec<u8>> {
    let kept = info.rejected && t.format == PixelFormat::Rgba8;
    if !kept && (!t.format.is_compressed() || t.levels.is_empty() || t.levels.len() as u32 > mip_count(t.width, t.height)) {
        return None;
    }
    if !kept && t.levels.iter().enumerate().any(|(l, d)| d.len() != level_size(t.format, t.width, t.height, l)) {
        return None;
    }
    let payload: usize = if kept { 0 } else { t.levels.iter().map(|l| l.len()).sum() };
    let mut b = Vec::with_capacity(key.header.len() + 48 + payload);
    b.extend_from_slice(&key.header);
    b.push(kept as u8);
    b.extend_from_slice(&info.psnr.0.to_bits().to_le_bytes());
    b.extend_from_slice(&info.psnr.1.to_bits().to_le_bytes());
    b.push(info.encoded as u8);
    b.push(info.rejected as u8);
    b.extend_from_slice(&t.width.to_le_bytes());
    b.extend_from_slice(&t.height.to_le_bytes());
    if !kept {
        b.push(format_code(t.format));
        b.push(t.has_alpha as u8);
        b.push(t.gpu_mips as u8);
        b.extend_from_slice(&(t.levels.len() as u32).to_le_bytes());
        for l in &t.levels {
            b.extend_from_slice(l);
        }
    }
    let sum = xxh64(&b, 0);
    b.extend_from_slice(&sum.to_le_bytes());
    Some(b)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn flag(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn f64(&mut self) -> Option<f64> {
        self.take(8).map(|b| f64::from_bits(u64::from_le_bytes(b.try_into().unwrap())))
    }
}

fn parse(buf: &[u8], key: &Key) -> Option<Entry> {
    let (body, sum) = buf.split_at_checked(buf.len().checked_sub(8)?)?;
    if xxh64(body, 0) != u64::from_le_bytes(sum.try_into().ok()?) {
        return None;
    }
    let mut r = Reader(body.strip_prefix(key.header.as_slice())?);
    let kept = r.flag()?;
    let psnr = (r.f64()?, r.f64()?);
    let info = Prepared { psnr, encoded: r.flag()?, rejected: r.flag()? };
    let (width, height) = (r.u32()?, r.u32()?);
    if width == 0 || height == 0 || width as usize > crate::MAX_DIMENSION || height as usize > crate::MAX_DIMENSION {
        return None;
    }
    if kept {
        return (r.0.is_empty() && info.rejected).then_some(Entry::Kept { width, height, info });
    }
    let format = format_of(r.u8()?).filter(|f| f.is_compressed())?;
    let (has_alpha, gpu_mips) = (r.flag()?, r.flag()?);
    let n = r.u32()?;
    if n == 0 || n > mip_count(width, height) {
        return None;
    }
    let levels = (0..n as usize).map(|l| r.take(level_size(format, width, height, l)).map(<[u8]>::to_vec)).collect::<Option<Vec<_>>>()?;
    r.0.is_empty().then_some(Entry::Texture(TextureData { width, height, format, levels, has_alpha, gpu_mips }, info))
}

const P1: u64 = 0x9E37_79B1_85EB_CA87;
const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const P3: u64 = 0x1656_67B1_9E37_79F9;
const P4: u64 = 0x85EB_CA77_C2B2_AE63;
const P5: u64 = 0x27D4_EB2F_1656_67C5;

fn round(acc: u64, v: u64) -> u64 {
    acc.wrapping_add(v.wrapping_mul(P2)).rotate_left(31).wrapping_mul(P1)
}

fn merge(h: u64, v: u64) -> u64 {
    (h ^ round(0, v)).wrapping_mul(P1).wrapping_add(P4)
}

fn le64(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}

pub fn xxh64(data: &[u8], seed: u64) -> u64 {
    let (stripes, mut rest) = data.as_chunks::<32>();
    let mut h = if data.len() >= 32 {
        let mut v = [seed.wrapping_add(P1).wrapping_add(P2), seed.wrapping_add(P2), seed, seed.wrapping_sub(P1)];
        for s in stripes {
            for (i, acc) in v.iter_mut().enumerate() {
                *acc = round(*acc, le64(&s[i * 8..]));
            }
        }
        let mut h = v[0].rotate_left(1).wrapping_add(v[1].rotate_left(7)).wrapping_add(v[2].rotate_left(12)).wrapping_add(v[3].rotate_left(18));
        for acc in v {
            h = merge(h, acc);
        }
        h
    } else {
        seed.wrapping_add(P5)
    };
    h = h.wrapping_add(data.len() as u64);
    while rest.len() >= 8 {
        h = (h ^ round(0, le64(rest))).rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
        rest = &rest[8..];
    }
    if rest.len() >= 4 {
        h = (h ^ (u32::from_le_bytes(rest[..4].try_into().unwrap()) as u64).wrapping_mul(P1)).rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
        rest = &rest[4..];
    }
    for &b in rest {
        h = (h ^ (b as u64).wrapping_mul(P5)).rotate_left(11).wrapping_mul(P1);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(P2);
    h ^= h >> 29;
    h = h.wrapping_mul(P3);
    h ^ (h >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::{load_gpu_bytes, load_gpu_cached};

    const ON: GpuOptions = GpuOptions { bc: true, compress: true };

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!("omsi-texture-cache-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn cache(&self, cap: u64) -> DiskCache {
            DiskCache::new(self.0.join("cache"), cap)
        }

        fn entries(&self) -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(self.0.join("cache")).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
            v.sort();
            v
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tga(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        b.extend_from_slice(&(w as u16).to_le_bytes());
        b.extend_from_slice(&(h as u16).to_le_bytes());
        b.extend_from_slice(&[32, 0x28]);
        for p in rgba.as_chunks::<4>().0 {
            b.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        b
    }

    fn dxt1(w: u32, h: u32) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[8..12].copy_from_slice(&0x1007u32.to_le_bytes());
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[28..32].copy_from_slice(&1u32.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(b"DXT1");
        for i in 0..(w / 4 * h / 4) {
            b.extend_from_slice(&[0xFF, 0xF0 - (i % 200) as u8, 0x10, i as u8, 0x1B, 0x6C, 0xC6, (i * 7) as u8]);
        }
        b
    }

    fn noise(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let mut s = 7u32;
        (0..w * h * 4)
            .map(|i| {
                s = s.wrapping_mul(1103515245).wrapping_add(12345);
                if i % 4 == 3 && !alpha { 255 } else { (s >> 16) as u8 }
            })
            .collect()
    }

    fn pictures() -> Vec<(&'static str, Vec<u8>)> {
        let (w, h) = (128u32, 128u32);
        let smooth: Vec<u8> = (0..w * h).flat_map(|i| [(i % w) as u8, (i / w) as u8, 100, 255]).collect();
        let soft: Vec<u8> = (0..w * h).flat_map(|i| [50, 60, (i / w) as u8, (i % w) as u8 * 2]).collect();
        let mut atlas = smooth.clone();
        let swatches = [[230u8, 14, 23], [40, 200, 60], [20, 30, 220], [240, 220, 30]];
        for y in 0..4 {
            for (x, c) in swatches.iter().enumerate() {
                atlas[((y * w as usize) + 8 + x) * 4..][..3].copy_from_slice(c);
            }
        }
        let odd: Vec<u8> = (0..130 * 131).flat_map(|i: u32| [(i % 130) as u8, (i / 131) as u8, 40, 255]).collect();
        vec![
            ("smooth.tga", tga(w, h, &smooth)),
            ("soft.tga", tga(w, h, &soft)),
            ("atlas.tga", tga(w, h, &atlas)),
            ("noise.tga", tga(w, h, &noise(w, h, false))),
            ("odd.tga", tga(130, 131, &odd)),
            ("tiny.tga", tga(16, 16, &noise(16, 16, true))),
            ("chain.dds", dxt1(64, 32)),
        ]
    }

    fn same(a: &(TextureData, Prepared), b: &(TextureData, Prepared)) -> bool {
        let (ta, ia) = a;
        let (tb, ib) = b;
        (ta.width, ta.height, ta.format, ta.has_alpha, ta.gpu_mips) == (tb.width, tb.height, tb.format, tb.has_alpha, tb.gpu_mips)
            && ta.levels == tb.levels
            && (ia.psnr.0.to_bits(), ia.psnr.1.to_bits(), ia.encoded, ia.rejected) == (ib.psnr.0.to_bits(), ib.psnr.1.to_bits(), ib.encoded, ib.rejected)
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn xxh64_matches_the_reference() {
        assert_eq!(xxh64(b"", 0), 0xef46_db37_51d8_e999);
        assert_eq!(xxh64(b"abc", 0), 0x44bc_2cf5_ad77_0999);
        assert_eq!(xxh64(&(0..79).collect::<Vec<u8>>(), 0), 0x5306_130d_1ba4_b651);
        let long: Vec<u8> = (0..3).flat_map(|_| 0..=255u8).chain(*b"xyz12").collect();
        assert_eq!(xxh64(&long, 0), 0x9b7d_50c0_4781_8b1b);
    }

    #[test]
    fn cached_textures_are_the_ones_computed() {
        let s = Scratch::new("same");
        let cache = s.cache(DEFAULT_CAP);
        for (name, bytes) in pictures() {
            let path = write(&s.0, name, &bytes);
            let fresh = load_gpu_bytes(&bytes, &path, ON).unwrap();
            let cold = load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap();
            assert!(same(&fresh, &cold), "{name} cold");
            let key = Key::new(&bytes, &path, ON).unwrap();
            let kept = cache.load(&key).is_some();
            assert_eq!(kept, name != "tiny.tga", "{name} {:?} {:?}", fresh.0.format, fresh.1);
            let warm = load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap();
            assert!(same(&fresh, &warm), "{name} warm");
        }
        let kinds: Vec<_> = pictures().iter().map(|(n, b)| (*n, load_gpu_bytes(b, &s.0.join(n), ON).unwrap())).map(|(n, (t, i))| (n, t.format, i.rejected)).collect();
        assert!(kinds.contains(&("atlas.tga", PixelFormat::Bc7, false)), "{kinds:?}");
        assert!(kinds.contains(&("noise.tga", PixelFormat::Rgba8, true)), "{kinds:?}");
        assert!(kinds.contains(&("soft.tga", PixelFormat::Bc3, false)), "{kinds:?}");
        assert!(kinds.contains(&("chain.dds", PixelFormat::Bc1, false)), "{kinds:?}");
        assert!(s.entries().iter().all(|p| p.extension().is_some_and(|x| x == ENTRY_EXT)));
    }

    #[test]
    fn nothing_is_kept_without_work() {
        let s = Scratch::new("idle");
        let cache = s.cache(DEFAULT_CAP);
        let (_, bytes) = pictures().into_iter().next().unwrap();
        let path = write(&s.0, "smooth.tga", &bytes);
        for o in [GpuOptions { bc: false, compress: false }, GpuOptions { bc: false, compress: true }, GpuOptions { bc: true, compress: false }] {
            let fresh = load_gpu_bytes(&bytes, &path, o).unwrap();
            assert!(same(&fresh, &load_gpu_cached(&bytes, &path, o, Some(&cache)).unwrap()));
        }
        assert!(s.entries().is_empty());
        let mut dds = dxt1(8, 8);
        dds[8..12].copy_from_slice(&(0x1007u32 | 0x20000).to_le_bytes());
        dds[28..32].copy_from_slice(&4u32.to_le_bytes());
        dds.extend_from_slice(&[0; 24]);
        let path = write(&s.0, "full.dds", &dds);
        assert!(same(&load_gpu_bytes(&dds, &path, ON).unwrap(), &load_gpu_cached(&dds, &path, ON, Some(&cache)).unwrap()));
        assert!(s.entries().is_empty());
    }

    #[test]
    fn a_damaged_entry_is_computed_again() {
        let s = Scratch::new("damaged");
        let cache = s.cache(DEFAULT_CAP);
        for (name, bytes) in pictures().into_iter().filter(|(n, _)| ["smooth.tga", "noise.tga", "chain.dds"].contains(n)) {
            let path = write(&s.0, name, &bytes);
            let fresh = load_gpu_bytes(&bytes, &path, ON).unwrap();
            load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap();
            let key = Key::new(&bytes, &path, ON).unwrap();
            let entry = cache.dir().join(key.name());
            let good = std::fs::read(&entry).unwrap();
            let mut damages: Vec<Vec<u8>> = vec![Vec::new(), good[..good.len() / 2].to_vec(), good[..good.len() - 1].to_vec(), vec![0xAB; good.len()]];
            for at in [0, 4, key.header.len() - 3, key.header.len() + 2, good.len() / 2, good.len() - 9, good.len() - 1] {
                let mut d = good.clone();
                d[at] ^= 0x10;
                damages.push(d);
            }
            let mut longer = good.clone();
            longer.extend_from_slice(&[0; 16]);
            damages.push(longer);
            for d in damages {
                std::fs::write(&entry, &d).unwrap();
                assert!(cache.load(&key).is_none(), "{name}");
                assert!(same(&fresh, &load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap()), "{name}");
                assert_eq!(std::fs::read(&entry).unwrap(), good, "{name}");
            }
            std::fs::remove_file(&entry).unwrap();
            std::fs::create_dir(&entry).unwrap();
            assert!(same(&fresh, &load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap()), "{name}");
            std::fs::remove_dir(&entry).unwrap();
        }
        let blocked = DiskCache::new(write(&s.0, "a-file", b"x").join("cache"), DEFAULT_CAP);
        let (name, bytes) = pictures().into_iter().next().unwrap();
        let path = s.0.join(name);
        assert!(same(&load_gpu_bytes(&bytes, &path, ON).unwrap(), &load_gpu_cached(&bytes, &path, ON, Some(&blocked)).unwrap()));
    }

    #[test]
    fn a_changed_file_or_option_is_computed_again() {
        let s = Scratch::new("changed");
        let cache = s.cache(DEFAULT_CAP);
        let all = pictures();
        let (smooth, atlas) = (&all[0].1, &all[2].1);
        assert_eq!(smooth.len(), atlas.len());
        let path = write(&s.0, "x.tga", smooth);
        let base = std::fs::metadata(&path).unwrap().modified().unwrap();
        load_gpu_cached(smooth, &path, ON, Some(&cache)).unwrap();
        let first = Key::new(smooth, &path, ON).unwrap();
        assert!(cache.load(&first).is_some());

        let f = File::options().write(true).open(&path).unwrap();
        f.set_modified(base + Duration::from_secs(5)).unwrap();
        drop(f);
        let touched = Key::new(smooth, &path, ON).unwrap();
        assert_eq!(touched.name(), first.name());
        assert!(cache.load(&touched).is_none());
        assert!(same(&load_gpu_bytes(smooth, &path, ON).unwrap(), &load_gpu_cached(smooth, &path, ON, Some(&cache)).unwrap()));
        assert!(cache.load(&touched).is_some() && cache.load(&first).is_none());

        std::fs::write(&path, atlas).unwrap();
        File::options().write(true).open(&path).unwrap().set_modified(base + Duration::from_secs(5)).unwrap();
        let swapped = Key::new(atlas, &path, ON).unwrap();
        assert!(cache.load(&swapped).is_none());
        let got = load_gpu_cached(atlas, &path, ON, Some(&cache)).unwrap();
        assert!(same(&load_gpu_bytes(atlas, &path, ON).unwrap(), &got));
        assert_eq!(got.0.format, PixelFormat::Bc7);

        let mut bigger = atlas.clone();
        bigger.extend_from_slice(&[0; 4]);
        std::fs::write(&path, &bigger).unwrap();
        File::options().write(true).open(&path).unwrap().set_modified(base + Duration::from_secs(5)).unwrap();
        assert!(cache.load(&Key::new(&bigger, &path, ON).unwrap()).is_none());

        std::fs::write(&path, atlas).unwrap();
        let other = GpuOptions { bc: true, compress: false };
        let dds = &all[6].1;
        let dpath = write(&s.0, "y.dds", dds);
        load_gpu_cached(dds, &dpath, ON, Some(&cache)).unwrap();
        let (k_on, k_off) = (Key::new(dds, &dpath, ON).unwrap(), Key::new(dds, &dpath, other).unwrap());
        assert_ne!(k_on.name(), k_off.name());
        assert!(cache.load(&k_on).is_some() && cache.load(&k_off).is_none());
        assert!(same(&load_gpu_bytes(dds, &dpath, other).unwrap(), &load_gpu_cached(dds, &dpath, other, Some(&cache)).unwrap()));
        assert!(cache.load(&k_off).is_some());

        let same_bytes = write(&s.0, "z.tga", smooth);
        assert_ne!(Key::new(smooth, &same_bytes, ON).unwrap().name(), first.name());
    }

    #[test]
    fn the_cache_stays_under_its_cap() {
        let s = Scratch::new("cap");
        let (_, bytes) = pictures().into_iter().next().unwrap();
        let probe = s.cache(DEFAULT_CAP);
        let path = write(&s.0, "p0.tga", &bytes);
        load_gpu_cached(&bytes, &path, ON, Some(&probe)).unwrap();
        let one = s.entries().iter().map(|p| std::fs::metadata(p).unwrap().len()).sum::<u64>();
        std::fs::remove_dir_all(s.0.join("cache")).unwrap();
        let cache = s.cache(one * 4 + one / 2);
        let t0 = SystemTime::now() - Duration::from_secs(10_000);
        let mut names = Vec::new();
        for i in 0..4 {
            let path = write(&s.0, &format!("p{i}.tga"), &bytes);
            load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap();
            let key = Key::new(&bytes, &path, ON).unwrap();
            File::options().write(true).open(cache.dir().join(key.name())).unwrap().set_modified(t0 + Duration::from_secs(i * 100)).unwrap();
            names.push(key.name().to_string());
        }
        let stale = cache.dir().join(format!("{}.1.2.{TEMP_EXT}", names[0]));
        std::fs::write(&stale, b"half").unwrap();
        File::options().write(true).open(&stale).unwrap().set_modified(t0).unwrap();
        assert_eq!(s.entries().len(), 5);
        let path = write(&s.0, "p4.tga", &bytes);
        load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap();
        let left: Vec<String> = s.entries().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let total: u64 = s.entries().iter().map(|p| std::fs::metadata(p).unwrap().len()).sum();
        assert!(total <= cache.cap / 4 * 3, "{total} {left:?}");
        assert!(!left.contains(&names[0]) && !left.contains(&names[1]) && left.contains(&names[3]), "{left:?}");
        assert!(left.contains(&Key::new(&bytes, &path, ON).unwrap().name().to_string()));
        assert!(!stale.exists());
    }

    #[test]
    fn a_start_that_outgrows_the_cap_keeps_its_own_entries_and_stops_storing() {
        let s = Scratch::new("outgrown");
        let (_, bytes) = pictures().into_iter().next().unwrap();
        let probe = s.cache(DEFAULT_CAP);
        let path = write(&s.0, "p0.tga", &bytes);
        load_gpu_cached(&bytes, &path, ON, Some(&probe)).unwrap();
        let one = s.entries().iter().map(|p| std::fs::metadata(p).unwrap().len()).sum::<u64>();
        std::fs::remove_dir_all(s.0.join("cache")).unwrap();
        let cache = s.cache(one * 2 + one / 2);
        let mut names = Vec::new();
        for i in 0..5 {
            let path = write(&s.0, &format!("p{i}.tga"), &bytes);
            load_gpu_cached(&bytes, &path, ON, Some(&cache)).unwrap();
            names.push(Key::new(&bytes, &path, ON).unwrap().name().to_string());
        }
        let left: Vec<String> = s.entries().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(left.len(), 2, "{left:?}");
        assert!(names[..2].iter().all(|n| left.contains(n)), "{left:?}");
        assert!(cache.full.load(Ordering::Relaxed));
    }

    #[test]
    fn parallel_writers_stay_within_the_cap() {
        use rayon::prelude::*;
        let s = Scratch::new("parallel-cap");
        let (_, bytes) = pictures().into_iter().next().unwrap();
        let probe = s.cache(DEFAULT_CAP);
        let path = write(&s.0, "p0.tga", &bytes);
        load_gpu_cached(&bytes, &path, ON, Some(&probe)).unwrap();
        let one = s.entries().iter().map(|p| std::fs::metadata(p).unwrap().len()).sum::<u64>();
        std::fs::remove_dir_all(s.0.join("cache")).unwrap();
        let mut cache = s.cache(one * 2 + one / 2);
        cache.hold_after_admission = Some(Duration::from_millis(100));
        let files: Vec<PathBuf> = (0..8).map(|i| write(&s.0, &format!("q{i}.tga"), &bytes)).collect();
        rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap().install(|| {
            files.par_iter().for_each(|path| {
                load_gpu_cached(&bytes, path, ON, Some(&cache)).unwrap();
            });
        });
        let total: u64 = s.entries().iter().map(|p| std::fs::metadata(p).unwrap().len()).sum();
        assert!(total <= cache.cap, "{total} > {}", cache.cap);
        assert_eq!(s.entries().len(), 2);
    }

    #[test]
    fn workers_share_the_cache() {
        use rayon::prelude::*;
        let s = Scratch::new("shared");
        let cache = s.cache(DEFAULT_CAP);
        let files: Vec<(PathBuf, Vec<u8>)> = pictures().into_iter().map(|(n, b)| (write(&s.0, n, &b), b)).collect();
        let fresh: Vec<_> = files.iter().map(|(p, b)| load_gpu_bytes(b, p, ON).unwrap()).collect();
        (0..48).into_par_iter().for_each(|i| {
            let k = i % files.len();
            let (p, b) = &files[k];
            assert!(same(&fresh[k], &load_gpu_cached(b, p, ON, Some(&cache)).unwrap()));
        });
        assert_eq!(s.entries().len(), files.len() - 1);
        assert!(s.entries().iter().all(|p| p.extension().is_some_and(|x| x == ENTRY_EXT)));
    }
}
