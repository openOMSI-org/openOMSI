use super::*;

#[test]
fn a_mods_lowercase_texture_folder_does_not_hide_the_originals() {
    // A mod ships `Vehicles/Pack/texture/..` (a livery) next to the original pack's
    // `Vehicles/Pack/Texture/..`: a file asked for from the mod's folder is found in the
    // original's, whatever the case of the folder, as on a case-insensitive file system
    let t = std::env::temp_dir().join(format!("omsi-cfg-mirror-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&t);
    let (mods, omsi) = (t.join("mods"), t.join("omsi"));
    std::fs::create_dir_all(mods.join("Vehicles/Pack/texture")).unwrap();
    std::fs::create_dir_all(omsi.join("Vehicles/Pack/Texture")).unwrap();
    std::fs::write(omsi.join("Vehicles/Pack/Texture/black.dds"), b"x").unwrap();
    let roots = vec![mods.clone(), omsi.clone()];
    let mirrored = mirrored_base(&mods.join("Vehicles/Pack/texture"), &omsi, &roots).unwrap();
    assert_eq!(mirrored, omsi.join("Vehicles/Pack/Texture"));
    assert!(resolve_existing(&mirrored, &["black.dds"]).is_some());
    // a folder the other root does not have keeps the plain spelling
    let none = mirrored_base(&mods.join("Vehicles/Other/texture"), &omsi, &roots).unwrap();
    assert_eq!(none, omsi.join("Vehicles/Other/texture"));
    std::fs::remove_dir_all(&t).unwrap();
}

#[test]
fn a_folder_spelled_twice_hides_neither_ones_content() {
    // An add-on unpacked beside the original folders leaves `Maps` with the new map next
    // to the `maps` holding all the others. Both spellings lower-case to one key, and the
    // listing used to keep whichever the file system happened to read last - the stock
    // maps then went missing (`missing: maps/Grundorf/global.cfg`).
    let t = std::env::temp_dir().join(format!("omsi-cfg-case-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&t);
    std::fs::create_dir_all(t.join("maps/Grundorf")).unwrap();
    if t.join("Maps").exists() {
        // a case-insensitive file system cannot hold both spellings, so there is nothing
        // to tell apart here
        std::fs::remove_dir_all(&t).unwrap();
        return;
    }
    std::fs::create_dir_all(t.join("Maps/Keedlestone 1.8")).unwrap();
    std::fs::write(t.join("maps/Grundorf/global.cfg"), b"x").unwrap();
    std::fs::write(t.join("Maps/Keedlestone 1.8/global.cfg"), b"x").unwrap();
    content_changed();

    // each map is found under the spelling the file asks for, whichever folder holds it
    assert_eq!(
        resolve_existing(&t, &["maps", "Grundorf", "global.cfg"]),
        Some(t.join("maps/Grundorf/global.cfg")),
    );
    assert_eq!(
        resolve_existing(&t, &["maps", "Keedlestone 1.8", "global.cfg"]),
        Some(t.join("Maps/Keedlestone 1.8/global.cfg")),
    );
    // and the capitalised spelling reaches the lower-case folder just as well
    assert_eq!(
        resolve_existing(&t, &["Maps", "Grundorf", "global.cfg"]),
        Some(t.join("maps/Grundorf/global.cfg")),
    );
    // a name neither folder has is still absent
    assert_eq!(
        resolve_existing(&t, &["maps", "Nowhere", "global.cfg"]),
        None
    );

    std::fs::remove_dir_all(&t).unwrap();
    content_changed();
}

#[test]
fn writable_check() {
    let tmp = std::env::temp_dir();
    assert!(is_writable(&tmp));
    let nonexistent = tmp.join("nonexistent_subfolder_xyz_123");
    assert!(!is_writable(&nonexistent));
}

#[test]
fn applications_folder_is_no_content_folder() {
    assert!(is_programs_folder(Path::new("/Users/x/Applications")));
    assert!(!is_programs_folder(Path::new("/Users/x/Games/openOMSI")));
    // content an older version installed there stays in use
    let dir = std::env::temp_dir()
        .join(format!("omsi-apps-{}", std::process::id()))
        .join("Applications");
    std::fs::create_dir_all(dir.join("Mods")).unwrap();
    std::fs::create_dir_all(dir.join("Vehicles")).unwrap();
    std::fs::create_dir_all(dir.join("maps").join("Grundorf")).unwrap();
    std::fs::write(dir.join("Mods").join("README.txt"), b"x").unwrap();
    std::fs::write(dir.join("maps").join("Grundorf").join("laststn.osn"), b"x").unwrap();
    assert!(is_programs_folder(&dir));
    std::fs::create_dir_all(dir.join("Vehicles").join("MAN_SD200")).unwrap();
    assert!(!is_programs_folder(&dir));
    std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn own_inputs_outlive_the_program_folder() {
    let t = std::env::temp_dir().join(format!("omsi-own-inputs-{}", std::process::id()));
    let own = t.join("home").join("Inputs").join("keyboard.cfg");
    let (first, second) = (
        t.join("openOMSI-0.2.38").join("keyboard.cfg"),
        t.join("openOMSI-0.2.39").join("keyboard.cfg"),
    );
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    std::fs::create_dir_all(second.parent().unwrap()).unwrap();
    let at = |p: &Path, secs: u64| {
        std::fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap()
    };
    assert_eq!(newest([Some(own.clone()), Some(first.clone())]), None);
    std::fs::write(&first, b"[game]\nsim_pause\n25\n0\n").unwrap();
    std::fs::write(&second, b"[game]\n").unwrap();
    at(&first, 2_000_000_000);
    at(&second, 1_000_000_000);
    let found = newest([None, Some(second.clone()), Some(first.clone())]).unwrap();
    assert_eq!(found, first);
    adopt_input(&own, &found);
    assert_eq!(std::fs::read(&own).unwrap(), b"[game]\nsim_pause\n25\n0\n");
    std::fs::write(&own, b"[game]\nsim_pause\n26\n0\n").unwrap();
    adopt_input(&own, &second);
    assert_eq!(std::fs::read(&own).unwrap(), b"[game]\nsim_pause\n26\n0\n");
    at(&own, 2_000_000_000);
    assert_eq!(
        newest([Some(own.clone()), Some(first.clone())]),
        Some(own.clone())
    );
    at(&first, 2_000_000_001);
    assert_eq!(
        newest([Some(own.clone()), Some(first.clone())]),
        Some(first.clone())
    );
    assert_eq!(std::fs::read_dir(own.parent().unwrap()).unwrap().count(), 1);
    std::fs::remove_dir_all(&t).unwrap();
    std::fs::create_dir_all(t.join("openOMSI-0.2.38")).unwrap();
    std::fs::write(t.join("openOMSI-0.2.38").join("openomsi.exe"), b"x").unwrap();
    std::fs::create_dir_all(t.join("photos")).unwrap();
    assert_eq!(
        sibling_installs(&t.join("openOMSI-0.2.40")),
        vec![t.join("openOMSI-0.2.38")]
    );
    std::fs::remove_dir_all(&t).unwrap();
    assert_eq!(
        content_folder_of_program(Path::new("/x/Games/openOMSI.app/Contents/MacOS/openomsi")),
        Some(PathBuf::from("/x/Games"))
    );
    assert_eq!(
        content_folder_of_program(Path::new("/x/openOMSI-0.2.38/openomsi.exe")),
        Some(PathBuf::from("/x/openOMSI-0.2.38"))
    );
}

#[test]
fn keywords() {
    assert_eq!(keyword_of("[mesh]"), Some("mesh"));
    assert_eq!(keyword_of("\t[mesh]"), None);
    assert_eq!(keyword_of("        [NightMapMode]"), None);
    assert_eq!(keyword_of("[mesh] "), None);
    assert_eq!(keyword_of("[busstop] macht aus dem Objekt"), None);
    assert_eq!(keyword_of("[coupling_front] / [coupling_back]"), None);
    // the original's spelling counts for its own keywords …
    assert_eq!(keyword_of("[matl_noZwrite]"), Some("matl_noZwrite"));
    assert_eq!(keyword_of("[matl_nozWrite]"), None);
    assert_eq!(keyword_of("[NoDistanceCheck]"), None);
    assert_eq!(keyword_of("[Mesh]"), None);
    assert_eq!(keyword_of("[LOD]"), Some("LOD"));
    assert_eq!(keyword_of("[lod]"), None);
    // … not for the rest
    assert_eq!(
        keyword_of("[Infosystem_Busstop]"),
        Some("Infosystem_Busstop")
    );
    // .hof: spreadsheet exports
    assert_eq!(
        keyword_with("[global_strings]\t\t\t", KeywordRule::TrimEnd),
        Some("global_strings")
    );
    assert_eq!(keyword_with("[end]\"\t", KeywordRule::TrimEnd), Some("end"));
    assert_eq!(keyword_with("\t[end]", KeywordRule::TrimEnd), None);
    assert_eq!(keyword_of("[global_strings]\t"), None);
    // ailists.cfg
    assert_eq!(
        keyword_with("[AIGroup_2]", KeywordRule::AnyCase),
        Some("AIGroup_2")
    );
    assert_eq!(keyword_with(" [aigroup_2]", KeywordRule::AnyCase), None);
    for k in keywords::MIXED_CASE.iter().chain(keywords::LOWER_CASE) {
        assert_eq!(omsi_spelling(&k.to_ascii_uppercase()), Some(*k));
    }
}

/// The stock files' indented keywords are the original's help texts and switched-off
/// blocks: the SD202 cabin's `[exit]` help line adds no exit.
#[test]
fn indented_help_text() {
    let text = "\t[entry]\t\tdefiniert Eingang\r\n\t[exit]\r\n\tnum\t\tanalog f\u{fc}r Ausgang\r\n\r\n###\r\n[exit]\r\n20\r\n";
    let f = CfgFile::from_str("passengercabin.cfg", text);
    let mut r = f.reader();
    assert_eq!(r.next_keyword().as_deref(), Some("exit"));
    assert_eq!(r.i32(), 20);
    assert_eq!(r.next_keyword(), None);
}

#[test]
fn disabled_blocks() {
    let text = "[mesh]\na.o3d\n-<DISABLED>-\n[mesh]\nb.o3d\n-<ENABLED>-\n[mesh]\n-<DISABLED>-\n[mesh]\nd.o3d\n";
    let f = CfgFile::from_str("model.cfg", text);
    let mut r = f.reader().disabled_blocks();
    let mut meshes = Vec::new();
    while let Some(k) = r.next_keyword() {
        assert_eq!(k, "mesh");
        meshes.push(r.str().to_string());
    }
    // a marker read as a parameter is a parameter; an unclosed block runs to the end
    assert_eq!(meshes, vec!["a.o3d", "-<DISABLED>-", "d.o3d"]);
    // loaders without the feature see the blocks
    let mut r = f.reader();
    let mut n = 0;
    while r.next_keyword().is_some() {
        n += 1;
    }
    assert_eq!(n, 4);
    let f = CfgFile::from_str(
        "model.cfg",
        "-<DISABLED>-\n[mesh]\nx\n -<ENABLED>-\n[mesh]\ny\n",
    );
    assert_eq!(f.reader().disabled_blocks().next_keyword(), None);
}

#[test]
fn bare_tokens() {
    let text = "[new_attachment]\n\ntext\n[complexity]\n2\n\nattach_trans\n1\n2\n3\n\tattach_rot_x\nAttach_rot_y\n";
    let f = CfgFile::from_str("x.sco", text);
    let mut r = f.reader();
    const T: &[&str] = &["attach_trans", "attach_rot_x", "attach_rot_y"];
    assert_eq!(
        r.next_entry(T),
        Some(Entry::Keyword("new_attachment".into()))
    );
    assert_eq!(r.next_entry(T), Some(Entry::Keyword("complexity".into())));
    assert_eq!(r.i32(), 2);
    assert_eq!(r.next_entry(T), Some(Entry::Token("attach_trans")));
    assert_eq!(r.f32s::<3>(), [1.0, 2.0, 3.0]);
    assert_eq!(r.next_entry(T), None);
}

#[test]
fn until_follows_the_rule() {
    let f = CfgFile::from_str("x.owt", "[description]\na\n[END]\n[end] \nb\n[end]\nc\n");
    let mut r = f.reader();
    r.next_keyword();
    assert_eq!(r.until("[end]"), vec!["a", "[END]", "[end] ", "b"]);
    let f = CfgFile::from_str("x.hof", "[addbusstop_list]\na\t\n[end]\t\t\nc\n");
    let mut r = f.reader().with_rule(KeywordRule::TrimEnd);
    r.next_keyword();
    assert_eq!(r.until("[end]"), vec!["a\t"]);
    assert_eq!(r.str(), "c");
}

#[test]
fn windows_paths() {
    assert_eq!(
        windows_components("texture\\anz-oben.jpg."),
        vec!["texture", "anz-oben.jpg"]
    );
    assert_eq!(
        windows_components(" ..\\..\\Model.\\a.o3d . "),
        vec!["..", "..", "Model", "a.o3d"]
    );
    assert_eq!(
        windows_components("\\Splines//x\\.\\y.sli"),
        vec!["Splines", "x", "y.sli"]
    );
    assert_eq!(windows_components("Folder..\\x"), vec!["Folder.", "x"]);
    assert_eq!(windows_components("Folder.\\"), vec!["Folder"]);
    assert_eq!(
        windows_components("str_gehweg02.bmp "),
        vec!["str_gehweg02.bmp"]
    );
    assert_eq!(windows_components(""), Vec::<&str>::new());
    assert_eq!(
        windows_components("a\\.. \\...\\. .\\b.bmp"),
        vec!["a", "b.bmp"]
    );
    let dir = std::env::temp_dir().join(format!("omsi-cfg-paths-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Texture").join("Bahn\u{fc}bergang")).unwrap();
    std::fs::write(
        dir.join("Texture")
            .join("Bahn\u{fc}bergang")
            .join("anz-oben.jpg"),
        b"x",
    )
    .unwrap();
    let p = resolve_path(
        &dir.join("model"),
        "..\\TEXTURE.\\BAHN\u{dc}BERGANG/Anz-Oben.JPG. ",
    );
    assert!(vfs::is_file(&p), "{}", p.display());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reader() {
    let f = CfgFile::from_str("x.cfg", "hello\r\n[mesh]\r\na.o3d\r\n\r\n[LOD]\r\n0.5\r\n");
    let mut r = f.reader();
    assert_eq!(r.next_keyword().as_deref(), Some("mesh"));
    assert_eq!(r.str(), "a.o3d");
    assert_eq!(r.next_keyword().as_deref(), Some("lod"));
    assert_eq!(r.f32(), 0.5);
    assert_eq!(r.next_keyword(), None);
}

#[test]
fn utf16() {
    let mut b = vec![0xFF, 0xFE];
    for c in "[name]\r\nGrundorf".encode_utf16() {
        b.extend_from_slice(&c.to_le_bytes());
    }
    let f = CfgFile::from_bytes("g.cfg", &b);
    assert_eq!(f.lines, vec!["[name]", "Grundorf"]);
}
