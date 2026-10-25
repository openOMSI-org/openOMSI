//! Admission hints only: the worker must still verify canonical paths and all hashes.
use std::path::Path;

pub(super) fn matches(left: &Path, right: &Path) -> bool {
    #[cfg(not(windows))]
    {
        left == right
    }
    #[cfg(windows)]
    {
        let mut left = left.components();
        let mut right = right.components();
        loop {
            match (left.next(), right.next()) {
                (None, None) => return true,
                (Some(a), Some(b)) => {
                    // A DOS alias cannot be resolved lexically (RUNNER~1 need not
                    // spell the canonical directory). Treat it as ambiguous here,
                    // without adding filesystem I/O to plugin load/reload. A false
                    // positive merely queues a request; helper() rejects the wrong
                    // canonical entry/folder on the preparation worker.
                    if !a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
                        && !short_name(a)
                        && !short_name(b)
                    {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
}

#[cfg(windows)]
fn short_name(component: std::path::Component<'_>) -> bool {
    let std::path::Component::Normal(name) = component else {
        return false;
    };
    name.to_str().is_some_and(|name| {
        name.split_once('~').is_some_and(|(prefix, suffix)| {
            let number = suffix.split('.').next().unwrap_or_default();
            !prefix.is_empty() && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
        })
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn dos_aliases_and_case_do_not_hide_candidates() {
        for (long, short) in [
            (
                r"C:\Users\runneradmin\AppData\Local\Temp\plugin.lua",
                r"C:\Users\RUNNER~1\AppData\Local\Temp\plugin.lua",
            ),
            (
                r"C:\Plugins\Long Plugin Folder\main.lua",
                r"c:\plugins\LONGPL~1\MAIN.LUA",
            ),
            (
                r"C:\Plugins\long plugin entry.lua",
                r"C:\Plugins\LONGPL~1.LUA",
            ),
        ] {
            assert!(matches(Path::new(long), Path::new(short)));
            assert!(matches(Path::new(short), Path::new(long)));
        }
        assert!(!matches(
            Path::new(r"C:\Plugins\a.lua"),
            Path::new(r"C:\Plugins\b.lua")
        ));
        assert!(!matches(
            Path::new(r"C:\Plugins\main.lua"),
            Path::new(r"D:\Plugins\main.lua")
        ));
        assert!(!matches(
            Path::new(r"C:\Plugins\main.lua"),
            Path::new(r"C:\Plugins\extra\main.lua")
        ));
    }
}
