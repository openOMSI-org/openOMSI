//! Bans of a machine, not of a name: the hardware identifier a game sends in its hello, and
//! the lists of banned identifiers a host keeps.
//!
//! **The identifier.** 128 bits: SHA-256 over a fixed text and the machine's own id (the
//! platform's machine GUID / `machine-id` / `IOPlatformUUID`; on a system with none, a random
//! number kept in `~/.openomsi/hardware-id`), cut to 16 bytes. That one never leaves the
//! machine: a host sends its realm (`REALM`, a random key kept in the owner's list), and the
//! game sends `omsi_net::host_hardware_id` of the two. A host's sessions all get the same
//! identifier, so a banned machine is known again; two owners get different ones, so they
//! cannot compare notes on who joined them unless they share one realm on purpose. No serial number of a disk or board is read. A
//! player switches it off in the launcher (`hardware_id = 0`) or with `OMSI_NO_HARDWARE_ID=1`
//! (a host may turn such players away, `require_hardware_id`). Bans made before the realm
//! (protocol 15) name identifiers no game sends any more. It is a claim of the game, not a proof of the
//! machine: a changed game, a reinstalled system, a virtual machine or a new machine id gives
//! another identifier, so a hardware ban stops the casual returner, not someone who works at
//! it (SERVER.md says so too).
//!
//! **The lists.** A ban has an id of its own, the identifier, the name the player had, the
//! reason, who banned, when, optionally until when, and a scope:
//! * `session` - this session's list, beside `server.cfg` (`bans.json`) for a dedicated
//!   server; for a game hosting by code it lasts as long as that game runs;
//! * `owner` ("all my sessions") - the owner's list, `~/.openomsi/bans-owner.json`, which every
//!   session hosted from this user account on this machine reads, dedicated servers and
//!   hosting games alike; `owner_bans = <file>` in `server.cfg` points several servers (on
//!   other machines too, by a synced file) at one list.
//!
//! A host holds both lists and turns a joining machine away if either names it. Files are
//! written whole to a temporary file and moved over the old one, so a crash leaves the old
//! list, never half of the new.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// How long a ban lasts at most as a reason can be (characters).
pub const MAX_REASON: usize = 120;
/// Bans kept per list at most.
const MAX_BANS: usize = 2000;

pub use omsi_admin::BanScope as Scope;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ban {
    /// 8 hex digits, made when the ban is: what an unban names.
    pub id: String,
    /// The hardware identifier (32 hex digits).
    pub hw: String,
    /// The name the player had (as they called themselves: no proof of who).
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reason: String,
    /// Who banned (an admin's label).
    #[serde(default)]
    pub by: String,
    /// Seconds since 1970.
    pub at: u64,
    /// Seconds since 1970 when it ends (None: never).
    #[serde(default)]
    pub until: Option<u64>,
    pub scope: Scope,
}

impl Ban {
    pub fn active(&self, now: u64) -> bool {
        self.until.is_none_or(|u| u > now)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    realm: String,
    #[serde(default)]
    bans: Vec<Ban>,
}

fn random_realm() -> String {
    let (a, b) = (omsi_net::random_session_id(), omsi_net::random_session_id());
    format!("{:016x}{:016x}", a ^ (b << 16), b ^ (a << 24))
}

static ALLOWED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether this game sends hosts an identifier at all (the launcher's `hardware_id`).
pub fn allow_hardware_id(on: bool) {
    ALLOWED.store(on, std::sync::atomic::Ordering::Relaxed);
    if !on {
        omsi_net::clear_hardware_id();
    }
}

/// The identifier a machine id makes (see the module's text).
pub fn derive_hardware_id(machine_id: &str) -> String {
    let h = Sha256::new().chain_update(b"openOMSI hardware id v1\0").chain_update(machine_id.trim().to_ascii_lowercase().as_bytes()).finalize();
    h[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// The machine's own id, if the system has one.
fn machine_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        for p in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(t) = std::fs::read_to_string(p) {
                if t.trim().len() >= 16 {
                    return Some(t);
                }
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/sbin/ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let line = text.lines().find(|l| l.contains("IOPlatformUUID"))?;
        line.split('"').nth(3).map(str::to_string)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("reg").args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"]).creation_flags(0x0800_0000).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        text.lines().find(|l| l.contains("MachineGuid"))?.split_whitespace().last().map(str::to_string)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

/// This machine's hardware identifier (None: switched off with `OMSI_NO_HARDWARE_ID=1`, or
/// nothing to make it of and nowhere to keep a random one).
pub fn local_hardware_id() -> Option<String> {
    static ID: OnceLock<Option<String>> = OnceLock::new();
    ID.get_or_init(|| {
        if omsi_cfg::flags::OMSI_NO_HARDWARE_ID.is_set() {
            return None;
        }
        if let Some(m) = machine_id() {
            return Some(derive_hardware_id(&m));
        }
        // (no machine id on this system: a random one, kept)
        let path = crate::data_dir()?.join("hardware-id");
        if let Ok(t) = std::fs::read_to_string(&path) {
            if omsi_net::is_hardware_id(t.trim()) {
                return Some(t.trim().to_string());
            }
        }
        let id: String = omsi_net::random_session_id().to_le_bytes().iter().chain(omsi_net::random_session_id().to_le_bytes().iter()).map(|b| format!("{b:02x}")).collect();
        let _ = std::fs::create_dir_all(path.parent()?);
        std::fs::write(&path, &id).ok()?;
        Some(id)
    })
    .clone()
}

/// Tell the sessions made from now on the machine's identifier, or none when switched off.
pub fn announce_local_hardware_id() {
    if !ALLOWED.load(std::sync::atomic::Ordering::Relaxed) {
        omsi_net::clear_hardware_id();
        return;
    }
    if let Some(id) = local_hardware_id() {
        omsi_net::set_hardware_id(&id);
    }
}

fn clean(text: &str, max: usize) -> String {
    omsi_admin::clean(text, max)
}

fn new_id(taken: &[&Ban]) -> String {
    loop {
        let id = format!("{:08x}", omsi_net::random_session_id() as u32);
        if !taken.iter().any(|b| b.id == id) {
            return id;
        }
    }
}

/// The two lists of one host.
#[derive(Default)]
pub struct BanStore {
    session_path: Option<PathBuf>,
    owner_path: Option<PathBuf>,
    session: Vec<Ban>,
    owner: Vec<Ban>,
    /// The key players make this host's identifier with (`REALM`), kept in the owner's file.
    pub realm: String,
    owner_stamp: Option<(std::time::SystemTime, u64)>,
    /// Counted up at every change (and when the owner's file changed under us), for the host
    /// loop to pass the lists on to the session only when they differ.
    pub revision: u64,
}

impl BanStore {
    /// The lists from their files (`None`: that list is kept in memory only). A file that
    /// cannot be read is an empty list, said in the log - and never overwritten by an
    /// empty one unless a ban is made (a `.bak` of an unreadable file is kept).
    pub fn open(session_path: Option<PathBuf>, owner_path: Option<PathBuf>) -> BanStore {
        let (session, _, _) = read(session_path.as_deref());
        let (owner, realm, readable) = read(owner_path.as_deref());
        let mut s = BanStore { session, owner, realm, owner_stamp: None, session_path, owner_path, revision: 1 };
        if s.realm.is_empty() {
            s.realm = random_realm();
            if !s.session.is_empty() || !s.owner.is_empty() {
                log::warn!("bans: hardware bans made before openOMSI's per-owner identifiers (LAN protocol 16) no longer match anybody; ban those players again");
            }
            let fresh = s.owner_path.as_deref().is_some_and(|p| !p.exists());
            if fresh {
                s.claim_realm();
            } else if readable {
                if let Err(e) = s.save(Scope::Owner) {
                    log::error!("bans: {e}; the identifiers players send will change at the next start");
                }
            }
        }
        s.owner_stamp = s.owner_path.as_deref().and_then(stamp);
        s.prune(omsi_admin::unix_now());
        s
    }

    /// The owner's list of this account (`~/.openomsi/bans-owner.json`).
    pub fn default_owner_path() -> Option<PathBuf> {
        Some(crate::data_dir()?.join("bans-owner.json"))
    }

    fn list(&self, scope: Scope) -> &Vec<Ban> {
        match scope {
            Scope::Session => &self.session,
            Scope::Owner => &self.owner,
        }
    }

    /// Ban machine `hw`; the new ban's id. A machine already on the list has its entry
    /// replaced (a newer reason, or a longer or shorter time).
    #[allow(clippy::too_many_arguments)]
    pub fn add(&mut self, scope: Scope, hw: &str, name: &str, reason: &str, by: &str, until: Option<u64>, now: u64) -> Result<Ban, String> {
        if !omsi_net::is_hardware_id(hw) {
            return Err("that player's game sent no hardware identifier (an older game, or it switched it off): only a ban by name and address is possible".into());
        }
        self.refresh_owner();
        let all: Vec<&Ban> = self.session.iter().chain(self.owner.iter()).collect();
        let ban = Ban { id: new_id(&all), hw: hw.to_string(), name: clean(name, 64), reason: clean(reason, MAX_REASON), by: clean(by, 64), at: now, until, scope };
        let list = match scope {
            Scope::Session => &mut self.session,
            Scope::Owner => &mut self.owner,
        };
        list.retain(|b| b.hw != hw);
        if list.len() >= MAX_BANS {
            return Err(format!("the ban list is full ({MAX_BANS}): remove old bans first"));
        }
        list.push(ban.clone());
        self.revision += 1;
        self.save(scope)?;
        Ok(ban)
    }

    /// Lift ban `id` (from either list); the ban that was.
    pub fn remove(&mut self, id: &str) -> Result<Ban, String> {
        self.refresh_owner();
        for scope in [Scope::Session, Scope::Owner] {
            let list = match scope {
                Scope::Session => &mut self.session,
                Scope::Owner => &mut self.owner,
            };
            if let Some(k) = list.iter().position(|b| b.id == id.trim()) {
                let ban = list.remove(k);
                self.revision += 1;
                self.save(scope)?;
                return Ok(ban);
            }
        }
        Err(format!("no ban '{}'", clean(id, 16)))
    }

    /// The ban that names machine `hw` and holds now.
    pub fn find(&self, hw: &str, now: u64) -> Option<&Ban> {
        self.session.iter().chain(self.owner.iter()).find(|b| b.hw == hw && b.active(now))
    }

    /// Every ban that holds now, newest first.
    pub fn active(&self, now: u64) -> Vec<Ban> {
        let mut v: Vec<Ban> = self.session.iter().chain(self.owner.iter()).filter(|b| b.active(now)).cloned().collect();
        v.sort_by(|a, b| b.at.cmp(&a.at).then(a.id.cmp(&b.id)));
        v
    }

    /// What the door holds: (identifier, the reason a turned-away player reads).
    pub fn door(&self, now: u64) -> Vec<(String, String)> {
        self.active(now).into_iter().map(|b| (b.hw, b.reason)).collect()
    }

    /// Forget the bans that ended (kept in the file until the next change, which drops them).
    pub fn prune(&mut self, now: u64) {
        let before = self.session.len() + self.owner.len();
        self.session.retain(|b| b.active(now));
        self.owner.retain(|b| b.active(now));
        if self.session.len() + self.owner.len() != before {
            self.revision += 1;
        }
    }

    /// A new owner's list with the realm in it, unless another process made one first: then
    /// its realm is taken, so every session of the account asks for the same identifiers.
    fn claim_realm(&mut self) {
        let Some(p) = self.owner_path.clone() else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let body = serde_json::to_vec_pretty(&File { version: 1, realm: self.realm.clone(), bans: Vec::new() }).unwrap_or_default();
        let made = std::fs::OpenOptions::new().write(true).create_new(true).open(&p).and_then(|mut f| std::io::Write::write_all(&mut f, &body));
        if made.is_err() {
            let (owner, realm, _) = read(Some(&p));
            if !realm.is_empty() {
                self.owner = owner;
                self.realm = realm;
            }
        }
    }

    /// Read the owner's file again if another session of the account changed it.
    pub fn refresh_owner(&mut self) {
        let Some(p) = self.owner_path.clone() else { return };
        let now = stamp(&p);
        if now != self.owner_stamp {
            let (owner, realm, _) = read(Some(&p));
            self.owner = owner;
            if !realm.is_empty() {
                self.realm = realm;
            }
            self.owner_stamp = now;
            self.revision += 1;
        }
    }

    fn save(&mut self, scope: Scope) -> Result<(), String> {
        let (path, list, realm) = match scope {
            Scope::Session => (&self.session_path, &self.session, ""),
            Scope::Owner => (&self.owner_path, &self.owner, self.realm.as_str()),
        };
        let Some(path) = path.clone() else { return Ok(()) };
        write(&path, list, realm).map_err(|e| format!("the ban list {} could not be saved: {e}", path.display()))?;
        if scope == Scope::Owner {
            self.owner_stamp = stamp(&path);
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.list(Scope::Session).len() + self.list(Scope::Owner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// When a file was written and how long it is (a file system's clock can be too coarse to
/// tell two writes in a row apart; the length mostly can).
fn stamp(p: &Path) -> Option<(std::time::SystemTime, u64)> {
    let m = std::fs::metadata(p).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn read(path: Option<&Path>) -> (Vec<Ban>, String, bool) {
    let Some(path) = path else { return (Vec::new(), String::new(), true) };
    let Ok(text) = std::fs::read_to_string(path) else { return (Vec::new(), String::new(), !path.exists()) };
    match serde_json::from_str::<File>(&text) {
        Ok(f) => (f.bans.into_iter().filter(|b| omsi_net::is_hardware_id(&b.hw)).collect(), Some(f.realm).filter(|r| omsi_net::is_hardware_id(r)).unwrap_or_default(), true),
        Err(e) => {
            log::warn!("bans: {} cannot be read ({e}): no bans from it; a copy is kept beside it", path.display());
            let _ = std::fs::copy(path, path.with_extension("json.bak"));
            (Vec::new(), String::new(), false)
        }
    }
}

fn write(path: &Path, bans: &[Ban], realm: &str) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec_pretty(&File { version: 1, realm: realm.to_string(), bans: bans.to_vec() }).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}

/// The host's lists, one for the process (a dedicated server, or the game that hosts).
static STORE: OnceLock<Mutex<BanStore>> = OnceLock::new();

/// Start the host's lists. The first call wins.
pub fn install(store: BanStore) {
    let _ = STORE.set(Mutex::new(store));
}

/// The host's lists (None: nothing installed: a game that does not host).
pub fn global() -> Option<MutexGuard<'static, BanStore>> {
    STORE.get().map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Pass the lists on to the session's door if they changed since `seen` (the caller's own
/// counter, 0 at the start); true if they did.
pub fn sync(lan: &mut omsi_net::LanSession, seen: &mut u64) -> bool {
    let Some(mut s) = global() else { return false };
    s.refresh_owner();
    s.prune(omsi_admin::unix_now());
    if s.revision == *seen {
        return false;
    }
    *seen = s.revision;
    lan.set_ban_realm(&s.realm);
    lan.set_hardware_bans(s.door(omsi_admin::unix_now()));
    true
}

static REQUIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A host that turns away players whose game sends no hardware identifier
/// (`require_hardware_id` of `server.cfg`).
pub fn require_hardware_id(on: bool) {
    REQUIRED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The host loop, every frame or tick: the lists to the session's door when they changed.
pub fn sync_host(lan: &mut omsi_net::LanSession) {
    if lan.role != omsi_net::Role::Host {
        return;
    }
    lan.require_hardware_id(REQUIRED.load(std::sync::atomic::Ordering::Relaxed));
    // (what this session has been given is the session's own: a game that hosts a second time
    // in the same run starts it again with the lists)
    let mut seen = lan.hw_ban_revision;
    if sync(lan, &mut seen) {
        lan.hw_ban_revision = seen;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(n: u8) -> String {
        derive_hardware_id(&format!("machine-{n}"))
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-bans-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_identifier_is_a_hash_of_the_machine_not_the_machine() {
        let a = derive_hardware_id("4C4C4544-0042-3510-8056-B2C04F4D4D31");
        assert!(omsi_net::is_hardware_id(&a));
        assert_eq!(a, derive_hardware_id(" 4c4c4544-0042-3510-8056-b2c04f4d4d31\n"));
        assert_ne!(a, derive_hardware_id("4C4C4544-0042-3510-8056-B2C04F4D4D32"));
        assert!(!a.contains("4c4c"));
        for bad in ["", "abc", &"g".repeat(32), &"A".repeat(32), &"a".repeat(33)] {
            assert!(!omsi_net::is_hardware_id(bad), "{bad}");
        }
    }

    #[test]
    fn a_ban_matches_its_machine_until_it_ends() {
        let mut s = BanStore::open(None, None);
        let b = s.add(Scope::Session, &hw(1), "Pichau", "griefing\nkick 3", "Admin (#2)", Some(1100), 1000).unwrap();
        assert_eq!(b.reason, "griefing kick 3");
        assert!(s.find(&hw(1), 1050).is_some());
        assert!(s.find(&hw(2), 1050).is_none());
        assert!(s.find(&hw(1), 1100).is_none(), "it ended");
        assert!(s.door(1050).iter().any(|d| d.0 == hw(1)));
        assert!(s.door(1200).is_empty());
        // a machine banned again has one entry, with the newer reason
        s.add(Scope::Session, &hw(1), "Pichau", "again", "x", None, 1010).unwrap();
        assert_eq!(s.active(1010).len(), 1);
        assert_eq!(s.active(1010)[0].reason, "again");
        assert!(s.add(Scope::Session, "nonsense", "x", "", "", None, 0).is_err());
    }

    #[test]
    fn the_realm_is_kept_in_the_owners_list_and_never_overwrites_an_unreadable_one() {
        let d = dir("realm");
        let owner = d.join("bans-owner.json");
        let realm = BanStore::open(None, Some(owner.clone())).realm;
        assert!(omsi_net::is_hardware_id(&realm));
        assert_eq!(BanStore::open(None, Some(owner.clone())).realm, realm);
        assert_ne!(BanStore::open(None, None).realm, realm);
        let broken = d.join("broken.json");
        std::fs::write(&broken, "{ not json").unwrap();
        assert!(omsi_net::is_hardware_id(&BanStore::open(None, Some(broken.clone())).realm));
        assert_eq!(std::fs::read_to_string(&broken).unwrap(), "{ not json");
    }

    #[test]
    fn a_ban_is_lifted_by_its_id_and_lists_survive_a_restart() {
        let d = dir("persist");
        let (sp, op) = (d.join("bans.json"), d.join("owner.json"));
        let mut s = BanStore::open(Some(sp.clone()), Some(op.clone()));
        let a = s.add(Scope::Session, &hw(1), "A", "r1", "me", None, 5).unwrap();
        let b = s.add(Scope::Owner, &hw(2), "B", "r2", "me", None, 6).unwrap();
        assert_ne!(a.id, b.id);
        let again = BanStore::open(Some(sp.clone()), Some(op.clone()));
        assert_eq!(again.active(10).len(), 2);
        assert_eq!(again.find(&hw(2), 10).map(|x| x.scope), Some(Scope::Owner));
        assert!(s.remove("nothing").is_err());
        assert_eq!(s.remove(&a.id).unwrap().hw, hw(1));
        assert_eq!(BanStore::open(Some(sp), Some(op)).active(10).len(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    /// "All my sessions": a ban in the owner's file is held by another session that reads the
    /// same file, even one that was already running, once it looks again.
    #[test]
    fn the_owners_list_reaches_the_other_sessions() {
        let d = dir("owner");
        let op = d.join("owner.json");
        let mut one = BanStore::open(Some(d.join("one.json")), Some(op.clone()));
        let mut two = BanStore::open(Some(d.join("two.json")), Some(op.clone()));
        assert!(two.find(&hw(3), 10).is_none());
        one.add(Scope::Owner, &hw(3), "C", "all", "me", None, 5).unwrap();
        // (a session-only ban of one stays out of the other)
        one.add(Scope::Session, &hw(4), "D", "here", "me", None, 5).unwrap();
        let rev = two.revision;
        two.refresh_owner();
        assert!(two.revision > rev);
        assert!(two.find(&hw(3), 10).is_some());
        assert!(two.find(&hw(4), 10).is_none());
        // lifted from the second session, gone from the first
        let id = two.find(&hw(3), 10).unwrap().id.clone();
        two.remove(&id).unwrap();
        one.refresh_owner();
        assert!(one.find(&hw(3), 10).is_none());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_file_that_cannot_be_read_gives_no_bans_and_is_kept() {
        let d = dir("broken");
        let p = d.join("bans.json");
        std::fs::write(&p, "{ not json").unwrap();
        let s = BanStore::open(Some(p.clone()), None);
        assert!(s.is_empty());
        assert!(p.with_extension("json.bak").is_file());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn scopes_are_read_from_words() {
        assert_eq!(Scope::parse("session"), Some(Scope::Session));
        assert_eq!(Scope::parse("ALL"), Some(Scope::Owner));
        assert_eq!(Scope::parse("run"), None);
    }

    #[test]
    fn switching_the_hardware_id_off_holds_for_the_next_session_of_the_same_process() {
        let (a, b) = ("ab".repeat(16), "cd".repeat(16));
        allow_hardware_id(true);
        omsi_net::set_hardware_id(&a);
        assert_eq!(omsi_net::hardware_id(), Some(a));
        allow_hardware_id(false);
        announce_local_hardware_id();
        assert_eq!(omsi_net::hardware_id(), None);
        allow_hardware_id(true);
        omsi_net::set_hardware_id(&b);
        assert_eq!(omsi_net::hardware_id(), Some(b));
    }
}
