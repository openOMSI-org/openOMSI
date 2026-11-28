//! What the mods of the content folder override (#3606).
//!
//! A mod is mounted as a content root of its own (`mod_dirs`, `mount_mod_dirs`): its
//! `Vehicles/...` or `maps/...` is found as if it were installed, without copying it into the
//! content folder's tree. That answers half of the issue — the other half is knowing *what a
//! mod replaces*, which is what a player needs before switching one off, and what a map author
//! needs when two packs both ship the same spline.
//!
//! Two things come out of a mod folder here:
//!
//! * **an override of something that already exists** — the pack ships a file that the content
//!   folder (or the original installation) has as well, so that file is not added, it is
//!   replaced: a repaint pack's `Vehicles/MAN_SD202/Texture/repaint.cti`, a patched
//!   `sound.cfg`;
//! * **a collision between two mods** — two packs ship the same content-relative file, and
//!   only the one searched first is used. This is the real content of the issue's `.shared`
//!   idea: the file is in two places, one of them is dead weight, and the player should see it
//!   rather than having the pack deduplicated behind their back.
//!
//! Nothing here moves, deletes or deduplicates a file: it reports.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// More files than this in one mod folder and the rest of it is not looked at: a report is a
/// convenience, not a reason to walk a hundred thousand textures on a player's disk.
pub const DEFAULT_LIMIT: usize = 20_000;

/// One file a mod provides that is also somewhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    /// The file, content-relative with `/` (`Vehicles/MAN_SD202/Texture/repaint.cti`).
    pub rel: String,
    /// The mod that provides it: the folder's name in `Mods/`.
    pub by: String,
    /// Who else has it, highest priority first - another mod's name, `content` for the content
    /// folder, else the root's folder name (the original installation). Empty-handed entries
    /// are not reported.
    pub also: Vec<String>,
}

impl Override {
    /// Whether two mods ship the same file (the `.shared` case) rather than a mod replacing a
    /// file of the content folder's or of the installation's.
    pub fn between_mods(&self, mods: &[String]) -> bool {
        self.also.iter().any(|a| mods.iter().any(|m| m == a))
    }
}

/// The name a root is reported under: the mod folder's name, `content` for the content folder
/// itself, else the root's own folder name.
fn label(root: &Path, content: &Path) -> String {
    if root == content {
        return "content".to_string();
    }
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// The files of `root` that another root of `roots` also has, `by` being `root`'s label.
/// `mods` names the roots that are mods, so a file two packs both ship can be told from a file
/// a pack replaces.
fn scan(
    root: &Path,
    by: &str,
    roots: &[PathBuf],
    content: &Path,
    limit: usize,
    out: &mut Vec<Override>,
) -> usize {
    let mut seen = 0usize;
    let mut stack = vec![root.to_path_buf()];
    let mut files: Vec<String> = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let Ok(rel) = p.strip_prefix(root) else {
                continue;
            };
            let rel: String = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.push(rel);
            seen += 1;
            if seen >= limit {
                break;
            }
        }
        if seen >= limit {
            break;
        }
    }
    for rel in files {
        let comps = crate::windows_components(&rel);
        let mut also: Vec<String> = Vec::new();
        for r in roots {
            if r == root {
                continue;
            }
            if crate::resolve_existing(r, &comps).is_some() {
                let l = label(r, content);
                if !also.contains(&l) {
                    also.push(l);
                }
            }
        }
        if !also.is_empty() {
            out.push(Override {
                rel,
                by: by.to_string(),
                also,
            });
        }
    }
    seen
}

/// What the mods of `<content>/Mods` override, given the content roots in the order they are
/// searched (`content_roots`, after `mount_mod_dirs`) and the mod folders (`mod_dirs`).
///
/// The roots are passed in rather than read from the global list, so a caller can ask about a
/// folder of mods without mounting it, and a test can fix the order.
pub fn overrides_in(
    content: &Path,
    roots: &[PathBuf],
    mods: &[PathBuf],
    limit: usize,
) -> Vec<Override> {
    let mut out: Vec<Override> = Vec::new();
    let mut names: BTreeSet<String> = BTreeSet::new();
    for m in mods {
        let by = label(m, content);
        names.insert(by.clone());
        scan(m, &by, roots, content, limit, &mut out);
    }
    // the highest-priority root wins, so the entry that matters is that one's: keep them in
    // the roots' order, a mod that is searched later overriding what an earlier one replaced
    out.sort_by(|a, b| a.rel.cmp(&b.rel).then_with(|| a.by.cmp(&b.by)));
    out
}

/// The same, for the content folder of a running game: the roots as `mount_mod_dirs` left
/// them, the mods of `Mods/`.
pub fn overrides(content: &Path, limit: usize) -> Vec<Override> {
    let mods = crate::mod_dirs(content);
    let mut roots = mods.clone();
    for r in crate::content_roots() {
        if !roots.contains(&r) {
            roots.push(r);
        }
    }
    if !roots.contains(&content.to_path_buf()) {
        roots.push(content.to_path_buf());
    }
    overrides_in(content, &roots, &mods, limit)
}

/// The report as text: a line per file, the mod it comes from and who else has it.
pub fn report(entries: &[Override], mods: &[String]) -> String {
    let mut out =
        String::from("# openOMSI mods: what they override\n# file\tprovided by\talso present in\n");
    for e in entries {
        let kind = if e.between_mods(mods) {
            " (two mods)"
        } else {
            ""
        };
        out.push_str(&format!(
            "{}\t{}\t{}{}\n",
            e.rel,
            e.by,
            e.also.join(", "),
            kind
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(root: &Path, rel: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"x").unwrap();
    }

    struct Tree {
        dir: PathBuf,
        content: PathBuf,
        a: PathBuf,
        b: PathBuf,
    }

    fn tree(tag: &str) -> Tree {
        let dir =
            std::env::temp_dir().join(format!("openomsi-overrides-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let content = dir.join("content");
        let a = content.join("Mods/PackA");
        let b = content.join("Mods/PackB");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        Tree { dir, content, a, b }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    #[test]
    fn a_file_two_packs_ship_is_reported_against_both() {
        let t = tree("two");
        w(&t.a, "Splines/rail.sli");
        w(&t.b, "Splines/rail.sli");
        // (PackA is searched before PackB: `mount_mod_dirs` puts a mod before the content
        // folder, and the folders in name order)
        let roots = vec![t.a.clone(), t.b.clone(), t.content.clone()];
        let mods = vec![t.a.clone(), t.b.clone()];
        let e = overrides_in(&t.content, &roots, &mods, DEFAULT_LIMIT);
        let rail: Vec<&Override> = e.iter().filter(|x| x.rel == "Splines/rail.sli").collect();
        assert_eq!(rail.len(), 2, "each pack reports the other: {e:?}");
        assert_eq!(
            (rail[0].by.as_str(), rail[0].also.clone()),
            ("PackA", vec!["PackB".to_string()])
        );
        assert_eq!(
            (rail[1].by.as_str(), rail[1].also.clone()),
            ("PackB", vec!["PackA".to_string()])
        );
        let names = vec!["PackA".to_string(), "PackB".to_string()];
        assert!(rail[0].between_mods(&names) && rail[1].between_mods(&names));
        // (a file only one pack ships is not an override)
        assert!(!e.iter().any(|x| x.rel.contains("only")));
    }

    #[test]
    fn a_pack_replacing_a_file_of_the_content_folder_says_so() {
        let t = tree("replace");
        w(&t.content, "Vehicles/MAN_SD202/Texture/repaint.cti");
        w(&t.a, "Vehicles/MAN_SD202/Texture/repaint.cti");
        w(&t.a, "Vehicles/MAN_SD202/Texture/mine.cti");
        let roots = vec![t.a.clone(), t.content.clone()];
        let e = overrides_in(
            &t.content,
            &roots,
            std::slice::from_ref(&t.a),
            DEFAULT_LIMIT,
        );
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].rel, "Vehicles/MAN_SD202/Texture/repaint.cti");
        assert_eq!(e[0].also, vec!["content".to_string()]);
        assert!(
            !e[0].between_mods(&["PackA".to_string()]),
            "not a collision between two packs"
        );
    }

    #[test]
    fn an_installation_file_is_named_by_its_folder() {
        let t = tree("stock");
        let stock = t.dir.join("OMSI 2");
        std::fs::create_dir_all(&stock).unwrap();
        w(&stock, "Splines/stock.sli");
        w(&t.content, "Splines/stock.sli");
        w(&t.a, "Splines/stock.sli");
        let roots = vec![t.a.clone(), t.content.clone(), stock.clone()];
        let e = overrides_in(
            &t.content,
            &roots,
            std::slice::from_ref(&t.a),
            DEFAULT_LIMIT,
        );
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(
            e[0].also,
            vec!["content".to_string(), "OMSI 2".to_string()],
            "the roots in their order"
        );
    }

    #[test]
    fn the_walk_stops_at_the_limit() {
        let t = tree("limit");
        for i in 0..20 {
            w(&t.a, &format!("Vehicles/X/f{i}.txt"));
            w(&t.content, &format!("Vehicles/X/f{i}.txt"));
        }
        let roots = vec![t.a.clone(), t.content.clone()];
        let all = overrides_in(
            &t.content,
            &roots,
            std::slice::from_ref(&t.a),
            DEFAULT_LIMIT,
        );
        assert_eq!(all.len(), 20);
        let few = overrides_in(&t.content, &roots, std::slice::from_ref(&t.a), 5);
        assert!(few.len() <= 5, "{}", few.len());
    }

    #[test]
    fn the_mod_folders_are_found_as_the_game_finds_them() {
        let t = tree("roots");
        w(&t.a, "Vehicles/ModBus/bus.bus");
        // (a pack that kept its own folder inside)
        w(&t.content, "Mods/PackC/PackC/maps/M/global.cfg");
        // (the launcher's own folders, and a pack's readme, are not mods)
        w(&t.content, "Mods/disabled/Off/Vehicles/Off/off.bus");
        w(&t.content, "Mods/waiting/Vehicles/W/Vehicles/w.bus");
        std::fs::write(t.content.join("Mods/README.txt"), b"readme").unwrap();
        let roots = crate::mod_dirs(&t.content);
        let names: Vec<String> = roots
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["PackA", "PackB", "PackC"],
            "name order, the inner folder of PackC, nothing of the launcher's"
        );
        assert!(roots[2].ends_with("Mods/PackC/PackC"));
    }

    #[test]
    fn the_report_says_which_files_two_packs_share() {
        let entries = vec![
            Override {
                rel: "Splines/rail.sli".into(),
                by: "PackA".into(),
                also: vec!["PackB".into()],
            },
            Override {
                rel: "Vehicles/MAN_SD202/Texture/repaint.cti".into(),
                by: "PackA".into(),
                also: vec!["content".into()],
            },
        ];
        let text = super::report(&entries, &["PackA".to_string(), "PackB".to_string()]);
        assert!(
            text.contains("Splines/rail.sli\tPackA\tPackB (two mods)"),
            "{text}"
        );
        assert!(text.contains("repaint.cti\tPackA\tcontent\n"), "{text}");
    }
}
