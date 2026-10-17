//! `--pack-map <map> --pack-to <folder or .zip>`: a map with everything it needs, for a
//! server that does not have this installation - the map's folder, the objects, splines,
//! vehicles and people its files name (and the folders those name in turn), its ground
//! pictures and the fonts its displays write with (`lan_mods::session_files`, the list the
//! host's mods are made of, here without leaving the installation out).
//!
//! Into a folder - the server's content folder - every file goes as a hard link when it can
//! (the same disk: no room taken twice), else as a copy; a file already there in the same
//! size and time is left. Into a `.zip`, for a server on another machine (unpacked into its
//! content folder). The names are written as the files are called on this disk, so that a
//! server on Linux, which tells `Vehicles` from `vehicles`, finds one folder of each.
//! What the map names that is nowhere here is listed: a server and its players lack it too.
//!
//! In the server's content folder the map is no stock content any more (`lan_mods`): a
//! player who joins without it gets it from the server, and one who has it in the same
//! version fetches nothing (the files are compared by their SHA-256).

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

/// `map` as the game names it (`maps/<name>/global.cfg`): from the map's name, its folder or
/// its `global.cfg`.
pub fn map_cfg(map: &str) -> String {
    let m = map.trim().trim_matches('"').replace('\\', "/");
    let m = m.trim_end_matches('/');
    let lower = m.to_ascii_lowercase();
    if lower.ends_with(".cfg") {
        return m.to_string();
    }
    if lower.starts_with("maps/") {
        return format!("{m}/global.cfg");
    }
    format!("maps/{m}/global.cfg")
}

/// What a pack came to.
#[derive(Debug, Default)]
pub struct Packed {
    pub files: usize,
    pub bytes: u64,
    pub linked: usize,
    pub copied: usize,
    pub kept: usize,
    pub missing: Vec<String>,
}

/// Pack `map` (see `map_cfg`) to `to`: a folder, or a `.zip` file.
pub fn pack(map: &str, to: &Path) -> Result<Packed> {
    let cfg = map_cfg(map);
    if omsi_cfg::find_in_roots(&cfg).is_none() {
        bail!("no map {cfg} here (the name of its folder under maps, e.g. \"Brasil Viagem Xtreme\")");
    }
    // (into one of the folders it is read from, a file would be linked onto itself)
    let target = std::path::absolute(to).unwrap_or_else(|_| to.to_path_buf());
    if let Some(r) = omsi_cfg::content_roots().into_iter().find(|r| target.starts_with(r) || r.starts_with(&target)) {
        bail!("{} is the content folder {} or in it: pack into the server's own folder", to.display(), r.display());
    }
    let t0 = Instant::now();
    eprintln!("Listing what {cfg} needs...");
    let (files, missing) = crate::mods::session_files(&cfg, None, None, None);
    let total: u64 = files.iter().filter_map(|(_, p)| omsi_cfg::vfs::file_size(p)).sum();
    eprintln!("{} files, {:.1} MB (listed in {:.1} s)", files.len(), total as f64 / 1e6, t0.elapsed().as_secs_f64());
    let zip = to.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let mut out = Packed { files: files.len(), bytes: total, missing, ..Default::default() };
    let mut said = Instant::now();
    let mut done = 0u64;
    let mut writer = if zip {
        if let Some(d) = to.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d)?;
        }
        Some(zip::ZipWriter::new(std::fs::File::create(to).with_context(|| to.display().to_string())?))
    } else {
        std::fs::create_dir_all(to).with_context(|| to.display().to_string())?;
        None
    };
    for (rel, src) in &files {
        let name = disk_name(src).unwrap_or_else(|| rel.clone());
        match writer.as_mut() {
            Some(z) => {
                let data = omsi_cfg::vfs::read(src).with_context(|| src.display().to_string())?;
                let packed_already = [".dds", ".png", ".jpg", ".jpeg", ".ogg", ".zip", ".rar", ".7z"].iter().any(|e| name.to_ascii_lowercase().ends_with(e));
                let method = if packed_already { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
                let options = zip::write::SimpleFileOptions::default().compression_method(method).large_file(data.len() as u64 >= 1 << 32);
                z.start_file(name.as_str(), options)?;
                z.write_all(&data)?;
                out.copied += 1;
                done += data.len() as u64;
            }
            None => {
                let dst = name.split('/').fold(to.to_path_buf(), |p, c| p.join(c));
                done += place(src, &dst, &mut out).with_context(|| format!("{} -> {}", src.display(), dst.display()))?;
            }
        }
        if said.elapsed().as_secs() >= 2 {
            said = Instant::now();
            eprintln!("  {:.0}% ({:.0} of {:.0} MB)", done as f64 * 100.0 / total.max(1) as f64, done as f64 / 1e6, total as f64 / 1e6);
        }
    }
    if let Some(z) = writer {
        z.finish()?;
    } else {
        write_report(to, &cfg, &out)?;
    }
    Ok(out)
}

/// A file's name as it is called on the disk, relative to the content root it is in.
fn disk_name(path: &Path) -> Option<String> {
    let roots = omsi_cfg::content_roots();
    let r = roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.components().count())?;
    let rest: Vec<String> = path.strip_prefix(r).ok()?.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    (!rest.is_empty()).then(|| rest.join("/"))
}

/// `src` at `dst`: left when the same file is there (size and time), else a hard link, else
/// a copy (a file in a mounted archive is always read out). Its size.
fn place(src: &Path, dst: &Path, out: &mut Packed) -> Result<u64> {
    let meta = std::fs::metadata(src).ok();
    if let (Some(m), Ok(d)) = (meta.as_ref(), std::fs::metadata(dst)) {
        if m.len() == d.len() && m.modified().ok() == d.modified().ok() {
            out.kept += 1;
            return Ok(m.len());
        }
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(dst);
    if meta.is_some() && std::fs::hard_link(src, dst).is_ok() {
        out.linked += 1;
        return Ok(meta.map(|m| m.len()).unwrap_or(0));
    }
    let data = omsi_cfg::vfs::read(src)?;
    std::fs::write(dst, &data)?;
    if let Some(t) = meta.and_then(|m| m.modified().ok()) {
        let _ = std::fs::File::options().write(true).open(dst).and_then(|f| f.set_modified(t));
    }
    out.copied += 1;
    Ok(data.len() as u64)
}

/// What was packed, beside it (`openomsi-map-pack.txt`): the map, the sizes, what is missing.
fn write_report(to: &Path, cfg: &str, p: &Packed) -> Result<()> {
    let mut text = format!(
        "openOMSI map pack\nmap = {cfg}\nfiles = {}\nbytes = {}\nlinked = {}\ncopied = {}\nkept = {}\n",
        p.files, p.bytes, p.linked, p.copied, p.kept
    );
    if !p.missing.is_empty() {
        text.push_str(&format!("\n# named by the map but not found on the machine that packed it ({}):\n", p.missing.len()));
        for m in &p.missing {
            text.push_str(m);
            text.push('\n');
        }
    }
    let name = format!("openomsi-map-pack-{}.txt", cfg.split('/').nth(1).unwrap_or("map").replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_"));
    std::fs::write(to.join(name), text)?;
    Ok(())
}

/// `--pack-map <map> --pack-to <to>`: pack it and say what was done on the console.
pub fn run(map: &str, to: &Path) -> Result<()> {
    let p = pack(map, to)?;
    eprintln!(
        "Packed {}: {} files, {:.1} MB ({} linked, {} copied, {} there already) into {}",
        map_cfg(map),
        p.files,
        p.bytes as f64 / 1e6,
        p.linked,
        p.copied,
        p.kept,
        to.display()
    );
    if !p.missing.is_empty() {
        eprintln!("{} files the map names are not on this machine (the server and its players lack them too):", p.missing.len());
        for m in p.missing.iter().take(20) {
            eprintln!("  {m}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_is_named_by_its_folder_its_path_or_its_global_cfg() {
        assert_eq!(map_cfg("Brasil Viagem Xtreme"), "maps/Brasil Viagem Xtreme/global.cfg");
        assert_eq!(map_cfg("maps/Grundorf"), "maps/Grundorf/global.cfg");
        assert_eq!(map_cfg("maps\\Grundorf\\"), "maps/Grundorf/global.cfg");
        assert_eq!(map_cfg("\"maps/Grundorf/global.cfg\""), "maps/Grundorf/global.cfg");
    }
}
