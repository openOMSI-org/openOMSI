use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[path = "candidates.rs"]
mod candidates;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessGrants {
    pub grants: Vec<ProcessGrant>,
    #[cfg(test)]
    #[serde(skip)]
    pub(super) before_verify: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessGrant {
    pub entry: PathBuf,
    pub sha256: String,
    pub folder: PathBuf,
    pub files: BTreeMap<String, String>,
    pub helpers: BTreeMap<String, String>,
}

impl ProcessGrant {
    pub fn capture(entry: &Path, folder: &Path, helpers: &[String]) -> Result<Self, String> {
        if !entry.is_absolute() || !folder.is_absolute() {
            return Err("grant paths must be absolute".into());
        }
        let entry = entry.canonicalize().map_err(|e| e.to_string())?;
        let folder = folder.canonicalize().map_err(|e| e.to_string())?;
        let names: std::collections::BTreeSet<String> = helpers.iter().map(|name| super::super::paths::clean(name)).collect::<Result<_, _>>()?;
        let files = super::content::snapshot(&entry, &folder, &names)?;
        let mut allowed = BTreeMap::new();
        for name in helpers {
            let name = super::super::paths::clean(name)?;
            let hash = files
                .get(&name)
                .ok_or("helper must be a file inside the plugin folder")?;
            #[cfg(windows)]
            if !name.to_ascii_lowercase().ends_with(".exe") {
                return Err("Windows helpers must be .exe files, not shell scripts".into());
            }
            allowed.insert(name, hash.clone());
        }
        Ok(Self {
            sha256: hash(&entry)?,
            entry,
            folder,
            files,
            helpers: allowed,
        })
    }

    fn matches(&self, entry: &Path, folder: Option<&Path>) -> bool {
        let Some(folder) = folder.or_else(|| entry.parent()) else {
            return false;
        };
        entry.canonicalize().ok().as_ref() == Some(&self.entry)
            && folder.canonicalize().ok().as_ref() == Some(&self.folder)
            && hash(entry).is_ok_and(|h| h == self.sha256)
            && super::content::snapshot(&self.entry, &self.folder, &self.helpers.keys().cloned().collect()).is_ok_and(|files| files == self.files)
    }
}

impl ProcessGrants {
    pub fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            grants: paths
                .into_iter()
                .filter_map(|entry| ProcessGrant::capture(&entry, entry.parent()?, &[]).ok())
                .collect(),
            ..Default::default()
        }
    }

    pub fn from_env() -> Self {
        let result = (|| {
            let path = PathBuf::from(omsi_cfg::flags::OMSI_PLUGIN_PROCESS_ALLOW.live_os()?);
            if !path.is_absolute() || std::fs::metadata(&path).ok()?.len() > 1024 * 1024 {
                return None;
            }
            let bytes = std::fs::read(path).ok()?;
            serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).ok()
        })();
        result.unwrap_or_default()
    }

    pub(crate) fn has_candidate(&self, entry: &Path, folder: Option<&Path>) -> bool {
        fn lexical(path: &Path) -> Option<PathBuf> {
            let absolute = std::path::absolute(path).ok()?;
            #[cfg(windows)]
            {
                let text = absolute.to_str()?;
                return Some(PathBuf::from(
                    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
                        format!(r"\\{unc}")
                    } else {
                        text.strip_prefix(r"\\?\").unwrap_or(text).to_owned()
                    },
                ));
            }
            #[cfg(not(windows))]
            Some(absolute)
        }
        let Some(folder) = folder.or_else(|| entry.parent()) else {
            return false;
        };
        let (Some(entry), Some(folder)) = (lexical(entry), lexical(folder)) else {
            return false;
        };
        self.grants.iter().any(|g| {
            lexical(&g.entry).is_some_and(|p| candidates::matches(&p, &entry))
                && lexical(&g.folder).is_some_and(|p| candidates::matches(&p, &folder))
        })
    }

    pub fn allows(&self, entry: &Path, folder: Option<&Path>) -> bool {
        self.grants.iter().any(|g| g.matches(entry, folder))
    }

    pub fn helper(
        &self,
        entry: &Path,
        folder: Option<&Path>,
        file: &str,
    ) -> Result<PathBuf, String> {
        let grant = self.grants.iter().find(|g| g.matches(entry, folder))
            .ok_or("plugin content changed or no hash-bound process grant exists; approve the new content")?;
        let clean = super::super::paths::clean(file)?;
        let expected = grant
            .helpers
            .get(&clean)
            .ok_or("helper is not listed in the player's grant")?;
        let path = super::super::paths::inside(&grant.folder, &clean)?;
        if !path
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&grant.folder)
            || hash(&path)? != *expected
        {
            return Err("helper changed or escaped its plugin folder".into());
        }
        #[cfg(windows)]
        if !clean.to_ascii_lowercase().ends_with(".exe") {
            return Err("Windows helpers must be .exe files".into());
        }
        Ok(path)
    }
}

pub(super) fn hash(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Err("not a regular file".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, PathBuf, ProcessGrants) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("omsi-grants-{}-{}-{unique}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(root.join("modules")).unwrap();
        let entry = root.join("plugin.lua");
        std::fs::write(&entry, "return 1").unwrap();
        std::fs::write(root.join("modules/code.lua"), "return 2").unwrap();
        std::fs::write(root.join("helper.exe"), "approved helper").unwrap();
        let grant = ProcessGrant::capture(&entry, &root, &["helper.exe".into()]).unwrap();
        (
            root,
            entry,
            ProcessGrants {
                grants: vec![grant],
                ..Default::default()
            },
        )
    }
    #[test]
    fn digest_is_sha256() {
        let (root, entry, _) = fixture();
        std::fs::write(&entry, "abc").unwrap();
        assert_eq!(
            hash(&entry).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manifest_roundtrip_preserves_exact_helper_consent() {
        let (root, entry, grants) = fixture();
        let json = serde_json::to_vec(&grants).unwrap();
        let grants: ProcessGrants = serde_json::from_slice(&json).unwrap();
        assert!(grants.allows(&entry, Some(&root)));
        assert!(grants.helper(&entry, Some(&root), "helper.exe").is_ok());
        for path in ["../helper.exe", "/bin/sh", "modules/code.lua"] {
            assert!(grants.helper(&entry, Some(&root), path).is_err());
        }
        assert!(
            grants
                .helper(
                    &entry,
                    Some(&root),
                    root.join("helper.exe").to_str().unwrap()
                )
                .is_err()
        );
        assert!(serde_json::from_str::<ProcessGrants>(r#"{"paths":["plugin.lua"]}"#).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn code_modules_helpers_and_added_files_revoke_saved_grants() {
        for name in ["plugin.lua", "modules/code.lua", "helper.exe", "new.lua"] {
            let (root, entry, grants) = fixture();
            std::fs::write(root.join(name), "replaced code").unwrap();
            assert!(!grants.allows(&entry, Some(&root)), "{name}");
            assert!(grants.helper(&entry, Some(&root), "helper.exe").is_err());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn removed_module_and_different_folder_revoke_consent() {
        let (root, entry, grants) = fixture();
        assert!(!grants.allows(&entry, Some(&root.join("modules"))));
        std::fs::remove_file(root.join("modules/code.lua")).unwrap();
        assert!(!grants.allows(&entry, Some(&root)));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mutable_saves_data_and_unrelated_shared_assets_do_not_revoke_consent() {
        let (root, entry, grants) = fixture();
        for name in ["plugin.save.lua", "other.save.lua", "plugin.data/cache.json", "other.data/large.bin", "data/status.txt", "other/assets/picture.bmp", "other.exe"] {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "normal writable data").unwrap();
        }
        assert!(grants.helper(&entry, Some(&root), "helper.exe").is_ok());
        // Shared Lua remains part of the trust boundary: require can load it.
        std::fs::write(root.join("other.lua"), "return 9").unwrap();
        assert!(!grants.allows(&entry, Some(&root)));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn folder_data_is_mutable_but_importable_lua_and_native_libraries_are_hashed() {
        use crate::lua::LuaSource;
        let (root, old_entry, _) = fixture();
        let entry = root.join("main.lua");
        std::fs::rename(old_entry, &entry).unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        std::fs::write(root.join("data/module.lua"), "return 5").unwrap();
        std::fs::write(root.join("helper.dll"), "native code").unwrap();
        std::fs::write(root.join("config.xml"), "immutable package asset").unwrap();
        let grant = ProcessGrant::capture(&entry, &root, &["helper.exe".into()]).unwrap();
        assert!(crate::lua::DiskSource { path: entry.clone() }.module("data.module").is_some());
        let grants = ProcessGrants { grants: vec![grant], ..Default::default() };
        std::fs::write(root.join("data.save.lua"), "return { counter = 1 }").unwrap();
        std::fs::write(root.join("data/state.json"), "{\"counter\":1}").unwrap();
        assert!(grants.allows(&entry, Some(&root)));
        for name in ["data/module.lua", "helper.dll", "config.xml"] {
            let path = root.join(name);
            let original = std::fs::read(&path).unwrap();
            std::fs::write(&path, "changed executable content or package asset").unwrap();
            assert!(!grants.allows(&entry, Some(&root)), "{name}");
            std::fs::write(path, original).unwrap();
        }
        std::fs::write(root.join("data/new.lua"), "return 8").unwrap();
        assert!(!grants.allows(&entry, Some(&root)));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ignored_data_does_not_use_the_code_file_or_byte_budget() {
        let (root, entry, grants) = fixture();
        let data = root.join("plugin.data");
        std::fs::create_dir(&data).unwrap();
        for i in 0..4100 { std::fs::write(data.join(format!("{i}.json")), "{}").unwrap(); }
        std::fs::File::create(data.join("large.bin")).unwrap().set_len(257 * 1024 * 1024).unwrap();
        assert!(grants.allows(&entry, Some(&root)));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn shared_folder_keeps_importable_code_and_native_libraries_in_consent() {
        for name in ["data/code.lua", "plugin.data/code.lua", "shared/other.lua", "helper.dll", "lib/loader.so.1", "lib/loader.dylib"] {
            let (root, entry, grants) = fixture();
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "new executable content").unwrap();
            assert!(!grants.allows(&entry, Some(&root)), "{name}");
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[cfg(windows)]
    #[test]
    fn short_paths_reach_worker_verification_but_do_not_grant_access() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetShortPathNameW(long: *const u16, short: *mut u16, size: u32) -> u32;
        }
        let (root, entry, grants) = fixture();
        let long: Vec<u16> = entry.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut short = vec![0; 32768];
        // SAFETY: both buffers are valid for the supplied lengths and long is NUL terminated.
        let n = unsafe { GetShortPathNameW(long.as_ptr(), short.as_mut_ptr(), short.len() as u32) };
        assert!(n > 0 && (n as usize) < short.len());
        let short = PathBuf::from(std::ffi::OsString::from_wide(&short[..n as usize]));
        assert!(grants.has_candidate(&short, short.parent()));
        assert!(grants.helper(&short, short.parent(), "helper.exe").is_ok());
        // The lexical DOS hint deliberately admits ambiguous aliases. Even identical
        // bytes at a different canonical path must still be rejected by the worker.
        let other = root.with_extension("alias~1");
        std::fs::create_dir(&other).unwrap();
        let other_entry = other.join("plugin.lua");
        std::fs::copy(&entry, &other_entry).unwrap();
        assert!(grants.has_candidate(&other_entry, Some(&other)));
        assert!(grants.helper(&other_entry, Some(&other), "helper.exe").is_err());
        std::fs::write(&entry, "modified plugin").unwrap();
        assert!(grants.has_candidate(&short, short.parent()));
        assert!(grants.helper(&short, short.parent(), "helper.exe").is_err());
        std::fs::remove_dir_all(other).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_code_cannot_receive_consent() {
        let (root, entry, _) = fixture();
        std::os::unix::fs::symlink(&entry, root.join("alias.lua")).unwrap();
        assert!(ProcessGrant::capture(&entry, &root, &[]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
