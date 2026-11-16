use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const FILES: [&str; 2] = ["keyboard.cfg", "gamectrler.cfg"];
const INDEX: &str = "profiles.json";
const DIR: &str = "profiles";
pub const FIRST: &str = "Default";

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
struct Index {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    buses: BTreeMap<String, String>,
}

pub fn inputs_dir() -> Option<PathBuf> {
    omsi_cfg::own_input(FILES[0]).and_then(|p| p.parent().map(Path::to_path_buf))
}

fn name_ok(n: &str) -> bool {
    let n = n.trim();
    !n.is_empty()
        && n.chars().count() <= 40
        && n != "."
        && n != ".."
        && !n
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

pub fn bus_key(bus: &str) -> String {
    bus.trim().replace('\\', "/").to_lowercase()
}

fn read(inputs: &Path) -> Index {
    std::fs::read_to_string(inputs.join(INDEX))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write(inputs: &Path, ix: &Index) -> Result<()> {
    std::fs::create_dir_all(inputs)?;
    let tmp = inputs.join(format!("{INDEX}.tmp"));
    std::fs::write(&tmp, serde_json::to_string_pretty(ix)?)?;
    std::fs::rename(tmp, inputs.join(INDEX))?;
    Ok(())
}

fn folder(inputs: &Path, name: &str) -> PathBuf {
    inputs.join(DIR).join(name.trim())
}

fn names(inputs: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(inputs.join(DIR))
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|n| n.to_lowercase());
    v
}

// (the game takes the newer of each own file and the content folder's, `omsi_cfg::input_in_use`)
fn save_in_use(inputs: &Path, rival: Option<&Path>, to: &Path) -> Result<()> {
    copy_files_then(inputs, rival, to, None, || Ok(()))
}

fn in_use(own: PathBuf, other: Option<PathBuf>) -> PathBuf {
    let modified = |p: &Path| {
        std::fs::metadata(p)
            .ok()
            .filter(|m| m.is_file())
            .and_then(|m| m.modified().ok())
    };
    match (other, modified(&own)) {
        (Some(o), mine) if modified(&o).is_some_and(|t| mine.is_none_or(|m| t > m)) => o,
        _ => own,
    }
}

// (a file the source lacks is written as the game's default: without one, the content
// folder's copy would be the one in use; all files are staged first, and when `then` fails
// with them in place the old ones go back)
fn copy_files_then(
    from: &Path,
    alt: Option<&Path>,
    to: &Path,
    rival: Option<&Path>,
    then: impl FnOnce() -> Result<()>,
) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let mut staged = Vec::new();
    let result = stage(from, alt, to, rival, &mut staged).and_then(|_| put(to, &staged, then));
    for (tmp, _) in &staged {
        let _ = std::fs::remove_file(tmp);
    }
    result
}

fn stage(
    from: &Path,
    alt: Option<&Path>,
    to: &Path,
    rival: Option<&Path>,
    staged: &mut Vec<(PathBuf, &'static str)>,
) -> Result<()> {
    for f in FILES {
        let a = in_use(from.join(f), alt.map(|d| d.join(f)));
        let tmp = to.join(format!("{f}.tmp"));
        if a.is_file() {
            std::fs::copy(&a, &tmp)?;
        } else if let Some(bytes) = default_file(f) {
            std::fs::write(&tmp, bytes)?;
        } else {
            continue;
        }
        staged.push((tmp.clone(), f));
        stamp(&tmp, rival.map(|r| r.join(f)).as_deref())?;
    }
    Ok(())
}

fn put(
    to: &Path,
    staged: &[(PathBuf, &'static str)],
    then: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let bak = |f: &str| to.join(format!("{f}.bak"));
    let mut moved = Vec::new();
    let mut installed = Vec::new();
    let result = (|| -> Result<()> {
        for f in FILES {
            let b = to.join(f);
            if b.is_file() {
                std::fs::rename(&b, bak(f))?;
                moved.push(f);
            }
        }
        for (tmp, f) in staged {
            std::fs::rename(tmp, to.join(f))?;
            installed.push(*f);
        }
        then()
    })();
    match &result {
        Ok(()) => {
            for f in &moved {
                let _ = std::fs::remove_file(bak(*f));
            }
        }
        Err(_) => {
            for f in installed.iter().filter(|f| !moved.contains(*f)) {
                let _ = std::fs::remove_file(to.join(f));
            }
            for f in &moved {
                let _ = std::fs::rename(bak(*f), to.join(f));
            }
        }
    }
    result
}

// (`omsi_cfg::input_in_use` takes the newest of the own and the content folder's file, and a
// copy may keep its source's modified time)
fn stamp(file: &Path, rival: Option<&Path>) -> Result<()> {
    let past = rival
        .and_then(|r| std::fs::metadata(r).ok())
        .and_then(|m| m.modified().ok())
        .map(|t| t + Duration::from_secs(1));
    let at = past.map_or(SystemTime::now(), |t| t.max(SystemTime::now()));
    std::fs::File::options()
        .write(true)
        .open(file)?
        .set_modified(at)?;
    Ok(())
}

fn default_file(name: &str) -> Option<Vec<u8>> {
    match name {
        "gamectrler.cfg" => Some(Vec::new()),
        _ => crate::root()
            .ok()
            .and_then(|r| std::fs::read(omsi_cfg::original_keyboard_cfg(&r)).ok()),
    }
}

pub fn list_in(inputs: &Path) -> Value {
    let ix = read(inputs);
    json!({ "active": ix.active, "default": ix.default, "profiles": names(inputs), "buses": ix.buses })
}

pub fn create_in(inputs: &Path, rival: Option<&Path>, name: &str) -> Result<()> {
    if !name_ok(name) {
        bail!("a profile's name: up to 40 characters, none of / \\ : * ? \" < > |");
    }
    let name = name.trim();
    if names(inputs).iter().any(|n| n.eq_ignore_ascii_case(name)) {
        bail!("there is a profile {name} already");
    }
    let mut ix = read(inputs);
    match ix.active.clone() {
        Some(a) => save_in_use(inputs, rival, &folder(inputs, &a))?,
        None if !name.eq_ignore_ascii_case(FIRST) && !folder(inputs, FIRST).exists() => {
            save_in_use(inputs, rival, &folder(inputs, FIRST))?;
            ix.default.get_or_insert_with(|| FIRST.to_string());
        }
        None => {}
    }
    save_in_use(inputs, rival, &folder(inputs, name))?;
    ix.active = Some(name.to_string());
    write(inputs, &ix)
}

pub fn switch_in(inputs: &Path, rival: Option<&Path>, name: &str) -> Result<()> {
    let mut ix = read(inputs);
    let target = folder(inputs, name);
    if !name_ok(name) || !target.is_dir() {
        bail!("no profile {name}");
    }
    if ix.active.as_deref() == Some(name.trim()) {
        return Ok(());
    }
    if let Some(a) = ix.active.as_deref().filter(|a| folder(inputs, a).is_dir()) {
        save_in_use(inputs, rival, &folder(inputs, a))?;
    }
    ix.active = Some(name.trim().to_string());
    copy_files_then(&target, None, inputs, rival, || write(inputs, &ix))
}

pub fn rename_in(inputs: &Path, old: &str, new: &str) -> Result<()> {
    if !name_ok(new) {
        bail!("a profile's name: up to 40 characters, none of / \\ : * ? \" < > |");
    }
    let new = new.trim();
    if !name_ok(old) || !folder(inputs, old).is_dir() {
        bail!("no profile {old}");
    }
    if names(inputs)
        .iter()
        .any(|n| n.eq_ignore_ascii_case(new) && !n.eq_ignore_ascii_case(old.trim()))
    {
        bail!("there is a profile {new} already");
    }
    std::fs::rename(folder(inputs, old), folder(inputs, new))?;
    let mut ix = read(inputs);
    let old = old.trim();
    for slot in [&mut ix.active, &mut ix.default] {
        if slot.as_deref() == Some(old) {
            *slot = Some(new.to_string());
        }
    }
    for p in ix.buses.values_mut() {
        if p == old {
            *p = new.to_string();
        }
    }
    write(inputs, &ix).inspect_err(|_| {
        let _ = std::fs::rename(folder(inputs, new), folder(inputs, old));
    })
}

pub fn delete_in(inputs: &Path, name: &str) -> Result<()> {
    let mut ix = read(inputs);
    let name = name.trim();
    if ix.active.as_deref() == Some(name) {
        bail!("the profile in use cannot be deleted: switch to another one first");
    }
    if !name_ok(name) || !folder(inputs, name).is_dir() {
        bail!("no profile {name}");
    }
    // (set aside first: an index that cannot be written puts it back)
    let aside = inputs.join(format!(".{name}.deleted"));
    let _ = std::fs::remove_dir_all(&aside);
    std::fs::rename(folder(inputs, name), &aside)?;
    if ix.default.as_deref() == Some(name) {
        ix.default = None;
    }
    ix.buses.retain(|_, p| p != name);
    if let Err(e) = write(inputs, &ix) {
        let _ = std::fs::rename(&aside, folder(inputs, name));
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&aside);
    Ok(())
}

pub fn set_default_in(inputs: &Path, name: Option<&str>) -> Result<()> {
    let mut ix = read(inputs);
    if let Some(n) = name.filter(|n| !name_ok(n) || !folder(inputs, n).is_dir()) {
        bail!("no profile {n}");
    }
    ix.default = name.map(|n| n.trim().to_string());
    write(inputs, &ix)
}

pub fn assign_in(inputs: &Path, bus: &str, name: Option<&str>) -> Result<()> {
    let key = bus_key(bus);
    if key.is_empty() {
        bail!("no bus chosen");
    }
    let mut ix = read(inputs);
    match name {
        Some(n) if !name_ok(n) || !folder(inputs, n).is_dir() => bail!("no profile {n}"),
        Some(n) => {
            ix.buses.insert(key, n.trim().to_string());
        }
        None => {
            ix.buses.remove(&key);
        }
    }
    write(inputs, &ix)
}

pub fn for_bus_in(inputs: &Path, rival: Option<&Path>, bus: &str) -> Result<Option<String>> {
    let ix = read(inputs);
    let Some(want) = ix.buses.get(&bus_key(bus)).or(ix.default.as_ref()).cloned() else {
        return Ok(None);
    };
    if ix.active.as_deref() == Some(want.as_str()) || !folder(inputs, &want).is_dir() {
        return Ok(None);
    }
    switch_in(inputs, rival, &want)?;
    Ok(Some(want))
}

fn dir() -> Result<PathBuf> {
    inputs_dir().ok_or_else(|| anyhow!("no home folder for the controls"))
}

fn rival() -> Option<PathBuf> {
    omsi_cfg::content_dir()
        .map(|c| c.join("Inputs"))
        .filter(|r| inputs_dir().as_ref() != Some(r))
}

// (a running game would read and save the other profile's files)
fn no_game_running() -> Result<()> {
    if crate::instances::list().iter().any(|i| i.running) {
        bail!("a game is running: switch the controls profile once it has ended");
    }
    Ok(())
}

pub fn list() -> Value {
    dir()
        .map(|d| list_in(&d))
        .unwrap_or_else(|_| json!({ "active": null, "default": null, "profiles": [], "buses": {} }))
}
pub fn create(name: &str) -> Result<()> {
    create_in(&dir()?, rival().as_deref(), name)
}
pub fn switch(name: &str) -> Result<()> {
    no_game_running()?;
    switch_in(&dir()?, rival().as_deref(), name)
}
pub fn rename(old: &str, new: &str) -> Result<()> {
    rename_in(&dir()?, old, new)
}
pub fn delete(name: &str) -> Result<()> {
    delete_in(&dir()?, name)
}
pub fn set_default(name: Option<&str>) -> Result<()> {
    set_default_in(&dir()?, name)
}
pub fn assign(bus: &str, name: Option<&str>) -> Result<()> {
    assign_in(&dir()?, bus, name)
}
/// `Ok(None)` while a game runs: the new one shares the files in use.
pub fn for_bus(bus: &str) -> Result<Option<String>> {
    if let Err(e) = no_game_running() {
        crate::log_to_file(&format!("controls profile for {bus}: {e:#}"));
        return Ok(None);
    }
    let Ok(d) = dir() else { return Ok(None) };
    for_bus_in(&d, rival().as_deref(), bus)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(d: &Path) -> String {
        std::fs::read_to_string(d.join("keyboard.cfg")).unwrap_or_default()
    }

    #[test]
    fn profiles_keep_their_keys_switch_and_follow_the_bus() {
        let d = std::env::temp_dir().join(format!("input-profiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("keyboard.cfg"), "original").unwrap();

        create_in(&d, None, "Wheel").unwrap();
        assert_eq!(list_in(&d)["profiles"], json!(["Default", "Wheel"]));
        assert_eq!(list_in(&d)["default"], "Default");
        assert_eq!(list_in(&d)["active"], "Wheel");
        std::fs::write(d.join("keyboard.cfg"), "wheel keys").unwrap();
        std::fs::write(d.join("gamectrler.cfg"), "wheel pad").unwrap();

        switch_in(&d, None, "Default").unwrap();
        assert_eq!(kb(&d), "original");
        assert_eq!(
            std::fs::read_to_string(d.join("gamectrler.cfg")).unwrap(),
            "",
            "Default had no controller file: none set up"
        );
        switch_in(&d, None, "Wheel").unwrap();
        assert_eq!(kb(&d), "wheel keys");
        assert!(create_in(&d, None, "wheel").is_err() && create_in(&d, None, "a/b").is_err());

        // a bus with its own, another with the default
        assign_in(&d, "Vehicles\\MAN_NL202\\NL202.bus", Some("Wheel")).unwrap();
        switch_in(&d, None, "Default").unwrap();
        assert_eq!(
            for_bus_in(&d, None, "vehicles/man_nl202/nl202.bus")
                .unwrap()
                .as_deref(),
            Some("Wheel")
        );
        assert_eq!(kb(&d), "wheel keys");
        assert_eq!(
            for_bus_in(&d, None, "Vehicles/Other/Other.bus")
                .unwrap()
                .as_deref(),
            Some("Default")
        );
        assert_eq!(kb(&d), "original");

        assert!(delete_in(&d, "Default").is_err(), "in use");
        rename_in(&d, "Wheel", "G29").unwrap();
        assert_eq!(list_in(&d)["buses"]["vehicles/man_nl202/nl202.bus"], "G29");
        delete_in(&d, "G29").unwrap();
        assert_eq!(list_in(&d)["buses"], json!({}));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_switched_in_file_is_newer_than_the_content_folders() {
        let t = std::env::temp_dir().join(format!("input-profiles-newer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let (d, content) = (t.join("own"), t.join("content").join("Inputs"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::create_dir_all(&content).unwrap();
        let at = |p: &Path, when: SystemTime| {
            std::fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(when)
                .unwrap()
        };
        let mtime = |p: &Path| std::fs::metadata(p).unwrap().modified().unwrap();
        std::fs::write(d.join("keyboard.cfg"), "first").unwrap();
        create_in(&d, None, "Old").unwrap();
        create_in(&d, None, "New").unwrap();
        std::fs::write(d.join("keyboard.cfg"), "second").unwrap();
        let old_kb = folder(&d, "Old").join("keyboard.cfg");
        at(
            &old_kb,
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000),
        );
        let later = SystemTime::now() + Duration::from_secs(3600);
        for f in FILES {
            std::fs::write(content.join(f), "content").unwrap();
            at(&content.join(f), later);
        }
        switch_in(&d, Some(&content), "Old").unwrap();
        assert_eq!(kb(&d), "first");
        for f in FILES {
            assert!(mtime(&d.join(f)) > later, "{f}");
            assert!(mtime(&d.join(f)) > mtime(&content.join(f)), "{f}");
        }
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn names_reaching_out_of_the_profiles_are_refused() {
        let d = std::env::temp_dir().join(format!("input-profiles-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("keyboard.cfg"), "keys").unwrap();
        create_in(&d, None, "Wheel").unwrap();
        for bad in ["..", ".", "", "a/b", "../Wheel"] {
            assert!(rename_in(&d, bad, "Other").is_err(), "{bad:?}");
            assert!(set_default_in(&d, Some(bad)).is_err(), "{bad:?}");
            assert!(
                assign_in(&d, "Vehicles/Bus/Bus.bus", Some(bad)).is_err(),
                "{bad:?}"
            );
            assert!(switch_in(&d, None, bad).is_err(), "{bad:?}");
            assert!(delete_in(&d, bad).is_err(), "{bad:?}");
        }
        assert!(d.join(DIR).join("Wheel").is_dir() && d.join("keyboard.cfg").is_file());
        assert_eq!(list_in(&d)["buses"], json!({}));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_switch_leaves_the_files_in_use_as_they_were() {
        let d = std::env::temp_dir().join(format!("input-profiles-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("keyboard.cfg"), "original").unwrap();
        std::fs::write(d.join("gamectrler.cfg"), "pad").unwrap();
        create_in(&d, None, "Wheel").unwrap();
        std::fs::write(d.join("keyboard.cfg"), "wheel keys").unwrap();
        std::fs::write(d.join("gamectrler.cfg"), "wheel pad").unwrap();
        switch_in(&d, None, "Default").unwrap();
        assign_in(&d, "Bus/A.bus", Some("Wheel")).unwrap();

        // the second file cannot be staged: a folder stands where its temporary file goes
        std::fs::create_dir_all(d.join("gamectrler.cfg.tmp")).unwrap();
        assert!(for_bus_in(&d, None, "Bus/A.bus").is_err());
        assert_eq!(kb(&d), "original");
        assert_eq!(
            std::fs::read_to_string(d.join("gamectrler.cfg")).unwrap(),
            "pad"
        );
        assert!(!d.join("keyboard.cfg.tmp").exists() && !d.join("keyboard.cfg.bak").exists());
        assert_eq!(list_in(&d)["active"], "Default");

        std::fs::remove_dir_all(d.join("gamectrler.cfg.tmp")).unwrap();
        assert_eq!(
            for_bus_in(&d, None, "Bus/A.bus").unwrap().as_deref(),
            Some("Wheel")
        );
        assert_eq!(kb(&d), "wheel keys");
        assert_eq!(
            std::fs::read_to_string(d.join("gamectrler.cfg")).unwrap(),
            "wheel pad"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Profiles `Default` (keys "original", pad "pad") and `Wheel` (keys "wheel keys", pad
    /// "wheel pad"), with `Default` in use.
    fn two_profiles(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("input-profiles-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("keyboard.cfg"), "original").unwrap();
        std::fs::write(d.join("gamectrler.cfg"), "pad").unwrap();
        create_in(&d, None, "Wheel").unwrap();
        std::fs::write(d.join("keyboard.cfg"), "wheel keys").unwrap();
        std::fs::write(d.join("gamectrler.cfg"), "wheel pad").unwrap();
        switch_in(&d, None, "Default").unwrap();
        d
    }

    fn pad(d: &Path) -> String {
        std::fs::read_to_string(d.join("gamectrler.cfg")).unwrap_or_default()
    }

    #[test]
    fn a_file_that_cannot_be_set_aside_leaves_every_file_in_use() {
        let d = two_profiles("backup");
        // a folder stands where the second file is set aside
        std::fs::create_dir_all(d.join("gamectrler.cfg.bak")).unwrap();
        assert!(switch_in(&d, None, "Wheel").is_err());
        assert_eq!(
            (kb(&d), pad(&d)),
            (String::from("original"), String::from("pad"))
        );
        assert_eq!(list_in(&d)["active"], "Default");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_index_that_cannot_be_written_puts_the_files_back() {
        let d = two_profiles("index");
        // a folder stands where profiles.json is written first
        std::fs::create_dir_all(d.join(format!("{INDEX}.tmp"))).unwrap();
        assert!(switch_in(&d, None, "Wheel").is_err());
        assert_eq!(
            (kb(&d), pad(&d)),
            (String::from("original"), String::from("pad"))
        );
        assert_eq!(list_in(&d)["active"], "Default");

        std::fs::remove_dir_all(d.join(format!("{INDEX}.tmp"))).unwrap();
        switch_in(&d, None, "Wheel").unwrap();
        assert_eq!(
            (kb(&d), pad(&d)),
            (String::from("wheel keys"), String::from("wheel pad"))
        );
        switch_in(&d, None, "Default").unwrap();
        assert_eq!(
            (kb(&d), pad(&d)),
            (String::from("original"), String::from("pad"))
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_rename_or_delete_whose_index_cannot_be_written_changes_nothing() {
        let d = two_profiles("rename");
        assign_in(&d, "Bus/A.bus", Some("Wheel")).unwrap();
        set_default_in(&d, Some("Wheel")).unwrap();
        let before = list_in(&d);
        std::fs::create_dir_all(d.join(format!("{INDEX}.tmp"))).unwrap();

        assert!(rename_in(&d, "Wheel", "Pad").is_err());
        assert!(folder(&d, "Wheel").is_dir() && !folder(&d, "Pad").exists());
        assert!(delete_in(&d, "Wheel").is_err());
        assert_eq!(
            std::fs::read_to_string(folder(&d, "Wheel").join("keyboard.cfg")).unwrap(),
            "wheel keys"
        );
        assert_eq!(list_in(&d), before);

        std::fs::remove_dir_all(d.join(format!("{INDEX}.tmp"))).unwrap();
        rename_in(&d, "Wheel", "Pad").unwrap();
        assert_eq!(list_in(&d)["buses"]["bus/a.bus"], "Pad");
        delete_in(&d, "Pad").unwrap();
        assert_eq!(list_in(&d)["profiles"], json!(["Default"]));
        assert!(
            std::fs::read_dir(&d)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".deleted"))
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_new_profile_keeps_the_controls_in_use_from_the_content_folder() {
        let d = std::env::temp_dir().join(format!("input-profiles-rival-{}", std::process::id()));
        let content = d.join("content/Inputs");
        let inputs = d.join("own");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&content).unwrap();
        std::fs::create_dir_all(&inputs).unwrap();
        let at = |p: &Path, secs: u64| {
            std::fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
                .unwrap();
        };
        // the game uses the content folder's newer keys and the own, newer controller file
        std::fs::write(inputs.join("keyboard.cfg"), "own keys").unwrap();
        at(&inputs.join("keyboard.cfg"), 1_000);
        std::fs::write(content.join("keyboard.cfg"), "content keys").unwrap();
        at(&content.join("keyboard.cfg"), 2_000);
        std::fs::write(inputs.join("gamectrler.cfg"), "own pad").unwrap();
        at(&inputs.join("gamectrler.cfg"), 3_000);
        std::fs::write(content.join("gamectrler.cfg"), "content pad").unwrap();
        at(&content.join("gamectrler.cfg"), 2_000);

        create_in(&inputs, Some(&content), "Wheel").unwrap();
        for p in ["Default", "Wheel"] {
            assert_eq!(kb(&folder(&inputs, p)), "content keys", "{p}");
            assert_eq!(
                std::fs::read_to_string(folder(&inputs, p).join("gamectrler.cfg")).unwrap(),
                "own pad",
                "{p}"
            );
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
