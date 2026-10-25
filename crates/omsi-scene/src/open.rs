//! Opening a map: global.cfg, object and spline types, the navigation map.
use super::*;

/// What is being read right now, by key: the tiles are read side by side, and two of them
/// that name the same type both read it when it was not there yet (the second copy then
/// thrown away). With this the second waits for the first's. Only for reads that run to
/// their end on their own thread - no worker pool inside, no other key asked for, no lock
/// the waiting threads could hold - so that the wait cannot close a circle.
pub struct OnceByKey<T>(Mutex<HashMap<String, Arc<std::sync::OnceLock<T>>>>);

impl<T> Default for OnceByKey<T> {
    fn default() -> Self {
        OnceByKey(Mutex::new(HashMap::new()))
    }
}

impl<T: Clone> OnceByKey<T> {
    /// `read`'s result for `key`, read once by whoever asks first while the others wait.
    /// `read` should leave its result where later callers look first: the key is
    /// forgotten here as soon as it is read.
    pub fn once(&self, key: &str, read: impl FnOnce() -> T) -> T {
        let cell = self.0.lock().entry(key.to_string()).or_default().clone();
        let value = cell.get_or_init(read).clone();
        let mut loads = self.0.lock();
        if loads.get(key).is_some_and(|c| Arc::ptr_eq(c, &cell)) {
            loads.remove(key);
        }
        value
    }
}

impl World {
    pub fn open(root: &Path, global_cfg: &Path, date: i32) -> Result<World> {
        let mut global = GlobalCfg::load(global_cfg)
            .with_context(|| format!("loading {}", global_cfg.display()))?;
        let map_dir = global.dir().to_path_buf();
        omsi_map::configure_grid(&global);
        // a map from OMSI 1 lists no entry points: the entry objects of its tiles are its
        // entry points, as Omsi.exe adds them when it opens a map (only then: the launcher's
        // list shows the same ones, and a map that lists its own opens no slower)
        if global.entry_points.is_empty() {
            let added = global.add_object_entry_points();
            log::info!("entry points: {added} from the tiles' entry objects");
        }
        omsi_sim::people::LEFT_HAND.store(global.left_hand_traffic, std::sync::atomic::Ordering::Relaxed);
        log::info!(
            "tile size {:.1} m ({})",
            omsi_map::tile_size(),
            if global.world_coordinates {
                "[worldcoordinates]"
            } else {
                "plain map"
            }
        );
        // where the sun is: the map's time zone, place and summer time
        let tz_path = omsi_cfg::resolve_path(&map_dir, "timezone.txt");
        let mut place = omsi_sim::daylight::SunPlace::default();
        if let Ok(tz) = omsi_map::TimeZone::load(&tz_path) {
            place.timezone = tz.offset_hours as f64;
            if let Some((lat, lon)) = tz.lat_lon() {
                place.latitude = lat;
                place.longitude = lon;
            }
            place.dst = tz.dst.iter().map(|d| (d.start, d.end, d.params[0], d.params[1], d.params[2])).collect();
            log::info!("sun: {:.3} N {:.3} E, UTC{:+}, {} summer time periods", place.latitude, place.longitude, place.timezone, place.dst.len());
        }
        omsi_sim::daylight::set_place(place);
        let mut signal_routes = omsi_cfg::CfgFile::read(&omsi_cfg::resolve_path(&map_dir, "signalroutes.cfg"))
            .map(|f| omsi_map::ailists::parse_signalroutes(&f))
            .unwrap_or_default();
        if !signal_routes.is_empty() {
            log::info!("signal routes: {} for {} signals", signal_routes.len(), signal_routes.iter().map(|r| r.signal.0).collect::<hashbrown::HashSet<_>>().len());
        }
        let chrono_dirs = omsi_map::active_chrono_dirs(&map_dir, date);
        // AI lists: the map's plus the chrono updates; depot entries filtered by validity date
        let mut ailists = omsi_map::ailists::ailists_with_chrono(&map_dir, &chrono_dirs);
        let mut ticket_pack = global.ticket_pack.clone();
        for c in &chrono_dirs {
            if let Some(cfg) = Some(omsi_cfg::resolve_path(c, "Chrono.cfg"))
                .filter(|p| omsi_cfg::vfs::is_file(p))
                .and_then(|p| omsi_cfg::CfgFile::read(&p).ok())
            {
                let cc = omsi_map::ailists::parse_chrono_cfg(&cfg);
                // the newest active scenario that names a ticket pack or money system has
                // it, else the map's own: the currency changes from DM to Euro on the
                // scenario's date (Omsi.exe 0x785f98 after "TMap.loadGlobalFile - 154":
                // active chrono entries from the last back, +0x30 money system then
                // +0x2c ticket pack, the map's own last, #2315)
                if let Some(t) = cc.ticket_pack.filter(|t| !t.trim().is_empty()) {
                    ticket_pack = t;
                }
                if let Some(m) = cc.money_system.filter(|m| !m.trim().is_empty()) {
                    global.money_system = m.trim().to_string();
                }
            }
        }
        for g in ailists.groups.iter_mut() {
            for tg in g.typgroups.iter_mut() {
                tg.entries
                    .retain(|e| omsi_map::typgroup_entry_valid(e, date));
            }
        }
        if !chrono_dirs.is_empty() {
            log::info!(
                "chrono: {} folders active on {date}: {:?}",
                chrono_dirs.len(),
                chrono_dirs
                    .iter()
                    .map(|d| d.file_name().unwrap().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            );
        }
        let data = Arc::new(omsi_sim::world_data::WorldCell::new(omsi_sim::world_data::WorldData {
            map: Arc::new(omsi_sim::world_data::MapFacts {
                root: root.to_path_buf(),
                map_dir: map_dir.clone(),
                raw_tiles: global.raw_tiles.clone(),
                tiles: global.tiles.iter().map(|t| ((t.x, t.y), t.file.clone())).collect(),
                ailists,
                date,
                chrono_dirs,
            }),
            ..Default::default()
        }));
        let transit = {
            let mut issues = Vec::new();
            let t = omsi_transit::TransitData::load(&map_dir, &mut issues);
            t.check(global.transport, &mut issues);
            if !t.is_empty() {
                log::info!("transport folder: {} ({} stations, {} routes, {} depots)", t.files.join(", "), t.stations.list.len(), t.routes.list.len(), t.depots.list.len());
            }
            for i in &issues {
                if i.error {
                    log::warn!("transport: {i}");
                } else {
                    log::info!("transport: {i}");
                }
            }
            Arc::new(t)
        };
        // the signals of the transport folder's blocks are signals of the map: the same objects, the
        // same `Signal` variable, shown by `signal_aspects`
        for sig in &transit.signals.signals {
            if let (Some(object), Some(_)) = (sig.object, sig.block.as_ref()) {
                if !signal_routes.iter().any(|r| r.signal.0 == object) {
                    signal_routes.push(omsi_map::ailists::SignalRoute { signal: (object, 0), ..Default::default() });
                }
            }
        }
        Ok(World {
            root: root.to_path_buf(),
            transit,
            global,
            map_dir,
            parklist: Mutex::new(HashMap::new()),
            settings: Default::default(),
            mirror_textures: Mutex::new(Vec::new()),
            mirror_aspect: Mutex::new(Vec::new()),
            mirror_glass: Mutex::new(Vec::new()),
            ticket_pack,
            object_types: Mutex::new(HashMap::new()),
            spline_types: Mutex::new(HashMap::new()),
            object_type_loads: Default::default(),
            spline_type_loads: Default::default(),
            textures: Arc::new(TextureCache::new()),
            data,
            vehicle_gpu: Mutex::new(HashMap::new()),
            vehicle_textures: Default::default(),
            freetex_reloads: Default::default(),
            vehicle_meshes: Default::default(),
            vehicle_ready: Default::default(),
            upgrades_pending: Default::default(),
            decoded_in_flight: Default::default(),
            upgrades_done: Default::default(),
            freetex_upgrades: Default::default(),
            texture_limit: Default::default(),
            video_memory_limit: Default::default(),
            budget_checked: Default::default(),
            traffic_inbox: Default::default(),
            controller_of_object: Mutex::new(HashMap::new()),
            light_objects: Mutex::new(Vec::new()),
            scripted: Mutex::new(ScriptedSet::default()),
            poles: Mutex::new(HashMap::new()),
            fallen_poles: Mutex::new(HashMap::new()),
            parked_objects: Mutex::new(HashMap::new()),
            help_arrows: Mutex::new(HashMap::new()),
            help_arrows_shown: std::sync::atomic::AtomicBool::new(false),
            over_road: std::sync::atomic::AtomicUsize::new(0),
            over_road_at: std::sync::Mutex::new(Vec::new()),
            departed: Mutex::new(std::collections::HashSet::new()),
            departed_objects: Mutex::new(std::collections::HashMap::new()),
            edit_objects: Mutex::new(HashMap::new()),
            object_edits: Mutex::new(HashMap::new()),
            terrain_edits: Mutex::new(HashMap::new()),
            waiting_cabins: Mutex::new(HashMap::new()),
            tiles_generation: std::sync::atomic::AtomicU64::new(0),
            light_maps: Mutex::new(HashMap::new()),
            light_maps_generation: std::sync::atomic::AtomicU64::new(0),
            light_map_atlas: Mutex::new(None),
            parked_live: std::sync::atomic::AtomicUsize::new(0),
            parked_max: super::host::open_settings().max_parked,
            signal_ids: signal_routes.iter().flat_map(|r| std::iter::once(r.signal.0).chain(r.dist_signal)).collect(),
            signal_routes,
            particle_objects: Mutex::new(HashMap::new()),
            fonts: {
                // (the fonts' sheets and the vehicles' bitmaps are read through it; a world
                // opened by a test or a tool has not been through `run`)
                super::host::install_sim_decoder();
                Arc::new(Mutex::new(omsi_sim::texttex::FontLibrary::new(root)))
            },
            night_slots: Mutex::new(Vec::new()),
            night_modes: Mutex::new(Vec::new()),
            static_coronas: Mutex::new(Vec::new()),
            static_lights: Mutex::new(Vec::new()),
            near_lights: Mutex::new(Default::default()),
            cookie_fades: Mutex::new(Default::default()),
            season_extra: Mutex::new(Default::default()),
            spline_ends: Mutex::new(Default::default()),
            index: Mutex::new(None),
            tile_state: Mutex::new(HashMap::new()),
            seeded: Mutex::new(Default::default()),
            gpu: Mutex::new(GpuCache::default()),
            missing: Mutex::new(Default::default()),
            staged: Mutex::new(HashMap::new()),
            layout: Mutex::new(None),
            sound_cfgs: Mutex::new(HashMap::new()),
            timetable_boards: Mutex::new(StopBoards::default()),
            calendar: std::sync::OnceLock::new(),
            registrations: std::sync::OnceLock::new(),
            start_clock: Mutex::new(omsi_sim::SimClock::default()),
        })
    }

    pub fn object_type(&self, rel: &str) -> Option<Arc<ObjectType>> {
        self.object_type_scheme(rel, None)
    }

    /// An object type with one of its `[CTC]` paint schemes applied (parked cars).
    pub fn object_type_scheme(&self, rel: &str, scheme: Option<usize>) -> Option<Arc<ObjectType>> {
        self.object_type_look(rel, scheme, None)
    }

    /// An object type (with a paint scheme), and with `look` its textures as they are in
    /// that season (`Some(None)`: summer's) - a plant of a season's phase that looks unlike
    /// the map's season (see `season_looks`). None for a look no texture of it has.
    pub(super) fn object_type_look(&self, rel: &str, scheme: Option<usize>, look: Option<&Option<String>>) -> Option<Arc<ObjectType>> {
        let key = format!(
            "{}{}{}",
            rel.to_ascii_lowercase().replace('\\', "/"),
            scheme.map(|i| format!("#{i}")).unwrap_or_default(),
            look.map(|l| format!("@{}", l.as_deref().unwrap_or("summer").to_ascii_lowercase())).unwrap_or_default()
        );
        if let Some(t) = self.object_types.lock().get(&key) {
            return t.clone();
        }
        self.object_type_loads.once(&key, || self.read_object_type(&key, rel, scheme, look))
    }

    /// `object_type_look`'s read of a type not read yet, kept under `key`.
    fn read_object_type(&self, key: &str, rel: &str, scheme: Option<usize>, look: Option<&Option<String>>) -> Option<Arc<ObjectType>> {
        if let Some(t) = self.object_types.lock().get(key) {
            // (read by another meanwhile)
            return t.clone();
        }
        let path = omsi_cfg::resolve_path(&self.root, rel);
        let loaded = (|| -> Option<Arc<ObjectType>> {
            let mut sco = SceneryObject::load(&path)
                .map_err(|e| log::warn!("{e}"))
                .ok()?;
            let sco_dir = path.parent()?.to_path_buf();
            let (model, model_dir) = match &sco.model_file {
                Some(m) => {
                    let mp = omsi_cfg::resolve_path(&sco_dir, m);
                    let model = Model::load(&mp).map_err(|e| log::warn!("{e}")).ok()?;
                    (model, mp.parent()?.to_path_buf())
                }
                None => (sco.model.clone(), sco_dir.clone()),
            };
            // OMSI reads these world-pass tags from a referenced model.cfg as well as from
            // the .sco wrapper. Preserve explicit wrapper values, including an explicit
            // Normal/false override; otherwise inherit the model definition as the C++ path
            // does. Missing render phases leave junction geometry in Normal, after splines.
            sco.inherit_model_tags(&model);
            let mut meshes = Vec::new();
            let mut mesh_visible = Vec::new();
            let mut mesh_def_index = Vec::new();
            let mut mesh_pivots = Vec::new();
            if !model.lods.is_empty() {
                let start = model.lods[0].first_mesh;
                for (i, md) in model.lod_meshes(0).iter().enumerate() {
                    let mesh_path = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&model_dir, "model"), &md.file);
                    let mesh_path = if omsi_cfg::vfs::is_file(&mesh_path) {
                        mesh_path
                    } else {
                        omsi_cfg::resolve_path(&model_dir, &md.file)
                    };
                    match omsi_o3d::load_mesh(&mesh_path) {
                        Ok(m) => {
                            meshes.push((
                                mesh_from_o3d(&m),
                                m.materials.clone(),
                                md.materials.clone(),
                            ));
                            mesh_visible.push(md.visible.clone());
                            mesh_def_index.push(start + i);
                            mesh_pivots.push(omsi_sim::anim::pivot_from_mesh(&m));
                        }
                        Err(e) => log::debug!("{}: {e}", mesh_path.display()),
                    }
                }
            }
            // lower detail levels
            let mut lower_lods = Vec::new();
            for l in 1..model.lods.len() {
                let mut list = Vec::new();
                for md in model.lod_meshes(l) {
                    let mesh_path = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&model_dir, "model"), &md.file);
                    let mesh_path = if omsi_cfg::vfs::is_file(&mesh_path) {
                        mesh_path
                    } else {
                        omsi_cfg::resolve_path(&model_dir, &md.file)
                    };
                    if let Ok(m) = omsi_o3d::load_mesh(&mesh_path) {
                        list.push((mesh_from_o3d(&m), m.materials.clone(), md.materials.clone()));
                    }
                }
                lower_lods.push((model.lods[l].min_size, list));
            }
            let lod0_min = model.lods.first().map(|l| l.min_size).unwrap_or(0.0);
            // [CTC] paint schemes (.cti items): retain their texture keys and folders so
            // the selected advertisements can be resolved when a placement chooses them.
            let ctc_schemes: Vec<(String, Vec<omsi_sim::vehicle::PaintScheme>)> = model
                .ctc
                .iter()
                .map(|c| {
                    (
                        c.variable.clone(),
                        omsi_sim::vehicle::load_paint_schemes(&omsi_cfg::resolve_path(
                            &sco_dir, &c.path,
                        )),
                    )
                })
                .collect();
            let paint_schemes: Vec<omsi_sim::vehicle::PaintScheme> = ctc_schemes
                .iter()
                .flat_map(|(_, schemes)| schemes.iter().cloned())
                .collect();
            let mut dynamic_textures: Vec<DynamicTextureGroup> = ctc_schemes
                .iter()
                .map(|(variable, schemes)| DynamicTextureGroup {
                    variable: variable.clone(),
                    choices: schemes
                        .iter()
                        .map(|scheme| {
                            scheme
                                .textures
                                .iter()
                                .filter_map(|(name, file)| {
                                    model
                                        .ctc_textures
                                        .iter()
                                        .find(|(ctc_name, _)| ctc_name.eq_ignore_ascii_case(name))
                                        .map(|(_, default)| {
                                            (default.clone(), file.clone(), scheme.dir.clone())
                                        })
                                })
                                .collect()
                        })
                        .collect(),
                })
                .collect();
            // Scenery models can also use the same script-variable texture selectors as
            // vehicles. Each [newtexchangemaster] is an independent dynamic texture group.
            dynamic_textures.extend(
                omsi_model::load_texchanges(&model_dir, &model.texchanges)
                    .into_iter()
                    .map(|master| {
                        let omsi_model::TexChangeMaster {
                            texture,
                            variable,
                            entries,
                            dir,
                        } = master;
                        DynamicTextureGroup {
                            variable,
                            choices: entries
                                .into_iter()
                                .map(|file| vec![(texture.clone(), file, dir.clone())])
                                .collect(),
                        }
                    }),
            );
            if let Some(ps) = scheme.and_then(|i| paint_schemes.get(i)) {
                let mut map: HashMap<String, String> = HashMap::new();
                for (ctc_name, file) in &ps.textures {
                    // (the scheme's picture lies in the scheme's folder, as for the buses;
                    // taken as a bare name it was looked for among the model's textures, not
                    // found, and the parked car stood there white)
                    let in_scheme = omsi_cfg::resolve_path(&ps.dir, file);
                    let file = if omsi_cfg::vfs::is_file(&in_scheme) { in_scheme.to_string_lossy().into_owned() } else { file.clone() };
                    for (name, default) in &model.ctc_textures {
                        if name.eq_ignore_ascii_case(ctc_name) {
                            map.insert(default.to_ascii_lowercase(), file.clone());
                        }
                    }
                }
                let subst = |t: &mut String| {
                    if let Some(n) = map.get(&t.to_ascii_lowercase()) {
                        *t = n.clone();
                    }
                };
                for (_, mats, overrides) in meshes
                    .iter_mut()
                    .chain(lower_lods.iter_mut().flat_map(|l| l.1.iter_mut()))
                {
                    mats.iter_mut().for_each(|m| subst(&mut m.texture));
                    overrides.iter_mut().for_each(|o| subst(&mut o.texture));
                }
            }
            let paint_scheme_count = paint_schemes.len();
            let has_mouse_events = mesh_def_index.iter().any(|d| {
                model.meshes.get(*d).and_then(|m| m.mouse_event.as_ref()).is_some()
            });
            let program = object_program(&self.root, &sco, &model, &mesh_def_index, has_mouse_events);
            let mesh_shadow = mesh_def_index
                .iter()
                .map(|d| model.meshes[*d].is_shadow)
                .collect();
            let mesh_casts = mesh_def_index.iter().map(|d| model.meshes[*d].shadow).collect();
            let deform = sco.crossing_height_deformation.as_ref().and_then(|f| {
                let mp = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&model_dir, "model"), f);
                let mp = if omsi_cfg::vfs::is_file(&mp) {
                    mp
                } else {
                    omsi_cfg::resolve_path(&model_dir, f)
                };
                match omsi_o3d::load_mesh(&mp) {
                    Ok(m) => Some(mesh_from_o3d(&m)),
                    Err(e) => {
                        log::warn!("crossing height deformation {}: {e}", mp.display());
                        None
                    }
                }
            });
            // Omsi.exe hands the deformation mesh to the model loader, which drapes every
            // [mesh] of the object onto it and rebuilds the normals from the faces
            // (D3DXComputeNormals): the file's normals of a crossing are never used.
            if deform.is_some() {
                for (mesh, _, _) in meshes
                    .iter_mut()
                    .chain(lower_lods.iter_mut().flat_map(|l| l.1.iter_mut()))
                {
                    omsi_geometry::compute_normals_d3d(mesh);
                }
            }
            // [terrainhole] <mesh>: the cutter that takes the ground away under a junction
            // or an underpass, so the carriageway is not buried under a mound of terrain
            let holes: Vec<MeshData> = sco
                .terrain_hole_sources(&model)
                .filter_map(|(hole_dir, f)| {
                    // the cutter sits next to the model, which is either the object's own
                    // folder or a `model` folder inside it
                    let mp = omsi_cfg::resolve_path(hole_dir, f);
                    let mp = if omsi_cfg::vfs::is_file(&mp) {
                        mp
                    } else {
                        omsi_cfg::resolve_path(&omsi_cfg::resolve_path(hole_dir, "model"), f)
                    };
                    match omsi_o3d::load_mesh(&mp) {
                        Ok(m) => Some(mesh_from_o3d(&m)),
                        Err(e) => {
                            log::warn!("terrain hole {}: {e}", mp.display());
                            None
                        }
                    }
                })
                .collect();
            let collision = sco
                .collision_mesh
                .as_ref()
                .filter(|_| !sco.no_collision)
                .and_then(|f| {
                    let mp = omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&model_dir, "model"), f);
                    let mp = if omsi_cfg::vfs::is_file(&mp) {
                        mp
                    } else {
                        omsi_cfg::resolve_path(&model_dir, f)
                    };
                    let mp = if omsi_cfg::vfs::is_file(&mp) {
                        mp
                    } else {
                        omsi_cfg::resolve_path(&sco_dir, f)
                    };
                    omsi_o3d::load_mesh(&mp)
                        .map(|m| mesh_from_o3d(&m))
                        .map_err(|e| log::debug!("collision mesh {}: {e}", mp.display()))
                        .ok()
                });
            let paint = paint_at_foot(&sco, &meshes);
            Some(Arc::new(ObjectType {
                sco,
                sound_path: Default::default(),
                model,
                model_dir,
                meshes,
                mesh_visible,
                mesh_def_index,
                mesh_pivots,
                mesh_shadow,
                mesh_casts,
                has_mouse_events,
                program,
                lower_lods,
                lod0_min,
                paint_scheme_count,
                dynamic_textures,
                holes,
                deform,
                collision,
                paint,
                // (worked out on first use)
                camera: Default::default(), collision_shape: Default::default(), embedded_lights: Default::default(),
            }))
        })();
        // the other season's look: its own copy of the type (its own textures on the GPU),
        // kept only when a texture of it differs
        let loaded = match (loaded, look) {
            (Some(mut t), Some(l)) => Arc::get_mut(&mut t).and_then(|ot| self.retexture_for_look(ot, l.as_deref())).map(|_| t),
            (t, _) => t,
        };
        // two loaders may have read the same type at once: all of them get the first copy,
        // so that it is uploaded (and evicted) once
        self.object_types
            .lock()
            .entry(key.to_string())
            .or_insert(loaded)
            .clone()
    }

    pub fn spline_type(&self, rel: &str) -> Option<Arc<SplineType>> {
        let key = rel.to_ascii_lowercase().replace('\\', "/");
        if let Some(t) = self.spline_types.lock().get(&key) {
            return t.clone();
        }
        self.spline_type_loads.once(&key, || self.read_spline_type(&key, rel))
    }

    /// `spline_type`'s read of a type not read yet, kept under `key`.
    fn read_spline_type(&self, key: &str, rel: &str) -> Option<Arc<SplineType>> {
        if let Some(t) = self.spline_types.lock().get(key) {
            return t.clone();
        }
        let path = omsi_cfg::resolve_path(&self.root, rel);
        let loaded = Spline::load(&path)
            .map_err(|e| log::warn!("{e}"))
            .ok()
            .map(|def| {
                omsi_geometry::register_half_cant_width(rel, &def);
                let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let dirs = texture_dirs(&self.root, &dir);
                let dirs: Vec<&Path> = dirs.iter().map(|p| p.as_path()).collect();
                let surf = def.textures.iter().map(|t| surf_map(&t.file, &dirs)).collect();
                let surface = def.textures.iter().map(|t| surface_id(&t.file, &dirs)).collect();
                Arc::new(SplineType { dir, def, surf, surface })
            });
        self.spline_types
            .lock()
            .entry(key.to_string())
            .or_insert(loaded)
            .clone()
    }

    /// The whole map's road network and where its objects stand, read from the tile files
    /// alone - the splines' and objects' paths, no mesh, no texture - for the navigator,
    /// which must route beyond the tiles loaded around the bus. Objects placed on the ground
    /// take the tile's terrain height; editor-only splines and objects count (some maps put
    /// all their traffic paths on invisible splines).
    pub fn navigation_map(&self) -> NavigationMap {
        let tiles: Vec<(i32, i32, PathBuf)> = self.map_tiles().into_iter().map(|(_, x, y, p)| (x, y, p)).collect();
        navigation_map_of(&self.root, &tiles, &self.chrono_dirs())
    }
}

/// The same read with no `World` holding the caches, so that anything else that wants to
/// know what a map has (the launcher's map picture) reads it the way the navigator does.
/// `root` is the installation the map belongs to: what its tiles name - a `.sli`, a `.sco`
/// and the model beside it - is resolved against it, so a mod's own copy wins over the
/// original's and a map that lacks one borrows the other installation's (`omsi_cfg::
/// resolve_path`).
pub fn navigation_map_of(root: &Path, tiles: &[(i32, i32, PathBuf)], chrono_dirs: &[PathBuf]) -> NavigationMap {
    use rayon::prelude::*;
    let t0 = std::time::Instant::now();
    // (each file read once: a tile that wants one being read waits for it, rather than
    // reading it too - see `OnceByKey`)
    type Once<T> = Arc<std::sync::OnceLock<Option<Arc<T>>>>;
    let scos: Mutex<HashMap<String, Once<SceneryObject>>> = Mutex::new(HashMap::new());
    let sco_of = |file: &str| -> Option<Arc<SceneryObject>> {
        let key = file.trim().to_ascii_lowercase().replace('\\', "/");
        let cell = scos.lock().entry(key).or_default().clone();
        cell.get_or_init(|| SceneryObject::load(&omsi_cfg::resolve_path(root, file)).ok().map(Arc::new)).clone()
    };
    let slis: Mutex<HashMap<String, Once<Spline>>> = Mutex::new(HashMap::new());
    let sli_of = |file: &str| -> Option<Arc<Spline>> {
        // (the navigator loads `.sli` through `World::spline_type` with the same two calls)
        let key = file.trim().to_ascii_lowercase().replace('\\', "/");
        let cell = slis.lock().entry(key).or_default().clone();
        cell.get_or_init(|| {
            Spline::load(&omsi_cfg::resolve_path(root, file)).ok().map(|def| {
                omsi_geometry::register_half_cant_width(file, &def);
                Arc::new(def)
            })
        })
        .clone()
    };
    #[allow(clippy::type_complexity)]
    let parts: Vec<(Vec<Lane>, Vec<(i64, DVec3)>, Vec<(DVec3, f64, String)>, Vec<(Vec<DVec3>, f32)>, Vec<(DVec3, String)>)> = tiles
        .par_iter()
        .map(|(tx, ty, path)| {
            let (tx, ty) = (*tx, *ty);
            let mut lanes = Vec::new();
            let mut positions = Vec::new();
            let mut signs = Vec::new();
            let mut roads = Vec::new();
            let mut names = Vec::new();
            let Some(tile) = omsi_world::map_index::read_tile(path, chrono_dirs) else {
                return (lanes, positions, signs, roads, names);
            };
            let origin2 = DVec2::new(tx as f64 * tile_size(), ty as f64 * tile_size());
            let terrain = Terrain::load(&tile_companion(&path, ".terrain")).unwrap_or_else(|_| Terrain::flat());
            for sp in tile.splines.iter().filter(|s| !s.deleted && !s.file.trim().is_empty()) {
                let Some(def) = sli_of(&sp.file) else { continue };
                if !def.paths.iter().any(|p| p.kind == 0) {
                    // Short surfaces also corroborate editor-only traffic paths at
                    // junctions; the navigator filters decorative patches for display.
                    if sp.length >= 2.0 {
                        let curve = SplineCurve::from_map(sp, origin2).with_sli(&def);
                        let n = ((curve.length / 4.0).ceil() as usize).clamp(1, 400);
                        let side = if sp.mirror { -1.0 } else { 1.0 };
                        for (lo, hi, z) in road_sections(&sp.file, &def) {
                            let offset = side * ((lo + hi) * 0.5) as f64;
                            let pts: Vec<DVec3> = (0..=n).map(|k| curve.offset_point(curve.length * k as f64 / n as f64, offset, z as f64)).collect();
                            roads.push((pts, hi - lo));
                        }
                    }
                }
                if def.paths.is_empty() {
                    continue;
                }
                let curve = SplineCurve::from_map(sp, origin2);
                let mut new_lanes = spline_lanes(&def, sp, &curve, (tx, ty));
                for l in new_lanes.iter_mut() {
                    l.invisible = def.only_editor;
                }
                lanes.extend(new_lanes);
            }
            for o in &tile.objects {
                if o.file.trim().is_empty() {
                    continue;
                }
                let (x, y) = (origin2.x + o.pos[0], origin2.y + o.pos[1]);
                let ground = || {
                    let (lx, ly) = ((x - origin2.x).clamp(0.0, tile_size()) as f32, (y - origin2.y).clamp(0.0, tile_size()) as f32);
                    terrain.sample(lx, ly) as f64
                };
                // street name signs carry the street's name as their text
                if is_street_sign(&o.file) {
                    if let Some(name) = o.extra.first().map(|t| clean_sign_text(t)).filter(|t| t.chars().filter(|c| c.is_alphabetic()).count() >= 3) {
                        signs.push((DVec3::new(x, y, o.pos[2] + ground()), o.rot[0], name));
                    }
                }
                let Some(sco) = sco_of(&o.file) else {
                    positions.push((o.id, DVec3::new(x, y, o.pos[2] + ground())));
                    continue;
                };
                // a name plate of another kind than the stock street sign (an add-on's own
                // sign, a sign with the name on its own object), or a bus stop named after its
                // street: a weaker hint of the street's name (see `street_name_of`)
                if !is_street_sign(&o.file) {
                    let plate = {
                        let f = o.file.to_ascii_lowercase();
                        ["schild", "sign", "name", "plate", "tafel"].iter().any(|k| f.contains(k))
                    };
                    if plate || sco.is_bus_stop {
                        for t in o.extra.iter().take(3) {
                            if let Some(n) = street_name_of(t) {
                                if plate {
                                    signs.push((DVec3::new(x, y, o.pos[2] + ground()), o.rot[0], n));
                                } else {
                                    names.push((DVec3::new(x, y, o.pos[2] + ground()), n));
                                }
                                break;
                            }
                        }
                    }
                }
                let absolute = sco.absolute_height();
                let pos = DVec3::new(x, y, if absolute { o.pos[2] } else { o.pos[2] + ground() });
                positions.push((o.id, pos));
                if !sco.paths.is_empty() {
                    lanes.extend(object_lanes(&sco, pos, [o.rot[0], 0.0, 0.0], None, (tx, ty), o.id, &o.rules));
                }
            }
            (lanes, positions, signs, roads, names)
        })
        .collect();
    let mut lanes = Vec::new();
    let mut positions = HashMap::new();
    let mut signs = Vec::new();
    let mut roads = Vec::new();
    let mut names = Vec::new();
    for (l, p, s, r, n) in parts {
        lanes.extend(l);
        positions.extend(p);
        signs.extend(s);
        roads.extend(r);
        names.extend(n);
    }
    log::info!("navigation map: {} roads without a path for cars", roads.len());
    log::info!(
        "navigation map: {} tiles, {} lanes, {} objects placed, {} street name signs, {} object types, {} spline types, {:.1} s",
        tiles.len(),
        lanes.len(),
        positions.len(),
        signs.len(),
        scos.lock().len(),
        slis.lock().len(),
        t0.elapsed().as_secs_f64()
    );
    NavigationMap { lanes, road_surfaces: roads, places: positions, signs, names }
}

/// The whole map for the navigator (see [`World::navigation_map`]).
pub struct NavigationMap {
    pub lanes: Vec<Lane>,
    /// Asphalt footprints used to corroborate editor-only driving paths. They are evidence
    /// for roads, not streets to draw: a paved yard or median has no road centre line.
    pub road_surfaces: Vec<(Vec<DVec3>, f32)>,
    /// Every placed object's position by id (bus stops beyond the loaded tiles).
    pub places: HashMap<i64, DVec3>,
    /// Street name signs: where, the object's heading and the name on it.
    pub signs: Vec<(DVec3, f64, String)>,
    /// Weaker hints of a street's name: bus stops named after the street they stand in.
    pub names: Vec<(DVec3, String)>,
}

/// A sign's text without the codes the sign's script reads off it: Berlin's plates write
/// "F_Hamburger Str" (the `F_` picks the plate's face), others an underscore for a space.
pub fn clean_sign_text(text: &str) -> String {
    let t = text.trim();
    let mut c = t.chars();
    let t = match (c.next(), c.next()) {
        (Some(a), Some('_')) if a.is_ascii_alphabetic() => &t[2..],
        _ => t,
    };
    t.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A text that reads as a street's name, cleaned (a stop's "U Rathaus Spandau" is no street,
/// "Heerstr/Wilhelmstr" names a crossing, "S+U Bf. Zoo" a station): the words maps write
/// streets with, in the languages OMSI maps are made in.
pub fn street_name_of(text: &str) -> Option<String> {
    let cleaned = clean_sign_text(text);
    let t = cleaned.trim_matches(|c: char| c == '"' || c == '\'');
    if t.contains('/') || t.contains('|') || t.chars().filter(|c| c.is_alphabetic()).count() < 4 || t.chars().count() > 48 {
        return None;
    }
    let lower = t.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| c.is_whitespace() || c == '-' || c == ',').filter(|w| !w.is_empty()).collect();
    let first = *words.first()?;
    let ends = [
        // German, Dutch, Scandinavian
        "straße", "strasse", "str.", "str", "weg", "allee", "platz", "damm", "ring", "chaussee", "gasse", "ufer", "steig", "pfad", "zeile", "promenade", "markt", "brücke", "bruecke", "steinweg", "straat", "laan", "plein", "gracht", "gatan", "gata", "vägen", "vej", "gade", "veien",
        // English
        "road", "rd", "rd.", "street", "st", "st.", "lane", "avenue", "ave", "drive", "way", "close", "crescent", "hill", "place", "square", "terrace", "grove", "gardens", "row", "walk", "parade", "broadway", "boulevard", "highway", "bypass", "mews", "court",
        // Romance
        "rue", "via", "viale", "piazza", "calle", "avenida", "rua", "strada", "bulevar",
    ];
    let starts = ["rue", "avenue", "boulevard", "chemin", "via", "viale", "piazza", "calle", "avenida", "rua", "ul.", "ul", "ulica", "aleja", "al.", "plac", "ulice", "náměstí", "třída", "улица", "ул.", "проспект", "пр.", "шоссе", "переулок", "бульвар", "площадь", "вулиця", "вул."];
    let last = *words.last()?;
    let ok = words.len() >= 2 && (ends.contains(&last) || starts.contains(&first))
        || ends.iter().any(|e| e.len() >= 4 && last.len() > e.len() + 2 && last.ends_with(e))
        || (last.len() > 5 && (last.ends_with("str") || last.ends_with("str.")))
        || words.iter().any(|w| ["utca", "út", "útja", "ulica", "улица", "проспект", "шоссе", "переулок"].contains(w));
    // (a station named after its street is still a station: "U Rathaus", "S Spandau")
    let station = ["u", "s", "s+u", "u+s", "bf", "bf.", "bhf", "bhf.", "hbf"].contains(&first);
    (ok && !station).then(|| t.to_string())
}

/// Whether an asset name describes a road surface. Match whole filename tokens so objects
/// such as `StreetLight.sli` do not become roads just because their name contains "street".
pub(super) fn road_surface_name(file: &str) -> bool {
    let base = file.replace('\\', "/").rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(&base);
    let tokens: Vec<&str> = stem.split(|c: char| !c.is_ascii_alphanumeric() && c != 'ß').filter(|t| !t.is_empty()).collect();
    let excludes = ["light", "lamp", "sign", "schild", "rail", "track", "gleis", "tram", "strab", "wire", "mast", "wall", "fence", "leitplanke", "gehweg", "side", "bord", "pavement", "fahrrad", "radweg", "cycle", "parking", "parkplatz", "gruen", "gras", "trottoir", "pieton"];
    if tokens.iter().any(|t| excludes.iter().any(|x| t.contains(x))) {
        return false;
    }
    stem.starts_with("str_")
        || tokens.iter().any(|t| {
            ["str", "strasse", "straße", "road", "roads", "street", "streets", "fahrbahn", "pflaster", "kopfstein", "cobble", "route", "routes", "rue", "chaussee", "bitume"].contains(t)
                || t.starts_with("asph")
        })
}

/// The horizontal road surfaces actually drawn by a pathless spline: lateral bounds and
/// height. A texture merely listed in the file is not evidence of a road, and the origin
/// need not be in the middle of the surface. Keep medians and pavements out of its width.
pub(super) fn road_sections(file: &str, def: &omsi_scenery::sli::Spline) -> Vec<(f32, f32, f32)> {
    let name = file.to_ascii_lowercase();
    if def.only_editor || ["gehweg", "radweg", "fahrrad", "tram", "strab", "gleis", "rail", "parking"].iter().any(|s| name.contains(s)) || def.paths.iter().any(|p| p.kind == 2) {
        return Vec::new();
    }
    let mut sections = Vec::new();
    for profile in &def.profiles {
        let road = def.textures.get(profile.texture).map(|t| road_surface_name(&t.file)).unwrap_or_else(|| def.textures.is_empty() && road_surface_name(file));
        if !road { continue; }
        for pair in profile.points.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
            if lo.is_finite() && hi.is_finite() && a.z.is_finite() && b.z.is_finite() && hi - lo > 0.1 && (a.z - b.z).abs() <= (hi - lo) * 0.15 {
                sections.push((lo, hi, (a.z + b.z) * 0.5));
            }
        }
    }
    sections.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32, f32)> = Vec::new();
    for (lo, hi, z) in sections {
        if let Some(last) = merged.last_mut().filter(|s| lo <= s.1 + 0.2 && (z - s.2).abs() < 0.2) {
            last.1 = last.1.max(hi);
        } else {
            merged.push((lo, hi, z));
        }
    }
    merged.retain(|(lo, hi, _)| hi - lo >= 4.0);
    merged
}

/// A street name sign object (the stock Verkehrszeichen_MC `StreetSign_*`, and the
/// German `Strassenschild`/`StrSchild` names add-on maps use).
pub(super) fn is_street_sign(file: &str) -> bool {
    let f = file.to_ascii_lowercase().replace(['\\', '_', ' ', '-'], "");
    let name = f.rsplit('/').next().unwrap_or(&f);
    ["streetsign", "streetname", "strschild", "strassenschild", "straßenschild", "strassenname", "roadsignname", "roadname"].iter().any(|k| name.contains(k))
}

fn object_program(
    root: &Path,
    sco: &SceneryObject,
    model: &Model,
    mesh_def_index: &[usize],
    has_mouse_events: bool,
) -> Option<Arc<omsi_script::Program>> {
    let animated = mesh_def_index.iter().any(|d| {
        !model.meshes[*d].animations.is_empty() || model.meshes[*d].visible_groups().next().is_some()
    });
    let has_freetex = mesh_def_index.iter().any(|d| {
        model.meshes[*d].materials.iter().any(|o| !o.item && o.freetex.is_some())
    });
    if !sco.scripts.scripts.is_empty()
        || !sco.scripts.stringvarlists.is_empty()
        || !sco.scripts.varlists.is_empty()
        || has_freetex
        || animated
        || has_mouse_events
        || sco.sound.is_some()
        || !sco.scripts.lua.is_empty()
    {
        Some(Arc::new(omsi_sim::scenery::compile_scenery(
            root,
            &sco.scripts,
        )))
    } else {
        None
    }
}
