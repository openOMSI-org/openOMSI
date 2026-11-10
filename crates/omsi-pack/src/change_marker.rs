//! Windows change markers for the open-time check in `VerifiedPack::verify_once`.
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{PackError, Result};

/// Size, modification time, NTFS change time and file ID of a pack file; `None` without a change time (FAT, exFAT).
pub(crate) fn file_marker(
    root: &Path,
    file: &str,
    folders: &mut HashMap<PathBuf, HashMap<String, [i64; 4]>>,
) -> Result<Option<[u8; 32]>> {
    let (dir, name) = file.rsplit_once('/').unwrap_or(("", file));
    let dir = root.join(dir);
    if !folders.contains_key(&dir) {
        let entries = folder_markers(&dir).unwrap_or_default();
        folders.insert(dir.clone(), entries);
    }
    let [size, modified, changed, id] = *folders[&dir]
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
    use windows::Win32::Foundation::HANDLE;
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
    // the call fails with ERROR_NO_MORE_FILES after the last entry
    while unsafe { GetFileInformationByHandleEx(handle, class, buffer.as_mut_ptr().cast(), (buffer.len() * 8) as u32) }.is_ok() {
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
    Ok(entries)
}
