/// This machine's hardware identifier (see `omsi-lan`'s `bans::local_hardware_id`), read by
/// every session made after it is set. It never goes over the wire: a host gets
/// `host_hardware_id` of it and the host's realm, so two hosts cannot match their players.
static HARDWARE_ID: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// Tell the sessions this machine's hardware identifier (32 lower-case hex digits; anything
/// else is ignored).
pub fn set_hardware_id(id: &str) {
    if is_hardware_id(id) {
        *HARDWARE_ID.write().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
    }
}

pub fn clear_hardware_id() {
    *HARDWARE_ID.write().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn hardware_id() -> Option<String> {
    HARDWARE_ID.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn host_hardware_id(machine: &str, realm: &str) -> String {
    use sha2::{Digest, Sha256};
    let h = Sha256::new().chain_update(b"openOMSI hardware id v2\0").chain_update(machine.as_bytes()).chain_update(b"\0").chain_update(realm.as_bytes()).finalize();
    h[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// A hardware identifier as it goes over the wire: 128 bits in 32 lower-case hex digits.
pub fn is_hardware_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
