//! Which files form executable consent, as opposed to mutable plugin data.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) fn snapshot(
    entry: &Path,
    root: &Path,
    helpers: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, String> {
    let shared_lua = entry
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lua"))
        && !entry
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("main.lua"));
    let mut files = BTreeMap::new();
    let mut pending = vec![(root.to_path_buf(), false)];
    let mut directories = 0usize;
    let mut total = 0u64;
    while let Some((dir, data)) = pending.pop() {
        directories += 1;
        if directories > 4096 {
            return Err("plugin grant exceeds 4096 directories".into());
        }
        for item in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let item = item.map_err(|e| e.to_string())?;
            let path = item.path();
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("plugin paths must be UTF-8")?
                .replace('\\', "/");
            let leaf = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            let kind = item.file_type().map_err(|e| e.to_string())?;
            let directory = kind.is_dir() || (kind.is_symlink() && path.is_dir());
            let required = helpers.iter().any(|h| same_name(h, &name)) || path == entry;
            let lua = leaf.ends_with(".lua") && !leaf.ends_with(".save.lua");
            let native = leaf.ends_with(".dll")
                || leaf.ends_with(".so")
                || leaf.contains(".so.")
                || leaf.ends_with(".dylib");
            let retained =
                required || lua || native || (!shared_lua && !data && !leaf.ends_with(".save.lua"));
            if !directory && !retained {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() || (!meta.is_file() && !meta.is_dir()) {
                return Err("plugin grants cannot include symlinks or special files".into());
            }
            if !path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(root)
            {
                return Err("plugin folder contains a path escaping its root".into());
            }
            if directory {
                // Keep walking data directories: data/module.lua remains require-able.
                let data =
                    data || (!name.contains('/') && (leaf == "data" || leaf.ends_with(".data")));
                pending.push((path, data));
            } else {
                total = total
                    .checked_add(meta.len())
                    .ok_or("plugin grant size overflow")?;
                if files.len() >= 4096 || total > 256 * 1024 * 1024 {
                    return Err("plugin grant exceeds 4096 files or 256 MiB".into());
                }
                files.insert(name, super::grants::hash(&path)?);
            }
        }
    }
    Ok(files)
}

fn same_name(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}
