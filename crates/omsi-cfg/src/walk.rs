use std::path::{Path, PathBuf};

use super::{candidates_in_dir, content_roots, owner_root};

pub(crate) fn pop_within(cur: &mut PathBuf, floor: Option<&Path>) {
    if floor.map_or(true, |f| cur.as_path() != f) {
        cur.pop();
    }
}

/// Case-insensitive walk of the components `rel` from `base` using the cached directory
/// listings only; None as soon as a component does not exist.
pub(crate) fn resolve_existing(base: &Path, rel: &[&str]) -> Option<PathBuf> {
    let floor = if rel.contains(&"..") {
        owner_root(base, &content_roots()).cloned()
    } else {
        None
    };
    resolve_within(base, rel, floor.as_deref())
}

fn resolve_within(base: &Path, rel: &[&str], floor: Option<&Path>) -> Option<PathBuf> {
    let mut cur = base.to_path_buf();
    for (i, comp) in rel.iter().enumerate() {
        if comp.is_empty() || *comp == "." {
            continue;
        }
        if *comp == ".." {
            pop_within(&mut cur, floor);
            continue;
        }
        let names = candidates_in_dir(&cur, comp);
        if names.len() < 2 {
            cur = cur.join(names.into_iter().next()?);
            continue;
        }
        // Two names differing in case alone, on a case-sensitive file system: the folder
        // meant is whichever one holds the rest of the path. An add-on unpacked beside the
        // original folders leaves a `Maps` with the new map next to the `maps` holding all
        // the others; both have to stay reachable.
        let rest = &rel[i + 1..];
        return names
            .iter()
            .find_map(|n| resolve_within(&cur.join(n), rest, floor));
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("omsi-cfg-walk-{tag}-{}", std::process::id()));
        let root = dir.join("content");
        std::fs::create_dir_all(root.join("Sceneryobjects").join(format!("Host-{tag}"))).unwrap();
        std::fs::create_dir_all(root.join("Sceneryobjects").join(format!("Other-{tag}"))).unwrap();
        std::fs::write(
            root.join("Sceneryobjects")
                .join(format!("Other-{tag}"))
                .join("mesh.o3d"),
            b"mesh",
        )
        .unwrap();
        std::fs::write(dir.join("secret.txt"), b"secret").unwrap();
        crate::add_content_root(root.clone());
        (dir, root)
    }

    #[test]
    fn dot_dot_stays_inside_the_content_root() {
        let (dir, root) = tree("dotdot");
        let base = root.join("Sceneryobjects").join("Host-dotdot");
        let ok = crate::resolve_path(&base, "..\\Other-dotdot\\MESH.o3d");
        assert_eq!(crate::vfs::read(&ok).unwrap(), b"mesh");
        let escaped = crate::resolve_path(&base, "..\\..\\..\\..\\..\\..\\..\\..\\secret.txt");
        assert!(escaped.starts_with(&root), "{}", escaped.display());
        assert!(crate::vfs::read(&escaped).is_err());
        assert!(resolve_existing(&base, &["..", "..", "..", "..", "secret.txt"]).is_none());
        assert_eq!(
            resolve_existing(&base, &["..", "Other-dotdot", "mesh.o3d"]),
            Some(
                root.join("Sceneryobjects")
                    .join("Other-dotdot")
                    .join("mesh.o3d")
            )
        );
        crate::remove_content_root(&root);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_drive_or_stream_component_is_dropped() {
        let (dir, root) = tree("drive");
        let base = root.join("Sceneryobjects");
        let p = crate::resolve_path(&base, "C:\\Other-drive\\mesh.o3d");
        assert!(p.starts_with(&root), "{}", p.display());
        assert_eq!(crate::vfs::read(&p).unwrap(), b"mesh");
        assert_eq!(
            crate::windows_components("a\\b:stream\\c.txt"),
            vec!["a", "c.txt"]
        );
        crate::remove_content_root(&root);
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_device_or_pipe_is_not_read() {
        let (dir, root) = tree("fifo");
        let fifo = root
            .join("Sceneryobjects")
            .join("Host-fifo")
            .join("pipe.o3d");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(crate::vfs::read(&fifo).is_err());
        assert!(crate::vfs::read(Path::new("/dev/zero")).is_err());
        assert!(crate::vfs::read(&root).is_err());
        crate::remove_content_root(&root);
        std::fs::remove_dir_all(dir).ok();
    }
}
