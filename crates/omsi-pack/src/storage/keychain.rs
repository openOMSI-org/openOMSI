//! Keychain items that any program of the user may read, like user-scoped DPAPI on Windows.
use crate::{PackError, Result};
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, OSStatus, TCFType};
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::ptr;

const DUPLICATE_ITEM: OSStatus = -25299;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecClass: CFStringRef;
    static kSecClassGenericPassword: CFStringRef;
    static kSecAttrService: CFStringRef;
    static kSecAttrAccount: CFStringRef;
    static kSecAttrAccess: CFStringRef;
    static kSecValueData: CFStringRef;
    static kSecACLAuthorizationDecrypt: CFStringRef;
    fn SecAccessCreate(
        descriptor: CFStringRef,
        trusted_list: CFArrayRef,
        access: *mut *mut c_void,
    ) -> OSStatus;
    fn SecAccessCopyMatchingACLList(access: *mut c_void, authorization: CFTypeRef) -> CFArrayRef;
    fn SecACLSetContents(
        acl: *mut c_void,
        application_list: CFArrayRef,
        description: CFStringRef,
        prompt_selector: u16,
    ) -> OSStatus;
    fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
}

/// Stores `secret` so that reading it never asks which program may use it.
pub(super) fn store(service: &str, account: &str, secret: &[u8]) -> Result<()> {
    let item = item(service, account, secret)?;
    let mut status = unsafe { SecItemAdd(item.as_concrete_TypeRef(), ptr::null_mut()) };
    if status == DUPLICATE_ITEM {
        security_framework::passwords::delete_generic_password(service, account)
            .map_err(|e| PackError::SecureStorage(e.to_string()))?;
        status = unsafe { SecItemAdd(item.as_concrete_TypeRef(), ptr::null_mut()) };
    }
    check(status)
}

/// The item's attributes, with an access object from [`access`].
fn item(service: &str, account: &str, secret: &[u8]) -> Result<CFDictionary<CFType, CFType>> {
    let description = CFString::new(service);
    let access = access(&description)?;
    let key = |k: CFStringRef| unsafe { CFString::wrap_under_get_rule(k) }.into_CFType();
    let class = unsafe { CFString::wrap_under_get_rule(kSecClassGenericPassword) };
    Ok(CFDictionary::from_CFType_pairs(&[
        (key(unsafe { kSecClass }), class.into_CFType()),
        (key(unsafe { kSecAttrService }), description.into_CFType()),
        (
            key(unsafe { kSecAttrAccount }),
            CFString::new(account).into_CFType(),
        ),
        (
            key(unsafe { kSecValueData }),
            CFData::from_buffer(secret).into_CFType(),
        ),
        (key(unsafe { kSecAttrAccess }), access),
    ]))
}

/// An access object whose decrypt rules have no application list, so any program may read.
fn access(description: &CFString) -> Result<CFType> {
    let no_apps = CFArray::<CFType>::from_CFTypes(&[]);
    let mut raw = ptr::null_mut();
    check(unsafe {
        SecAccessCreate(
            description.as_concrete_TypeRef(),
            no_apps.as_concrete_TypeRef(),
            &mut raw,
        )
    })?;
    let access = unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) };
    let rules = decrypt_rules(&access);
    if rules.is_empty() {
        return Err(PackError::SecureStorage(
            "keychain access has no decrypt rule".into(),
        ));
    }
    for rule in rules.iter() {
        check(unsafe {
            SecACLSetContents(
                rule.as_CFTypeRef() as *mut c_void,
                ptr::null(),
                description.as_concrete_TypeRef(),
                0,
            )
        })?;
    }
    Ok(access)
}

fn decrypt_rules(access: &CFType) -> CFArray<CFType> {
    let list = unsafe {
        SecAccessCopyMatchingACLList(
            access.as_CFTypeRef() as *mut c_void,
            kSecACLAuthorizationDecrypt as CFTypeRef,
        )
    };
    if list.is_null() {
        CFArray::from_CFTypes(&[])
    } else {
        unsafe { CFArray::wrap_under_create_rule(list) }
    }
}

fn check(status: OSStatus) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(PackError::SecureStorage(
            security_framework::base::Error::from_code(status).to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn SecACLCopyContents(
            acl: *mut c_void,
            application_list: *mut CFArrayRef,
            description: *mut CFStringRef,
            prompt_selector: *mut u16,
        ) -> OSStatus;
    }

    #[test]
    fn any_program_may_decrypt_a_stored_entitlement() {
        let access = access(&CFString::new("org.openomsi.test")).unwrap();
        let rules = decrypt_rules(&access);
        assert!(!rules.is_empty());
        for rule in rules.iter() {
            let (mut apps, mut description, mut prompt) = (ptr::null(), ptr::null(), 0);
            let status = unsafe {
                SecACLCopyContents(
                    rule.as_CFTypeRef() as *mut c_void,
                    &mut apps,
                    &mut description,
                    &mut prompt,
                )
            };
            assert_eq!(status, 0);
            if !description.is_null() {
                drop(unsafe { CFString::wrap_under_create_rule(description) });
            }
            assert!(
                apps.is_null(),
                "a decrypt rule names the programs that may read"
            );
        }
    }
}
