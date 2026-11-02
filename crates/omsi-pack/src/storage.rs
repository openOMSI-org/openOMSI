//! Native storage for signed offline publisher entitlements.
use crate::{read_bounded_file, PackError, Result, ENTITLEMENT_LIMIT};
use sha2::{Digest, Sha256};
use std::{fs, io, path::PathBuf};

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
    security_framework::passwords::set_generic_password(SERVICE, scope, secret)
        .map_err(|e| PackError::SecureStorage(e.to_string()))
}

#[cfg(target_os = "macos")]
fn native_load(scope: &str) -> Result<Vec<u8>> {
    validate_scope(scope)?;
    security_framework::passwords::generic_password(
        security_framework::passwords::PasswordOptions::new_generic_password(SERVICE, scope),
    )
    .map_err(|e| PackError::SecureStorage(e.to_string()))
}

#[cfg(target_os = "macos")]
fn native_delete(scope: &str) -> Result<()> {
    validate_scope(scope)?;
    security_framework::passwords::delete_generic_password(SERVICE, scope)
        .map_err(|e| PackError::SecureStorage(e.to_string()))
}

#[cfg(windows)]
fn native_store(scope: &str, secret: &[u8]) -> Result<()> {
    use std::ffi::c_void;
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB},
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

#[cfg(windows)]
fn native_load(scope: &str) -> Result<Vec<u8>> {
    use std::ffi::c_void;
    use windows::Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    validate_scope(scope)?;
    let path = windows_key_path(scope)?;
    let encrypted = read_bounded_file(&path, ENTITLEMENT_LIMIT + 4096)?;
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
    use std::{io::Write as _, os::windows::ffi::OsStrExt};
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };
    let path = windows_key_path(scope)?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
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
