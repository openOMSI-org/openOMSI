use std::path::Path;

/// Unpack `zip` into `to` (a fresh folder), with the files' Unix modes and links.
pub fn unpack(zip: &Path, to: &Path) -> anyhow::Result<()> {
    let _ = std::fs::remove_dir_all(to);
    std::fs::create_dir_all(to)?;
    let root = to.canonicalize()?;
    let mut a = zip::ZipArchive::new(std::fs::File::open(zip)?)?;
    for i in 0..a.len() {
        let mut e = a.by_index(i)?;
        let Some(rel) = e.enclosed_name() else { continue };
        // (macOS' resource forks, which ditto keeps beside the files)
        if rel.components().next().map(|c| c.as_os_str() == "__MACOSX").unwrap_or(false) || rel.file_name().map(|n| n.to_string_lossy().starts_with("._")).unwrap_or(false) {
            continue;
        }
        if rel.ancestors().any(|p| !p.as_os_str().is_empty() && to.join(p).is_symlink()) {
            continue;
        }
        let out = to.join(&rel);
        if e.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        let Some(parent) = out.parent() else { continue };
        std::fs::create_dir_all(parent)?;
        if !parent.canonicalize()?.starts_with(&root) {
            continue;
        }
        let mode = e.unix_mode();
        #[cfg(unix)]
        if let Some(m) = mode {
            if m & 0o170000 == 0o120000 {
                let mut target = String::new();
                std::io::Read::read_to_string(&mut e, &mut target)?;
                if link_points_down(Path::new(&target)) {
                    std::os::unix::fs::symlink(target, &out)?;
                }
                continue;
            }
        }
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut e, &mut f)?;
        drop(f);
        #[cfg(unix)]
        if let Some(m) = mode {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(m & 0o777))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
    }
    Ok(())
}

// Without `..` a link only reaches below its own folder, so no chain of links can climb out.
#[cfg(unix)]
fn link_points_down(target: &Path) -> bool {
    use std::path::Component;
    !target.as_os_str().is_empty() && target.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn links_and_setuid_bits_do_not_leave_the_unpacked_folder() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("omsi-unpack-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let zip = root.join("pack.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.add_symlink("App.app/Contents/Current", "Versions/A", o).unwrap();
            z.add_symlink("App.app/escape", "../../outside", o).unwrap();
            z.add_symlink("App.app/absolute", "/etc", o).unwrap();
            z.add_symlink("a/b", "..", o).unwrap();
            z.add_symlink("a/b/c", "..", o).unwrap();
            z.start_file("a/b/c/escaped", o).unwrap();
            z.write_all(b"x").unwrap();
            z.add_symlink("x", "a/b/..", o).unwrap();
            z.add_symlink("into", "sub", o).unwrap();
            z.start_file("into/through", o).unwrap();
            z.write_all(b"x").unwrap();
            z.start_file("App.app/run", o.unix_permissions(0o4755)).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        let to = root.join("out");
        unpack(&zip, &to).unwrap();
        assert_eq!(std::fs::read_link(to.join("App.app/Contents/Current")).unwrap(), Path::new("Versions/A"));
        assert_eq!(std::fs::read_link(to.join("into")).unwrap(), Path::new("sub"));
        for refused in ["App.app/escape", "App.app/absolute", "a/b", "x"] {
            assert!(!to.join(refused).is_symlink(), "{refused}");
        }
        assert!(!root.join("escaped").exists() && !to.join("escaped").exists() && !to.join("a/escaped").exists());
        assert!(!to.join("sub").exists());
        assert_eq!(std::fs::metadata(to.join("App.app/run")).unwrap().permissions().mode() & 0o7777, 0o755);
        let _ = std::fs::remove_dir_all(&root);
    }
}
