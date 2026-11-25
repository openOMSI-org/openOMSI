use super::*;

fn entries(names: &[&str]) -> Vec<Entry> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| Entry {
            rel: n.to_string(),
            size: 10,
            index: i,
        })
        .collect()
}

fn dests(p: &Plan) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = p.maps.iter().map(|m| (m.dest.clone(), m.aside)).collect();
    v.sort();
    v
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("omsi-launcher-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn plans() {
    let content = tmp("plans");
    std::fs::create_dir_all(content.join("Vehicles/MAN_SD202")).unwrap();
    let inst = Installed {
        content: &content,
        root: None,
    };
    // an OMSI-style pack one folder down, with a read-me
    let p = plan(
        &entries(&[
            "Pack/Vehicles/Foo/foo.bus",
            "Pack/Vehicles/Foo/Model/model.cfg",
            "Pack/Sceneryobjects/X/x.sco",
            "Pack/readme.txt",
        ]),
        "Pack",
        &inst,
    );
    assert_eq!(
        dests(&p),
        vec![
            ("Sceneryobjects".into(), false),
            ("Vehicles/Foo".into(), false)
        ]
    );
    assert!(
        p.notes.iter().any(|n| n.contains("1 file")),
        "{:?}",
        p.notes
    );
    // a lone bus folder: its Sound / Script / Texture are not the content folders
    let p = plan(
        &entries(&[
            "Foo/foo.bus",
            "Foo/Sound/a.wav",
            "Foo/Script/a.osc",
            "Foo/Texture/a.dds",
        ]),
        "Foo",
        &inst,
    );
    assert_eq!(dests(&p), vec![("Vehicles/Foo".into(), false)]);
    assert_eq!(p.maps[0].files.len(), 4);
    assert!(p.maps[0].files.iter().any(|f| f.1 == "Sound/a.wav"));
    // a bus folder at the top of the archive
    let p = plan(
        &entries(&[
            "foo.bus",
            "Model/model.cfg",
            "Sound/sound.cfg",
            "Script/main.osc",
            "Texture/a.dds",
        ]),
        "MyBus",
        &inst,
    );
    assert_eq!(dests(&p), vec![("Vehicles/MyBus".into(), false)]);
    // a map
    let p = plan(
        &entries(&[
            "Ahlheim/global.cfg",
            "Ahlheim/tile_0_0.map",
            "Ahlheim/texture/a.dds",
        ]),
        "AhlheimV5",
        &inst,
    );
    assert_eq!(dests(&p), vec![("maps/Ahlheim".into(), false)]);
    // repaints: for an installed bus (any case) merged, for a missing one kept aside
    let p = plan(
        &entries(&[
            "Vehicles/man_sd202/Texture/Repaint/x.dds",
            "Vehicles/man_sd202/Model/x.cti",
            "Vehicles/O305/Texture/y.dds",
        ]),
        "Repaints",
        &inst,
    );
    assert_eq!(
        dests(&p),
        vec![
            ("Vehicles/MAN_SD202".into(), false),
            ("Vehicles/O305".into(), true)
        ]
    );
    assert!(p.warnings.iter().any(|w| w.contains("O305")));
    // nothing recognisable
    let p = plan(&entries(&["docs/a.pdf", "b.txt"]), "Junk", &inst);
    assert!(p.maps.is_empty());
    let _ = std::fs::remove_dir_all(&content);
}

fn write_zip(path: &Path, files: &[(&str, usize)]) {
    let f = std::fs::File::create(path).unwrap();
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, size) in files {
        z.start_file(*name, o).unwrap();
        let data: Vec<u8> = (0..*size).map(|i| (i * 7 % 251) as u8).collect();
        z.write_all(&data).unwrap();
    }
    z.finish().unwrap();
}

#[test]
fn install_zip_moves_into_place_and_cleans_up() {
    let dir = tmp("zip");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let zip = dir.join("Foo Bus.zip");
    write_zip(
        &zip,
        &[
            ("Foo/foo.bus", 100),
            ("Foo/Model/model.cfg", 2000),
            ("Foo/Texture/a.dds", 300_000),
            ("readme.txt", 10),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip.clone(),
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert_eq!(p.installed, vec!["Vehicles/Foo".to_string()]);
    assert_eq!(
        std::fs::metadata(content.join("Vehicles/Foo/Texture/a.dds"))
            .unwrap()
            .len(),
        300_000
    );
    assert!(!content.join(STAGING).exists(), "staging removed");
    // again: merged over the existing folder
    let p = run_blocking(
        content.clone(),
        None,
        zip.clone(),
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert!(
        p.report
            .iter()
            .any(|l| l.contains("3 existing file(s) replaced")),
        "{:?}",
        p.report
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pack's fonts go into the shared Fonts folder: each of them is the mod's, beside its
/// bus, and a font that was there before is not.
#[test]
fn a_packs_fonts_are_noted_as_its_own() {
    let dir = tmp("fonts");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    std::fs::write(content.join("Fonts/old.oft"), b"x").unwrap();
    let zip = dir.join("Bus Pack.zip");
    write_zip(
        &zip,
        &[
            ("Pack/Vehicles/Foo/foo.bus", 100),
            ("Pack/Fonts/new.oft", 50),
            ("Pack/Fonts/new.bmp", 50),
            ("Pack/Fonts/old.oft", 50),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip,
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    let m = crate::mods::list(&content)
        .into_iter()
        .find(|m| m.noted)
        .expect("noted");
    let mut paths = m.paths.clone();
    paths.sort();
    assert_eq!(paths, ["Fonts/new.bmp", "Fonts/new.oft", "Vehicles/Foo"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_zip_is_used_in_place() {
    let dir = tmp("inplace");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    // laid out like OMSI 2 (one folder down, with a read-me beside it)
    let zip = dir.join("Big Map.zip");
    write_zip(
        &zip,
        &[
            ("OMSI 2/maps/Big/global.cfg", 100),
            ("OMSI 2/Sceneryobjects/Big/x.sco", 2000),
            ("readme.txt", 10),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip.clone(),
        InstallMode::InPlace,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert_eq!(p.mode, InstallMode::InPlace);
    let placed = content.join(ARCHIVES).join("Big Map.zip");
    assert_eq!(
        std::fs::metadata(&placed).unwrap().len(),
        std::fs::metadata(&zip).unwrap().len()
    );
    assert!(
        zip.exists(),
        "a linked or copied archive stays where it was"
    );
    assert!(!content.join("maps/Big").exists(), "nothing unpacked");
    assert!(!content.join(STAGING).exists(), "staging removed");
    assert!(
        p.installed
            .iter()
            .any(|i| i.starts_with("maps (in Archives/Big Map.zip)")),
        "{:?}",
        p.installed
    );
    // the game's reader finds the map in it
    let archive = omsi_cfg::vfs::ZipArchive::open(&placed).unwrap();
    assert_eq!(archive.prefix(), "OMSI 2/");
    // again: already there
    let p = run_blocking(
        content.clone(),
        None,
        zip.clone(),
        InstallMode::InPlace,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    // a lone bus folder cannot be read in place: explicit in-place fails, auto unpacks
    let lone = dir.join("Foo Bus.zip");
    write_zip(&lone, &[("Foo/foo.bus", 100), ("Foo/Texture/a.dds", 3000)]);
    let p = run_blocking(
        content.clone(),
        None,
        lone.clone(),
        InstallMode::InPlace,
        None,
        false,
    );
    assert_eq!(p.state, "failed", "{p:?}");
    assert!(p.message.contains("unpacked"), "{}", p.message);
    let p = run_blocking(
        content.clone(),
        None,
        lone.clone(),
        InstallMode::Auto,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert_eq!(p.mode, InstallMode::Extract);
    assert!(content.join("Vehicles/Foo/foo.bus").exists());
    // what the page is told before
    let info = inspect(&content, None, &zip).unwrap();
    assert!(
        info.is_zip && info.in_place_ok && info.unpacked_bytes == 2100,
        "{info:?}"
    );
    let info = inspect(&content, None, &lone).unwrap();
    assert!(!info.in_place_ok, "{info:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn downloaded_plugins_are_not_enabled() {
    let dir = tmp("plugins");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let zip = dir.join("Pack.zip");
    write_zip(
        &zip,
        &[
            ("Vehicles/Foo/foo.bus", 10),
            ("Plugins/evil.dll", 100),
            ("Plugins/evil.opl", 10),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip,
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert!(content.join("Vehicles/Foo/foo.bus").exists());
    assert!(
        !content.join("Plugins/evil.dll").exists(),
        "never where the game loads plugins"
    );
    assert!(
        content
            .join("Mods")
            .join(PLUGINS_HELD)
            .join("evil.dll")
            .exists()
    );
    assert!(
        p.warnings.iter().any(|w| w.contains("not enabled")),
        "{:?}",
        p.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_never_replaces_an_installed_folder() {
    let dir = tmp("file-over-folder");
    let (from, to) = (dir.join("from"), dir.join("to"));
    std::fs::create_dir_all(to.join("Bus")).unwrap();
    std::fs::write(to.join("Bus").join("keep.bus"), b"x").unwrap();
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("Bus"), b"a file").unwrap();
    assert!(move_into(&from, &to).is_err());
    assert!(to.join("Bus").join("keep.bus").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancel_removes_partial_files() {
    let dir = tmp("cancel");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let zip = dir.join("Big.zip");
    let files: Vec<(String, usize)> = (0..4)
        .map(|i| (format!("Vehicles/Big/Texture/t{i}.dds"), 100))
        .collect();
    let refs: Vec<(&str, usize)> = files.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    let mut with_bus = refs.clone();
    with_bus.push(("Vehicles/Big/big.bus", 10));
    write_zip(&zip, &with_bus);
    // Pause after a file is staged: a fast disk must not finish before the test
    // requests cancellation, and cancellation must clean up actual partial files.
    let job = start_inner(content.clone(), None, zip, InstallMode::Extract, false, 1);
    wait_for(&job, 1);
    assert_eq!(job.snapshot().files_done, 1);
    assert!(staging_dir(&content, job.id).exists());
    job.cancel();
    let p = wait_done(&job);
    assert_eq!(p.state, "cancelled", "{p:?}");
    assert!(!content.join("Vehicles/Big").exists());
    assert!(!content.join(STAGING).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// An inbox folder of `n` textures and a bus file.
fn inbox_bus(content: &Path, n: usize) -> PathBuf {
    let src = content.join("Mods").join("Big Bus");
    std::fs::create_dir_all(src.join("Texture")).unwrap();
    std::fs::write(src.join("big.bus"), b"[friendlyname]").unwrap();
    std::fs::write(src.join("readme.txt"), b"read me").unwrap();
    for i in 0..n {
        std::fs::write(src.join("Texture").join(format!("t{i}.dds")), [7u8; 64]).unwrap();
    }
    src
}

fn wait_for(job: &Job, files: u64) {
    while job.snapshot().files_done < files && job.snapshot().finished.is_none() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn wait_done(job: &Job) -> Progress {
    while job.snapshot().finished.is_none() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    job.snapshot()
}

#[cfg(unix)]
fn links(p: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).unwrap().nlink()
}

/// A bus installed from the inbox and then deleted from Mods/installed leaves the
/// lists: its folder goes to Mods/uninstalled (#819).
#[test]
fn a_bus_deleted_from_installed_is_uninstalled() {
    let dir = tmp("uninstall");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    // a bus that was there before, which the mod only adds to, stays
    std::fs::create_dir_all(content.join("Vehicles/Old")).unwrap();
    let src = inbox_bus(&content, 3);
    let p = wait_done(&start_inner(
        content.clone(),
        None,
        src,
        InstallMode::Extract,
        true,
        0,
    ));
    assert_eq!(p.state, "done", "{p:?}");
    assert!(content.join("Vehicles/Big Bus/big.bus").exists());
    assert!(content.join("Mods/installed/Big Bus").exists());
    // still in Mods/installed: nothing happens
    assert!(uninstall_removed_now(&content).is_empty());
    assert!(content.join("Vehicles/Big Bus/big.bus").exists());
    std::fs::remove_dir_all(content.join("Mods/installed/Big Bus")).unwrap();
    assert_eq!(uninstall_removed_now(&content), ["Big Bus"]);
    assert!(!content.join("Vehicles/Big Bus").exists());
    assert!(
        content
            .join("Mods")
            .join(UNINSTALLED)
            .join("Big Bus/Vehicles/Big Bus/big.bus")
            .exists()
    );
    assert!(content.join("Vehicles/Old").exists());
    // once only
    assert!(uninstall_removed_now(&content).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cancelled_inbox_folder_keeps_its_files() {
    let dir = tmp("inbox");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let src = inbox_bus(&content, 3000);
    let job = start_inner(
        content.clone(),
        None,
        src.clone(),
        InstallMode::Extract,
        true,
        1500,
    );
    wait_for(&job, 1500);
    assert_eq!(
        std::fs::read_dir(src.join("Texture")).unwrap().count(),
        3000,
        "linked, not moved: the inbox is whole while the job runs"
    );
    #[cfg(unix)]
    assert_eq!(
        links(&src.join("big.bus")),
        2,
        "big.bus is linked into the staging folder"
    );
    job.cancel();
    let p = wait_done(&job);
    assert_eq!(p.state, "cancelled", "{p:?}");
    assert_eq!(
        std::fs::read_dir(src.join("Texture")).unwrap().count(),
        3000,
        "every file is there"
    );
    #[cfg(unix)]
    assert_eq!(links(&src.join("big.bus")), 1, "the staged link is gone");
    assert!(src.join("big.bus").exists());
    assert!(!content.join("Vehicles/Big Bus").exists());
    assert!(!content.join(STAGING).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_killed_inbox_install_loses_nothing() {
    let dir = tmp("killed");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let src = inbox_bus(&content, 400);
    let job = start_inner(
        content.clone(),
        None,
        src.clone(),
        InstallMode::Extract,
        true,
        200,
    );
    wait_for(&job, 200);
    // the launcher "dies" half-way: its staging folder now belongs to a process that is
    // gone, and the next launcher's start-up removes it
    let staging = staging_dir(&content, job.id);
    let dead = content.join(STAGING).join(format!("999999-{}", job.id));
    std::fs::rename(&staging, &dead).unwrap();
    let removed = cleanup_stale(&dir.join("data"), Some(&content));
    assert!(removed.iter().any(|l| l.contains("999999")), "{removed:?}");
    assert!(!dead.exists());
    assert_eq!(
        std::fs::read_dir(src.join("Texture")).unwrap().count(),
        400,
        "every inbox file is still there"
    );
    assert_eq!(
        std::fs::read(src.join("big.bus")).unwrap(),
        b"[friendlyname]"
    );
    job.cancel();
    wait_done(&job);
    assert_eq!(std::fs::read_dir(src.join("Texture")).unwrap().count(), 400);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_inbox_folder_is_installed_and_leaves_the_inbox() {
    let dir = tmp("inbox-done");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let src = inbox_bus(&content, 20);
    let job = start(
        content.clone(),
        None,
        src.clone(),
        InstallMode::Extract,
        true,
    );
    let p = wait_done(&job);
    assert_eq!(p.state, "done", "{p:?}");
    assert_eq!(p.installed, vec!["Vehicles/Big Bus".to_string()]);
    assert_eq!(
        std::fs::read_dir(content.join("Vehicles/Big Bus/Texture"))
            .unwrap()
            .count(),
        20
    );
    #[cfg(unix)]
    assert_eq!(
        links(&content.join("Vehicles/Big Bus/big.bus")),
        1,
        "the inbox copy is gone"
    );
    assert!(!src.exists(), "the inbox folder was cleared");
    // the installed files left the inbox; only the empty folders went to Mods/installed
    assert!(content.join("Mods/installed/Big Bus").is_dir());
    assert!(
        !content
            .join("Mods/installed/Big Bus/Texture/t0.dds")
            .exists()
    );
    assert!(!content.join(STAGING).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn safe_names() {
    assert_eq!(
        safe_rel("Vehicles/Foo/foo.bus").as_deref(),
        Some("Vehicles/Foo/foo.bus")
    );
    assert_eq!(
        safe_rel("Vehicles\\Foo\\./foo.bus").as_deref(),
        Some("Vehicles/Foo/foo.bus")
    );
    assert_eq!(safe_rel("Foo//bar/").as_deref(), Some("Foo/bar"));
    for bad in [
        "..\\..\\..\\evil",
        "a/..\\..\\b",
        "../x",
        "a/../b",
        "/etc/passwd",
        "\\\\server\\share\\x",
        "C:\\x",
        "C:x",
        "Foo/C:evil/x.bus",
        "a\0b",
        "",
        "./.",
    ] {
        assert_eq!(safe_rel(bad), None, "{bad:?}");
    }
    for bad in [
        "Plugins/NUL",
        "a/con.txt",
        "a/Com1.dll",
        "a/LPT9",
        "a/aux .x",
        "Vehicles/Foo./x.bus",
        "Vehicles/Foo /x.bus",
        "a/x.bus.",
        "a/...",
        "a/CONOUT$",
    ] {
        assert_eq!(safe_rel(bad), None, "{bad:?}");
    }
    for good in [
        "Vehicles/Console/x.bus",
        "a/COM10.txt",
        "a/null.cfg",
        "a/.hidden",
        "Sceneryobjects/Laterne/model/LPT.o3d",
    ] {
        assert_eq!(safe_rel(good).as_deref(), Some(good), "{good:?}");
    }
    assert!(check_rel("Vehicles/Foo").is_ok());
    assert!(check_rel("Vehicles/../Foo").is_err());
    assert!(check_rel("maps/..").is_err());
    assert_eq!(safe_rel("a/COM0"), None);
    assert_eq!(safe_rel("a/LPT0.txt"), None);
    for kept in ["Vehicles/Old Bus.", "Sceneryobjects/Aux", "Vehicles/Foo /x"] {
        assert!(check_installed_rel(kept).is_ok(), "{kept:?}");
    }
    for bad in ["../x", "a/../b", "/etc", "\\\\server\\x", "C:x", "a//b", ""] {
        assert!(check_installed_rel(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn backslash_traversal_in_a_zip_is_refused() {
    let dir = tmp("zipslip");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    // files are unpacked into dir/content/.install-staging/<pid>-<job>/0: four levels up
    // is `dir`, so this name would land in `dir/evil.txt`
    let zip = dir.join("Evil Map.zip");
    write_zip(
        &zip,
        &[
            ("global.cfg", 10),
            ("..\\..\\..\\..\\evil.txt", 10),
            ("tile_0_0.map", 10),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip,
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "failed", "{p:?}");
    assert!(p.message.contains("refused"), "{}", p.message);
    assert!(
        !dir.join("evil.txt").exists(),
        "nothing written outside the staging folder"
    );
    assert!(!content.join("maps/Evil Map").exists());
    assert!(!content.join(STAGING).exists());
    // the same inside a bus folder
    let zip = dir.join("Evil Bus.zip");
    write_zip(
        &zip,
        &[
            ("Vehicles/Foo/foo.bus", 10),
            ("Vehicles/Foo/..\\..\\..\\..\\evil.txt", 10),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip,
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "failed", "{p:?}");
    assert!(!dir.join("evil.txt").exists());
    // a zip made on Windows with `\` separators still installs
    let zip = dir.join("Win Bus.zip");
    write_zip(
        &zip,
        &[
            ("Vehicles\\Bar\\bar.bus", 10),
            ("Vehicles\\Bar\\Texture\\a.dds", 100),
        ],
    );
    let p = run_blocking(
        content.clone(),
        None,
        zip,
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert_eq!(
        std::fs::metadata(content.join("Vehicles/Bar/Texture/a.dds"))
            .unwrap()
            .len(),
        100
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn absurdly_deep_or_long_names_are_refused() {
    let deep = format!("Sceneryobjects/{}x.sco", "a/".repeat(32_000));
    assert_eq!(safe_rel(&deep), None);
    let at_limit = format!("{}x.sco", "a/".repeat(63));
    assert_eq!(safe_rel(&at_limit).as_deref(), Some(at_limit.as_str()));
    assert_eq!(safe_rel(&format!("{}x.sco", "a/".repeat(64))), None);
    assert_eq!(
        safe_rel(&format!("Vehicles/{}.bus", "b".repeat(1100))),
        None
    );
    let mut root = Node::default();
    root.insert(&at_limit, 7);
    root.insert("Vehicles/Foo/foo.bus", 3);
    root.insert("Vehicles/Bar/bar.bus", 5);
    let mut all = Vec::new();
    root.all(&mut all);
    assert_eq!(all, vec![5, 3, 7]);
    assert!(root.has_ext_deep(&["sco"]));
    assert!(!root.has_ext_deep(&["ovh"]));
}

#[test]
fn seven_zip_names_stay_inside_the_staging_folder() {
    let dest = tmp("7zslip").join("staging");
    assert_eq!(
        staged_target(&dest, "Vehicles/Foo/foo.bus"),
        Some(dest.join("Vehicles/Foo/foo.bus"))
    );
    assert_eq!(
        staged_target(&dest, "Vehicles\\Foo\\foo.bus"),
        Some(dest.join("Vehicles/Foo/foo.bus"))
    );
    for bad in [
        "a/C:Plugins/evil.dll",
        "a/C:openomsi.exe",
        "C:\\Plugins\\evil.dll",
        "\\\\host\\share\\x",
        "../x",
        "/etc/x",
        "Plugins/NUL",
    ] {
        assert_eq!(staged_target(&dest, bad), None, "{bad:?}");
    }
}

#[cfg(unix)]
#[test]
fn backslash_names_in_a_folder_stay_inside() {
    let dir = tmp("folderslip");
    let content = dir.join("content");
    omsi_cfg::ensure_content_layout(&content).unwrap();
    let src = dir.join("Odd Bus");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("odd.bus"), b"[friendlyname]").unwrap();
    // one file whose name is a whole Windows path, one that tries to climb out
    std::fs::write(src.join("Texture\\a.dds"), [1u8; 32]).unwrap();
    std::fs::write(src.join("..\\..\\..\\..\\evil.txt"), b"x").unwrap();
    let p = run_blocking(
        content.clone(),
        None,
        src.clone(),
        InstallMode::Extract,
        None,
        false,
    );
    assert_eq!(p.state, "done", "{p:?}");
    assert!(
        p.warnings.iter().any(|w| w.contains("1 file(s)")),
        "{:?}",
        p.warnings
    );
    assert_eq!(
        std::fs::metadata(content.join("Vehicles/Odd Bus/Texture/a.dds"))
            .unwrap()
            .len(),
        32
    );
    assert!(!dir.join("evil.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_staging_is_removed() {
    let dir = tmp("stale");
    let content = dir.join("content");
    let data = dir.join("data");
    std::fs::create_dir_all(content.join(STAGING).join("999999-3/0")).unwrap();
    std::fs::write(content.join(STAGING).join("999999-3/0/x"), b"partial").unwrap();
    std::fs::create_dir_all(data.join("unzip/Old")).unwrap();
    let out = cleanup_stale(&data, Some(&content));
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(!content.join(STAGING).exists());
    assert!(!data.join("unzip").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn free_space_is_known() {
    assert!(free_space(&std::env::temp_dir()).unwrap_or(0) > 0);
}
