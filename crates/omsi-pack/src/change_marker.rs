//! Windows change markers for the open-time check in `VerifiedPack::verify_once`.
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{PackError, Result};

/// Size, modification time, NTFS change time and file ID of a pack file; `None` when its folder cannot be listed or it has no change time (FAT, exFAT).
pub(crate) fn file_marker(
    root: &Path,
    file: &str,
    folders: &mut HashMap<PathBuf, Option<HashMap<String, [i64; 4]>>>,
) -> Result<Option<[u8; 32]>> {
    let (dir, name) = file.rsplit_once('/').unwrap_or(("", file));
    let dir = root.join(dir);
    let Some(entries) = folders.entry(dir).or_insert_with_key(|dir| folder_markers(dir).ok()) else {
        return Ok(None);
    };
    let [size, modified, changed, id] = *entries
        .get(&name.to_ascii_lowercase())
        .ok_or(PackError::Invalid("chunk is missing, truncated, or changed"))?;
    if changed == 0 {
        return Ok(None);
    }
    let mut marker = [0u8; 32];
    for (slot, value) in marker.chunks_exact_mut(8).zip([size, modified, changed, id]) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
    Ok(Some(marker))
}

fn folder_markers(dir: &Path) -> std::io::Result<HashMap<String, [i64; 4]>> {
    use std::os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _};
    use windows::Win32::Foundation::{ERROR_NO_MORE_FILES, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetFileInformationByHandleEx, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY,
    };

    let folder = fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(dir)?;
    let handle = HANDLE(folder.as_raw_handle());
    let mut entries = HashMap::new();
    let mut buffer = vec![0u64; 8192];
    let mut class = FileIdBothDirectoryRestartInfo;
    loop {
        // only ERROR_NO_MORE_FILES after the last entry means the listing is complete
        match unsafe { GetFileInformationByHandleEx(handle, class, buffer.as_mut_ptr().cast(), (buffer.len() * 8) as u32) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => return Ok(entries),
            Err(e) => return Err(std::io::Error::other(e)),
        }
        class = FileIdBothDirectoryInfo;
        let mut offset = 0;
        loop {
            // SAFETY: the call wrote a chain of 8-byte aligned entries inside the buffer, each
            // followed by its name
            let (entry, name) = unsafe {
                let ptr = buffer.as_ptr().cast::<u8>().add(offset).cast::<FILE_ID_BOTH_DIR_INFO>();
                let name = std::ptr::addr_of!((*ptr).FileName).cast::<u16>();
                (&*ptr, std::slice::from_raw_parts(name, (*ptr).FileNameLength as usize / 2))
            };
            entries.insert(
                String::from_utf16_lossy(name).to_ascii_lowercase(),
                [entry.EndOfFile, entry.LastWriteTime, entry.ChangeTime, entry.FileId],
            );
            if entry.NextEntryOffset == 0 {
                break;
            }
            offset += entry.NextEntryOffset as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_that_cannot_be_listed_gives_no_marker() {
        let root = std::env::temp_dir().join(format!("omsi-pack-change-marker-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("listed")).unwrap();
        fs::write(root.join("listed/a.bin"), b"chunk").unwrap();
        let mut folders = HashMap::new();
        // (a folder that cannot be opened for listing: no marker, so `verify_once` runs the full scan)
        assert_eq!(file_marker(&root, "unlisted/a.bin", &mut folders).unwrap(), None);
        assert_eq!(folders[&root.join("unlisted")], None);
        assert!(file_marker(&root, "listed/a.bin", &mut folders).unwrap().is_some());
        assert!(file_marker(&root, "listed/b.bin", &mut folders).is_err(), "a file missing from a complete listing");
        fs::remove_dir_all(root).unwrap();
    }
}
