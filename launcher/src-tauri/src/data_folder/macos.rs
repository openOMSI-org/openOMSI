use super::normalize;
use objc2::{MainThreadMarker, rc::Retained, runtime::Bool};
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
use objc2_foundation::{
    NSData, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions,
    NSUserDefaults,
};
use omsi_launcher_lib::i18n::{tr, trf};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const BOOKMARKS: &str = "openomsi.dataFolderBookmarks.v1";
// Keep source AND destination grants alive throughout migration and for this process.
static ACCESS: Mutex<BTreeMap<String, Retained<NSURL>>> = Mutex::new(BTreeMap::new());

fn stored() -> Result<BTreeMap<String, Vec<u8>>, String> {
    let defaults = NSUserDefaults::standardUserDefaults();
    match defaults.dataForKey(&NSString::from_str(BOOKMARKS)) {
        Some(data) => serde_json::from_slice(&data.to_vec())
            .map_err(|e| trf("Cannot read data folder bookmarks: {error}", &[("error", &e)])),
        None => Ok(BTreeMap::new()),
    }
}

fn save(bookmarks: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let bytes = serde_json::to_vec(bookmarks)
        .map_err(|e| trf("Cannot save data folder bookmarks: {error}", &[("error", &e)]))?;
    let data = NSData::with_bytes(&bytes);
    // NSData is a property-list value. Preferences stay outside the movable .openomsi.
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(data.as_ref()), &NSString::from_str(BOOKMARKS))
    };
    Ok(())
}

fn bookmark(url: &NSURL) -> Result<Vec<u8>, String> {
    url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
        NSURLBookmarkCreationOptions::WithSecurityScope,
        None,
        None,
    )
    .map(|data| data.to_vec())
    .map_err(|e| trf("Cannot preserve data folder permission: {error}", &[("error", &e)]))
}

fn activate(bytes: &[u8]) -> Result<(Retained<NSURL>, bool), String> {
    let mut stale = Bool::NO;
    // The stale out-pointer remains valid for this synchronous Foundation call.
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &NSData::with_bytes(bytes),
            NSURLBookmarkResolutionOptions::WithSecurityScope
                | NSURLBookmarkResolutionOptions::WithoutUI,
            None,
            &mut stale,
        )
    }
    .map_err(|e| trf("Cannot restore data folder permission: {error}", &[("error", &e)]))?;
    let path = url
        .to_file_path()
        .ok_or_else(|| tr("The data folder bookmark is not a file URL"))?;
    let key = path
        .to_str()
        .ok_or_else(|| tr("The data folder path must be valid UTF-8"))?
        .to_owned();
    let mut access = ACCESS
        .lock()
        .map_err(|e| trf("Cannot lock data folder permissions: {error}", &[("error", &e)]))?;
    if !access.contains_key(&key) {
        // A non-sandboxed build may already have access and return false. Never treat
        // the panel itself as proof of durable permission: resolving a bookmark and
        // starting its scope is required; check actual access if no scope was started.
        let started = unsafe { url.startAccessingSecurityScopedResource() };
        if !started {
            std::fs::read_dir(&path).map_err(|e| {
                trf(
                    "Cannot access {path}: {error}",
                    &[("path", &path.display()), ("error", &e)],
                )
            })?;
        } else {
            access.insert(key, url.clone());
        }
    }
    Ok((url, stale.as_bool()))
}

// Resolve existing aliases without requiring a not-yet-created .openomsi directory.
fn comparable(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        path.parent()
            .and_then(|parent| parent.canonicalize().ok())
            .zip(path.file_name())
            .map(|(parent, name)| parent.join(name))
            .unwrap_or_else(|| path.to_path_buf())
    })
}

pub fn pick(requested: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| tr("The data folder picker must run on the main thread"))?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    panel.setCanCreateDirectories(true);
    panel.setShowsHiddenFiles(true);
    panel.setTitle(Some(&NSString::from_str(&tr(
        "Choose .openomsi or its parent folder",
    ))));
    if let Some(target) = &requested {
        let message = trf(
            "Allow openOMSI to use {path}. Select that existing .openomsi folder or its direct parent.",
            &[("path", &target.display())],
        );
        panel.setMessage(Some(&NSString::from_str(&message)));
        if let Some(parent) = target.parent() {
            panel.setDirectoryURL(NSURL::from_directory_path(parent).as_deref());
        }
    }
    // Always present the panel, even if this target was authorized previously. Keep
    // its original URL: rebuilding it from a path can discard the granted scope.
    if panel.runModal() != NSModalResponseOK {
        return Ok(None);
    }
    let url = panel.URL().ok_or_else(|| tr("No data folder was selected"))?;
    let selected = url
        .to_file_path()
        .ok_or_else(|| tr("The selected folder is not a file URL"))?;
    let target = match requested {
        Some(target) => {
            let selected = comparable(&selected);
            if selected != comparable(&target)
                && target
                    .parent()
                    .is_none_or(|parent| selected != comparable(parent))
            {
                return Err(trf(
                    "Select {path} or its direct parent folder",
                    &[("path", &target.display())],
                ));
            }
            target
        }
        None => normalize(&selected)?,
    };
    let mut bookmarks = stored()?;
    let started = unsafe { url.startAccessingSecurityScopedResource() };
    let bytes = bookmark(&url);
    if started {
        unsafe { url.stopAccessingSecurityScopedResource() };
    }
    let bytes = bytes?;
    let (active, _) = activate(&bytes)?;
    let selected = active
        .to_file_path()
        .ok_or_else(|| tr("The selected folder is not a file URL"))?;
    bookmarks.insert(selected.to_string_lossy().into_owned(), bytes);
    save(&bookmarks)?;
    Ok(Some(target))
}

pub fn restore() -> Result<(), String> {
    let mut bookmarks = stored()?;
    let mut errors = Vec::new();
    let mut changed = false;
    // One revoked grant must not prevent other source/destination grants restoring.
    for bytes in bookmarks.values_mut() {
        match activate(bytes) {
            Ok((url, true)) => match bookmark(&url) {
                Ok(fresh) => {
                    *bytes = fresh;
                    changed = true;
                }
                Err(e) => errors.push(e),
            },
            Ok(_) => {}
            Err(e) => errors.push(e),
        }
    }
    if changed {
        save(&bookmarks)?;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
