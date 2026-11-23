//! Native storage for signed offline publisher entitlements.
#[cfg(windows)]
use crate::{ENTITLEMENT_LIMIT, read_bounded_file};
use crate::{PackError, Result};
#[cfg(windows)]
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(target_os = "macos")]
mod keychain;

#[cfg(target_os = "macos")]
const SERVICE: &str = "org.openomsi.protected-pack";

pub trait KeyStore {
    /// Store the signed entitlement envelope as an opaque native-protected secret.
    fn store(&self, scope: &str, signed_entitlement: &[u8]) -> Result<()>;
    fn load(&self, scope: &str) -> Result<Vec<u8>>;
    fn delete(&self, scope: &str) -> Result<()>;
}

/// OS-native key store: Keychain on macOS and user-scoped DPAPI on Windows.
pub struct SystemKeyStore;

impl KeyStore for SystemKeyStore {
    fn store(&self, scope: &str, signed_entitlement: &[u8]) -> Result<()> {
        native_store(scope, signed_entitlement)
    }
    fn load(&self, scope: &str) -> Result<Vec<u8>> {
        native_load(scope)
    }
    fn delete(&self, scope: &str) -> Result<()> {
        native_delete(scope)
    }
}

#[cfg(any(target_os = "macos", windows))]
fn validate_scope(scope: &str) -> Result<()> {
    if crate::is_sha256(scope) {
        Ok(())
    } else {
        Err(PackError::Invalid("native entitlement scope is malformed"))
    }
}

#[cfg(target_os = "macos")]
fn native_store(scope: &str, secret: &[u8]) -> Result<()> {
    validate_scope(scope)?;
    keychain::store(SERVICE, scope, secret)
}

#[cfg(target_os = "macos")]
fn keychain_error(error: security_framework::base::Error) -> PackError {
    // errSecItemNotFound is the only status that means this grant is absent.
    if error.code() == -25300 {
        PackError::KeyNotFound
    } else {
        PackError::SecureStorage(error.to_string())
    }
}

#[cfg(target_os = "macos")]
fn native_load(scope: &str) -> Result<Vec<u8>> {
    validate_scope(scope)?;
    security_framework::passwords::generic_password(
        security_framework::passwords::PasswordOptions::new_generic_password(SERVICE, scope),
    )
    .map_err(keychain_error)
}

#[cfg(target_os = "macos")]
fn native_delete(scope: &str) -> Result<()> {
    validate_scope(scope)?;
    security_framework::passwords::delete_generic_password(SERVICE, scope).map_err(keychain_error)
}

#[cfg(windows)]
fn native_store(scope: &str, secret: &[u8]) -> Result<()> {
    use std::ffi::c_void;
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    };
    validate_scope(scope)?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut entropy_bytes = Sha256::digest(scope.as_bytes());
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            windows::core::PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|e| PackError::SecureStorage(e.to_string()))?;
    let protected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut c_void)));
    }
    write_windows_key(scope, &protected)
}

/// The DPAPI blob stored at `path`. The key file itself must not be a link, but a folder above
/// it may be one: a player's `AppData\Local` moved to another drive with `mklink /J` is a
/// junction. So the path is resolved first, as for the publisher trust file, and the bounded
/// reader then sees only real folders.
#[cfg(windows)]
fn read_key_file(path: &Path) -> Result<Vec<u8>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(PackError::KeyNotFound);
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(PackError::Invalid(
                "a stored key must not be a symbolic link",
            ));
        }
        _ => {}
    }
    let resolved = path.canonicalize().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            PackError::KeyNotFound
        } else {
            error.into()
        }
    })?;
    read_bounded_file(&resolved, ENTITLEMENT_LIMIT + 4096).map_err(|error| match error {
        PackError::Io(error) if error.kind() == io::ErrorKind::NotFound => PackError::KeyNotFound,
        error => error,
    })
}

#[cfg(windows)]
fn native_load(scope: &str) -> Result<Vec<u8>> {
    use std::ffi::c_void;
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };
    validate_scope(scope)?;
    let encrypted = read_key_file(&windows_key_path(scope)?)?;
    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let mut entropy_bytes = Sha256::digest(scope.as_bytes());
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| {
        PackError::SecureStorage(
            "DPAPI could not unlock this key for the current Windows user".into(),
        )
    })?;
    let length = output.cbData as usize;
    if output.pbData.is_null() {
        return Err(PackError::Invalid("DPAPI returned an empty entitlement"));
    }
    if length == 0 {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(output.pbData as *mut c_void)));
        }
        return Err(PackError::Invalid("DPAPI returned an empty entitlement"));
    }
    let mut result = unsafe {
        let decrypted = std::slice::from_raw_parts_mut(output.pbData, length);
        let copy = if length <= ENTITLEMENT_LIMIT {
            Some(decrypted.to_vec())
        } else {
            None
        };
        zeroize::Zeroize::zeroize(decrypted);
        copy
    };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut c_void)));
    }
    if length > ENTITLEMENT_LIMIT {
        if let Some(bytes) = result.as_mut() {
            zeroize::Zeroize::zeroize(bytes);
        }
        return Err(PackError::Invalid(
            "stored entitlement exceeds its size limit",
        ));
    }
    Ok(result.take().unwrap_or_default())
}

#[cfg(windows)]
fn native_delete(scope: &str) -> Result<()> {
    validate_scope(scope)?;
    let path = windows_key_path(scope)?;
    fs::remove_file(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            PackError::KeyNotFound
        } else {
            e.into()
        }
    })
}

#[cfg(windows)]
fn windows_key_path(scope: &str) -> Result<PathBuf> {
    validate_scope(scope)?;
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or(PackError::SecureStorageUnavailable)?;
    let dir = base.join("openOMSI/protected-keys");
    fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{scope}.dpapi")))
}

#[cfg(windows)]
fn write_windows_key(scope: &str, value: &[u8]) -> Result<()> {
    write_key_file(&windows_key_path(scope)?, value)
}

/// Replace the key file at `path` with `value` through a temporary file beside it. The
/// temporary file gets a random name: one a crash left behind never blocks a later store.
#[cfg(windows)]
fn write_key_file(path: &Path, value: &[u8]) -> Result<()> {
    use std::{io::Write as _, os::windows::ffi::OsStrExt};
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };
    let dir = path
        .parent()
        .ok_or(PackError::Invalid("stored key path has no folder"))?;
    let (tmp, mut f) = crate::create_private_temp(dir, ".key")?;
    if let Err(e) = f.write_all(value).and_then(|_| f.sync_all()) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    let source: Vec<u16> = tmp.as_os_str().encode_wide().chain([0]).collect();
    let target: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    if let Err(e) = unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } {
        let _ = fs::remove_file(&tmp);
        return Err(PackError::SecureStorage(e.to_string()));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn native_store(_: &str, _: &[u8]) -> Result<()> {
    Err(PackError::SecureStorageUnavailable)
}
#[cfg(not(any(target_os = "macos", windows)))]
fn native_load(_: &str) -> Result<Vec<u8>> {
    Err(PackError::SecureStorageUnavailable)
}
#[cfg(not(any(target_os = "macos", windows)))]
fn native_delete(_: &str) -> Result<()> {
    Err(PackError::SecureStorageUnavailable)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn keychain_access_errors_are_not_reported_as_absent_grants() {
        assert!(matches!(
            keychain_error(security_framework::base::Error::from_code(-25300)),
            PackError::KeyNotFound
        ));
        assert!(matches!(
            keychain_error(security_framework::base::Error::from_code(-25293)),
            PackError::SecureStorage(_)
        ));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn a_key_below_a_junction_is_read_but_the_pack_reader_still_refuses_it() {
        let base =
            std::env::temp_dir().join(format!("omsi-pack-key-junction-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let real = base.join("real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("key.dpapi"), b"sealed").unwrap();
        let moved = base.join("moved");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&moved)
            .arg(&real)
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
        let key = read_key_file(&moved.join("key.dpapi"));
        let pack_rule = read_bounded_file(&moved.join("key.dpapi"), 64);
        let absent = read_key_file(&moved.join("absent.dpapi"));
        // (the junction first: removing the folder that holds it must not follow it)
        let _ = fs::remove_dir(&moved);
        let _ = fs::remove_dir_all(&base);
        assert_eq!(key.unwrap(), b"sealed");
        assert!(
            pack_rule.is_err(),
            "a junction in a pack path must stay refused"
        );
        assert!(matches!(absent, Err(PackError::KeyNotFound)));
    }

    #[test]
    fn a_temporary_file_left_by_a_crash_does_not_block_storing_a_key() {
        let dir = std::env::temp_dir().join(format!("omsi-pack-key-temp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("key.dpapi");
        // the name the old code gave this process's temporary file
        let left = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&left, b"left over").unwrap();
        let first = write_key_file(&path, b"first");
        let second = write_key_file(&path, b"second");
        let stored = fs::read(&path);
        let _ = fs::remove_dir_all(&dir);
        first.unwrap();
        second.unwrap();
        assert_eq!(stored.unwrap(), b"second");
    }
}
