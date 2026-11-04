//! Publisher-signed offline entitlements: the licences that unlock protected packs.

use crate::{
    load_manifest, now_unix, sha256_hex, KeyStore, Manifest, PackError, Result, SigningKey,
    TrustedPublishers, ENTITLEMENT_LIMIT, FORMAT_VERSION,
};
use ring::signature::{self, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::{Zeroize, Zeroizing};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entitlement {
    pub format_version: u32,
    pub publisher_id: String,
    pub signing_key_id: String,
    pub pack_id: String,
    pub pack_version: String,
    pub manifest_sha256: String,
    pub key_id: String,
    /// The pack manifest's licence ID, when the pack was built for one sale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence_id: Option<String>,
    pub expires_unix: u64,
    /// This signed key grant is a bearer secret. Deliver it confidentially; anyone who obtains
    /// it can copy it to another account/device, even after the original recipient unlocks it.
    pub content_key: [u8; 32],
}

impl Drop for Entitlement {
    fn drop(&mut self) {
        self.content_key.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedEntitlement {
    pub(crate) entitlement: Entitlement,
    pub(crate) signature: Vec<u8>,
}

/// Create a publisher-signed offline bearer entitlement for one exact pack revision.
pub fn issue_entitlement(
    manifest: &Manifest,
    content_key: [u8; 32],
    expires_unix: u64,
    signer: &SigningKey,
) -> Result<Vec<u8>> {
    let content_key = Zeroizing::new(content_key);
    if manifest.key_id.as_deref() != Some(sha256_hex(&*content_key).as_str())
        || signer.key_id() != manifest.signing_key_id
        || expires_unix <= now_unix()?
    {
        return Err(PackError::Invalid(
            "entitlement signer, key id, or expiry does not match the pack",
        ));
    }
    let entitlement = Entitlement {
        format_version: FORMAT_VERSION,
        publisher_id: manifest.publisher_id.clone(),
        signing_key_id: signer.key_id(),
        pack_id: manifest.pack_id.clone(),
        pack_version: manifest.version.clone(),
        manifest_sha256: sha256_hex(&serde_json::to_vec(manifest)?),
        key_id: manifest.key_id.clone().unwrap(),
        licence_id: manifest.licence_id.clone(),
        expires_unix,
        content_key: *content_key,
    };
    let mut bytes = serde_json::to_vec(&entitlement)?;
    let signature = signer.pair.sign(&bytes).as_ref().to_vec();
    bytes.zeroize();
    let signed = SignedEntitlement {
        entitlement,
        signature,
    };
    Ok(serde_json::to_vec(&signed)?)
}

/// Verify a publisher-signed bearer entitlement against the pack's signed manifest, then
/// persist its content key in the caller's native protected store.
pub fn unlock(
    pack_root: &Path,
    bytes: &[u8],
    trusted: &TrustedPublishers,
    keys: &dyn KeyStore,
) -> Result<Manifest> {
    if bytes.len() > ENTITLEMENT_LIMIT {
        return Err(PackError::Invalid("offline entitlement is too large"));
    }
    let manifest = load_manifest(pack_root, trusted)?;
    let entitlement = verify_entitlement(&manifest, bytes, trusted)?;
    let scope = entitlement_scope(&manifest, &entitlement.key_id)?;
    keys.store(&scope, bytes)?;
    Ok(manifest)
}

/// Remove the OS-protected entitlement for one signed pack revision. The scope includes the
/// canonical manifest digest and content-key ID, so this cannot remove another pack's grant.
pub fn forget_unlock(
    pack_root: &Path,
    trusted: &TrustedPublishers,
    keys: &dyn KeyStore,
) -> Result<()> {
    let manifest = load_manifest(pack_root, trusted)?;
    let Some(key_id) = manifest.key_id.as_deref() else {
        return Ok(());
    };
    let scope = entitlement_scope(&manifest, key_id)?;
    match keys.delete(&scope) {
        Err(PackError::KeyNotFound) => Ok(()),
        result => result,
    }
}

pub(crate) fn verify_entitlement(
    manifest: &Manifest,
    bytes: &[u8],
    trusted: &TrustedPublishers,
) -> Result<Entitlement> {
    if bytes.len() > ENTITLEMENT_LIMIT {
        return Err(PackError::Invalid("offline entitlement is too large"));
    }
    let signed: SignedEntitlement = serde_json::from_slice(bytes)?;
    let e = &signed.entitlement;
    let canonical = serde_json::to_vec(e)?;
    let public = trusted
        .key(&e.publisher_id, &e.signing_key_id)
        .ok_or(PackError::UntrustedPublisher)?;
    if e.format_version != FORMAT_VERSION || sha256_hex(public) != e.signing_key_id {
        return Err(PackError::Invalid(
            "unsupported or mismatched entitlement signer",
        ));
    }
    UnparsedPublicKey::new(&signature::ED25519, public)
        .verify(&canonical, &signed.signature)
        .map_err(|_| PackError::InvalidSignature)?;
    if e.expires_unix <= now_unix()? {
        return Err(PackError::Expired);
    }
    if e.key_id != sha256_hex(&e.content_key)
        || e.publisher_id != manifest.publisher_id
        || e.signing_key_id != manifest.signing_key_id
        || e.pack_id != manifest.pack_id
        || e.pack_version != manifest.version
        || e.key_id != manifest.key_id.as_deref().unwrap_or("")
        || e.licence_id != manifest.licence_id
        || e.manifest_sha256 != sha256_hex(&serde_json::to_vec(manifest)?)
    {
        return Err(PackError::Invalid(
            "offline entitlement is for another pack, version, licence, or signing key",
        ));
    }
    Ok(signed.entitlement)
}

pub(crate) fn entitlement_scope(manifest: &Manifest, key_id: &str) -> Result<String> {
    let mut bytes = serde_json::to_vec(manifest)?;
    bytes.push(0);
    bytes.extend_from_slice(key_id.as_bytes());
    Ok(sha256_hex(&bytes))
}
