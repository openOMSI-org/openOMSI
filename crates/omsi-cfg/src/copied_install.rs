use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

pub enum Look {
    Root(PathBuf),
    Waiting,
    Missing(Vec<String>),
}

struct Watch {
    signature: Vec<(usize, u128)>,
    since: Instant,
    verdict: Option<Option<PathBuf>>,
    missing: Vec<String>,
}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

const QUIET_SECS: f32 = 10.0;

pub fn look(given: Option<PathBuf>, docs: impl FnOnce() -> Option<PathBuf>) -> Look {
    if let Some(p) = given {
        crate::content_changed();
        let missing = crate::missing_original_essentials(&p);
        return if missing.is_empty() {
            Look::Root(p)
        } else {
            Look::Missing(missing.into_iter().take(6).collect())
        };
    }
    let Some(docs) = docs() else {
        return Look::Waiting;
    };
    let cands = candidates(&docs);
    if cands.is_empty() {
        *WATCH.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return Look::Waiting;
    }
    let signature: Vec<(usize, u128)> = cands.iter().map(|p| tree_signature(p)).collect();
    let mut w = WATCH.lock().unwrap_or_else(|e| e.into_inner());
    let watch = match w.as_mut() {
        Some(x) if x.signature == signature => x,
        _ => {
            *w = Some(Watch {
                signature,
                since: Instant::now(),
                verdict: None,
                missing: Vec::new(),
            });
            return Look::Waiting;
        }
    };
    if watch.since.elapsed().as_secs_f32() < QUIET_SECS {
        return Look::Waiting;
    }
    if watch.verdict.is_none() {
        crate::content_changed();
        let checked: Vec<(PathBuf, Vec<String>)> = cands
            .iter()
            .map(|p| (p.clone(), crate::missing_original_essentials(p)))
            .collect();
        let found = checked
            .iter()
            .find(|(_, m)| m.is_empty())
            .map(|(p, _)| p.clone());
        watch.missing = checked
            .into_iter()
            .map(|(_, m)| m)
            .min_by_key(|m| m.len())
            .unwrap_or_default()
            .into_iter()
            .take(6)
            .collect();
        watch.verdict = Some(found);
    }
    match watch.verdict.clone().flatten() {
        Some(p) => Look::Root(p),
        None => Look::Missing(watch.missing.clone()),
    }
}

fn has_exe(p: &Path) -> bool {
    std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Omsi.exe")
        })
}

fn looks_like_game(p: &Path) -> bool {
    let named = p.file_name().is_some_and(|f| {
        crate::install_search::NAMES
            .iter()
            .any(|n| f.to_string_lossy().eq_ignore_ascii_case(n))
    });
    p.is_dir() && (named || has_exe(p))
}

fn subfolders(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

fn candidates(docs: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in subfolders(docs) {
        if looks_like_game(&p) {
            out.push(p.clone());
        }
        out.extend(subfolders(&p).into_iter().filter(|q| looks_like_game(q)));
    }
    out
}

fn tree_signature(root: &Path) -> (usize, u128) {
    let (mut entries, mut latest) = (0usize, 0u128);
    let mut dirs = vec![root.to_path_buf()];
    while let Some(d) = dirs.pop() {
        if let Ok(t) = std::fs::metadata(&d).and_then(|m| m.modified()) {
            latest = latest.max(
                t.duration_since(std::time::UNIX_EPOCH)
                    .map(|x| x.as_millis())
                    .unwrap_or(0),
            );
        }
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            entries += 1;
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                dirs.push(e.path());
            }
        }
    }
    (entries, latest)
}
