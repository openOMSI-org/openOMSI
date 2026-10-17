use super::*;

#[test]
fn a_fetched_bus_reads_only_its_own_files() {
    let Some(root) = std::env::var_os("OMSI_ROOT").map(PathBuf::from) else { return };
    let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    omsi_cfg::add_content_root(root.clone());
    let store = std::env::temp_dir().join(format!("openomsi-mounted-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&store);
    open_for_test(&store);
    let bus = "Vehicles/MAN_SD200/MAN_SD77.bus";
    let ty = omsi_sim::VehicleType::load(&root, &omsi_cfg::resolve_path(&root, bus)).unwrap();
    let files = super::super::upload::listed(&root, super::super::bus_files::of_type(&root, &ty, Some(0), None));
    let m = VehicleManifest { bus: bus.into(), entries: super::super::upload::hashed(&files) }.validated().unwrap();
    for (rel, p) in &files {
        let e = m.entries.iter().find(|e| e.path == *rel).unwrap();
        put(e, &omsi_cfg::vfs::read(p).unwrap()).unwrap();
    }
    let (fp, _) = commit(m).unwrap();
    viewer().set(&fp, Wanted::Mounted(PathBuf::new()));
    let at = mount(&fp).unwrap();
    let audit = omsi_cfg::vfs::Audit::begin();
    let mt = omsi_sim::VehicleType::load(&root, &bus_file(&fp).unwrap()).unwrap();
    let reads = audit.end();
    assert!(reads.len() > 100);
    assert_eq!(escapes(&at, &root, &reads), Vec::<PathBuf>::new(), "read from outside its own files");
    assert_eq!(mt.meshes.len(), ty.meshes.len());
    // its textures and sounds, looked up as the renderer looks them up: all its own
    let outside: Vec<PathBuf> = super::super::bus_files::of_type(&root, &mt, Some(0), None)
        .into_iter()
        .filter(|p| !p.starts_with(&at) && !p.components().any(|c| c.as_os_str().eq_ignore_ascii_case("Fonts")))
        .collect();
    assert!(outside.is_empty(), "{outside:?}");
    // and its displays write with its own fonts
    let mut fonts = omsi_sim::texttex::FontLibrary::only(&at);
    for t in &mt.model.text_textures {
        assert!(fonts.has_exact(&t.font), "font {} not among its own", t.font);
    }
    let _ = std::fs::remove_dir_all(&store);
}
