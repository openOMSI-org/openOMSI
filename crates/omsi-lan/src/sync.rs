//! Vehicle sync tables, canonical variable indices and sound configuration.

use omsi_script::VarId;
use std::path::PathBuf;
use std::sync::Arc;

/// The variables of a vehicle type the state carries beyond the pose, the same on every
/// machine for the same files: the lamps of its model (`[matl_lightmap]`, `[matl_change]`,
/// `[light_enh]`, `[light_enh_2]`, `[interiorlight]`), its `[visible]` switches, and the
/// variables its outside sounds follow (volume curves, pitch, conditions) together with
/// those of the parts that move where they can be seen (wipers, ramps, doors not animated
/// by `door_N`). Each list is sorted by name, so both games agree on the order; `hash`
/// tells when they do not (another version of the vehicle).
pub struct SyncTable {
    pub lamps: Vec<(String, VarId)>,
    pub switches: Vec<(String, VarId)>,
    pub values: Vec<(String, VarId)>,
    /// `door_0`, `door_1` … as far as the scripts have them.
    pub doors: Vec<VarId>,
    /// All model animation inputs, independently of the capped pose values list.
    pub animations: hashbrown::HashSet<VarId>,
    pub hash: u32,
    /// Variables of the horn: its switch and the volume its sound follows.
    pub horn: Vec<VarId>,
    pub engine_n: Option<VarId>,
    pub ai_engine: Option<VarId>,
    pub ai_light: Option<VarId>,
    pub ai_interior: Option<VarId>,
    pub throttle: Option<VarId>,
    pub brake: Option<VarId>,
    /// The outside sounds: `[sound_ai]` (or `[sound]`), and the horn and indicator relay
    /// entries of `[sound]` - with the folder their files are in.
    pub sounds: Vec<(Arc<omsi_vehicle::SoundCfg>, PathBuf)>,
    /// The whole `[sound]` set, heard from inside by whoever rides in the bus.
    pub interior: Option<(Arc<omsi_vehicle::SoundCfg>, PathBuf)>,
    /// The rear sections' outside sounds (`[sound_ai]`, or `[sound]`), by section.
    pub part_sounds: Vec<(usize, Arc<omsi_vehicle::SoundCfg>, PathBuf)>,
}

/// Engine variables every copy works out for itself (or that come in the pose).
pub fn engine_fed(name: &str) -> bool {
    const PREFIXES: [&str; 12] = [
        "wheel_",
        "axle_",
        "velocity",
        "ai_",
        "envir_",
        "dirt",
        "precip",
        "rain_",
        "streetcond",
        "door_",
        "pax_",
        "refresh_",
    ];
    let n = name.to_ascii_lowercase();
    PREFIXES.iter().any(|p| n.starts_with(p))
        || matches!(n.as_str(), "n_wheel" | "wetness" | "time" | "timegap")
}

/// Canonical wire indices are sorted by name, independently of local compiler IDs.
/// All numeric and string variables participate, including engine-fed and implicit ones.
pub struct VarTable {
    pub hash: u32,
    pub floats: Vec<u32>,
    pub strings: Vec<u32>,
}

pub fn var_table(program: &omsi_script::Program) -> VarTable {
    let mut h: u32 = 0x811c_9dc5;
    let mut collect = |names: &[String], kind: u8| {
        h = (h ^ kind as u32).wrapping_mul(0x0100_0193);
        let mut entries: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(id, name)| (name.to_ascii_lowercase(), id as u32))
            .collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        entries
            .into_iter()
            .take(u16::MAX as usize + 1)
            .map(|(name, id)| {
                for b in name.bytes().chain(std::iter::once(0)) {
                    h = (h ^ b as u32).wrapping_mul(0x0100_0193);
                }
                id
            })
            .collect()
    };
    let floats = collect(&program.var_names, 0);
    let strings = collect(&program.str_var_names, 1);
    VarTable {
        hash: h,
        floats,
        strings,
    }
}

/// Sound entries of the full `[sound]` set that are heard from outside as well: the horn,
/// the indicator relay, kneeling.
pub fn outside_entry(e: &omsi_vehicle::SoundEntry) -> bool {
    let hit = |s: &str| {
        let l = s.to_ascii_lowercase();
        ["hupe", "horn", "blinker", "kneel"]
            .iter()
            .any(|k| l.contains(k))
    };
    e.triggers.iter().any(|t| hit(t))
        || e.vol_curves.iter().any(|c| hit(&c.variable))
        || e.conditions.iter().any(|c| hit(&c.variable))
}

/// Supplement sound_ai without losing the other names of a multi-trigger entry.
fn supplemental_sound_entry(
    entry: &omsi_vehicle::SoundEntry,
    present: &dyn Fn(&str) -> bool,
) -> Option<omsi_vehicle::SoundEntry> {
    if entry.triggers.is_empty() {
        return outside_entry(entry).then(|| entry.clone());
    }
    let mut entry = entry.clone();
    entry.triggers.retain(|t| !present(t));
    (!entry.triggers.is_empty()).then_some(entry)
}

pub fn fnv1a(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in data {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

impl SyncTable {
    /// `parts`: the types of its rear sections, whose lamps, displays, moving parts and
    /// sounds follow the leading vehicle's variables as its own do (an articulated bus's
    /// rear section stood dark and silent in the other players' games: none of its
    /// variables were in the table).
    pub fn new(ty: &omsi_sim::VehicleType, parts: &[Arc<omsi_sim::VehicleType>]) -> SyncTable {
        let program = &ty.program;
        let types: Vec<&omsi_sim::VehicleType> = std::iter::once(ty)
            .chain(parts.iter().map(|p| p.as_ref()))
            .collect();
        let animations =
            crate::animation_sync::model_variables(program, types.iter().map(|t| &t.model));
        let var = |n: &str| -> Option<VarId> {
            let n = n.trim();
            if n.is_empty() || n.parse::<f32>().is_ok() {
                return None;
            }
            program.var(n)
        };
        // names as the program spells them, sorted and unique, capped as the wire allows
        let collect = |names: &mut dyn Iterator<Item = String>,
                       skip: &dyn Fn(&str) -> bool,
                       cap: usize|
         -> Vec<(String, VarId)> {
            let mut v: Vec<(String, VarId)> = names
                .filter_map(|n| var(&n).map(|id| (program.var_names[id as usize].clone(), id)))
                .filter(|(n, _)| !skip(n))
                .collect();
            v.sort_by_key(|(n, _)| n.to_ascii_lowercase());
            v.dedup_by_key(|(n, _)| n.to_ascii_lowercase());
            v.truncate(cap);
            v
        };
        let paint_vars: Vec<String> = ty
            .paint_schemes
            .iter()
            .flat_map(|s| s.set_vars.iter().map(|(n, _)| n.to_ascii_lowercase()))
            .collect();
        let lamp_names = types
            .iter()
            .flat_map(|t| t.model.meshes.iter())
            .flat_map(|m| {
                m.materials
                    .iter()
                    .flat_map(|mat| {
                        mat.lightmap
                            .as_ref()
                            .map(|l| l.1.clone())
                            .into_iter()
                            .chain(mat.change.as_ref().map(|c| c.2.clone()))
                    })
                    .chain(m.light_enh.iter().map(|l| l.variable.clone()))
                    .chain(m.light_enh_2.iter().map(|l| l.variable.clone()))
            })
            .chain(
                types
                    .iter()
                    .flat_map(|t| t.model.interior_lights.iter())
                    .map(|l| l.variable.clone()),
            )
            .chain(
                types
                    .iter()
                    .flat_map(|t| t.model.spotlights_2.iter())
                    .map(|l| l.variable.clone()),
            );
        let lamps = collect(
            &mut lamp_names.collect::<Vec<_>>().into_iter(),
            &|n| engine_fed(n) || paint_vars.contains(&n.to_ascii_lowercase()),
            omsi_net::wire::MAX_LAMPS,
        );
        let switch_names: Vec<String> = types
            .iter()
            .flat_map(|t| t.model.meshes.iter())
            .flat_map(|m| m.visible_groups().flatten().map(|c| c.variable.clone()))
            .collect();
        let lamp_set: Vec<VarId> = lamps.iter().map(|l| l.1).collect();
        let switches = collect(
            &mut switch_names.into_iter(),
            &|n| {
                engine_fed(n)
                    || paint_vars.contains(&n.to_ascii_lowercase())
                    || var(n).map(|id| lamp_set.contains(&id)).unwrap_or(true)
            },
            omsi_net::wire::MAX_SWITCHES,
        );
        // the outside sounds
        let def = &ty.def;
        let load = |rel: &str| -> Option<(Arc<omsi_vehicle::SoundCfg>, PathBuf)> {
            let path = omsi_cfg::resolve_path(def.dir(), rel);
            match omsi_vehicle::SoundCfg::load(&path) {
                Ok(c) => Some((
                    Arc::new(c),
                    path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
                )),
                Err(e) => {
                    log::warn!("LAN: sounds of {}: {e}", def.path.display());
                    None
                }
            }
        };
        let mut sounds = Vec::new();
        let interior = def.sound.as_deref().and_then(|rel| load(rel));
        if let Some(rel) = def.sound_ai.as_deref().or(def.sound.as_deref()) {
            sounds.extend(load(rel));
        }
        if let (Some(_), Some(full)) = (def.sound_ai.as_deref(), def.sound.as_deref()) {
            if let Some((cfg, dir)) = load(full) {
                let entries: Vec<omsi_vehicle::SoundEntry> = cfg
                    .sounds
                    .iter()
                    .filter_map(|entry| {
                        supplemental_sound_entry(entry, &|trigger| {
                            sounds.iter().any(|(ai, _)| {
                                ai.sounds.iter().any(|entry| {
                                    entry
                                        .triggers
                                        .iter()
                                        .any(|t| t.trim().eq_ignore_ascii_case(trigger.trim()))
                                })
                            })
                        })
                    })
                    .collect();
                if !entries.is_empty() {
                    sounds.push((
                        Arc::new(omsi_vehicle::SoundCfg {
                            sounds: entries,
                            unknown_keywords: Vec::new(),
                        }),
                        dir,
                    ));
                }
            }
        }
        // the rear sections' outside sounds, each where its section is
        let mut part_sounds = Vec::new();
        for (k, part) in parts.iter().enumerate() {
            let pdef = &part.def;
            if let Some(rel) = pdef.sound_ai.as_deref().or(pdef.sound.as_deref()) {
                let path = omsi_cfg::resolve_path(pdef.dir(), rel);
                match omsi_vehicle::SoundCfg::load(&path) {
                    Ok(c) => part_sounds.push((
                        k,
                        Arc::new(c),
                        path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
                    )),
                    Err(e) => log::warn!("LAN: sounds of {}: {e}", pdef.path.display()),
                }
            }
        }
        // the variables the outside sounds follow: the leading vehicle's, then the rear
        // sections' (whatever of the capped list those leave)
        let sound_vars =
            |cfgs: &mut dyn Iterator<Item = &Arc<omsi_vehicle::SoundCfg>>| -> Vec<String> {
                let mut out = Vec::new();
                for cfg in cfgs {
                    for e in &cfg.sounds {
                        out.extend(e.vol_curves.iter().map(|c| c.variable.clone()));
                        out.extend(e.conditions.iter().map(|c| c.variable.clone()));
                        if !e.pitch_variable.is_empty() {
                            out.push(e.pitch_variable.clone());
                        }
                    }
                }
                out
            };
        let sound_names = sound_vars(&mut sounds.iter().map(|s| &s.0));
        let part_sound_names = sound_vars(&mut part_sounds.iter().map(|s| &s.1));
        // what moves where it can be seen
        let mut value_names: Vec<String> = Vec::new();
        // the parts that move where they can be seen (not the cockpit's switches)
        // - and the doors opened by hand, whose meshes are clickable: the W906's cab and rear
        // doors (`[mouseevent] cp_kryshka1_opn`, "kryshka" a Russian mod's word for a
        // leaf) swung open in the driver's game and stayed shut in everybody else's
        const DOORISH: [&str; 6] = ["door", "tuer", "tür", "ramp", "kryshka", "dver"];
        let doorish = |t: &str| {
            let t = t.to_ascii_lowercase();
            DOORISH.iter().any(|k| t.contains(k))
        };
        for (t, i, m) in types.iter().flat_map(|t| {
            t.model
                .meshes
                .iter()
                .enumerate()
                .map(move |(i, m)| (t, i, m))
        }) {
            if !t.meshes.iter().any(|vm| vm.def_index == i) {
                continue;
            }
            let file = m.file.to_ascii_lowercase();
            let wiper =
                ["wisch", "wiper"].iter().any(|k| file.contains(k)) && m.mouse_event.is_none();
            let door = doorish(&file)
                || m.mesh_ident.as_deref().is_some_and(|x| doorish(x))
                || m.mouse_event.as_deref().is_some_and(|x| doorish(x))
                || m.animations.iter().any(|a| doorish(&a.variable));
            if wiper || door {
                value_names.extend(m.animations.iter().map(|a| a.variable.clone()));
            }
        }
        // the water on the windows (the sender's wipers wipe it: another player's bus stood
        // dry in the rain, its window films left out as engine-fed `rain_` variables)
        let rain_film = |n: &str| n.trim().to_ascii_lowercase().starts_with("rain_window");
        for m in types.iter().flat_map(|t| t.model.meshes.iter()) {
            value_names.extend(
                m.materials
                    .iter()
                    .filter_map(|mat| mat.alphascale.clone())
                    .filter(|n| rain_film(n)),
            );
            // what scrolls a texture along its slot: a roller blind turning to the next
            // number (its pictures come in the pose, see `freetex_names`)
            value_names.extend(m.materials.iter().flat_map(|mat| {
                mat.texcoord_trans_x
                    .iter()
                    .chain(mat.texcoord_trans_y.iter())
                    .cloned()
            }));
        }
        let taken: Vec<VarId> = lamps.iter().chain(&switches).map(|l| l.1).collect();
        // What is seen first, then the leading vehicle's sounds, then the rear sections',
        // each sorted by name: the list is capped, and taken as one list in name order the
        // rear sections' sound variables pushed what is seen out of it.
        let skip = |n: &str| {
            (engine_fed(n) && !rain_film(n)) || var(n).map(|id| taken.contains(&id)).unwrap_or(true)
        };
        let mut values = collect(
            &mut value_names.into_iter(),
            &skip,
            omsi_net::wire::MAX_VALUES,
        );
        for names in [sound_names, part_sound_names] {
            let had: Vec<VarId> = values.iter().map(|v| v.1).collect();
            let more = collect(
                &mut names.into_iter(),
                &|n| skip(n) || var(n).is_some_and(|id| had.contains(&id)),
                omsi_net::wire::MAX_VALUES - values.len(),
            );
            values.extend(more);
        }
        let doors: Vec<VarId> = (0..omsi_net::wire::MAX_DOORS)
            .map_while(|i| program.var(&format!("door_{i}")))
            .collect();
        let horn_sound_vars: Vec<VarId> = sounds
            .iter()
            .flat_map(|(c, _)| c.sounds.iter())
            .filter(|e| {
                e.triggers
                    .iter()
                    .chain(e.vol_curves.iter().map(|c| &c.variable))
                    .any(|t| {
                        t.to_ascii_lowercase().contains("hupe")
                            || t.to_ascii_lowercase().contains("horn")
                    })
            })
            .flat_map(|e| e.vol_curves.iter().filter_map(|c| var(&c.variable)))
            .collect();
        let horn: Vec<VarId> = ["cockpit_hupe", "cockpit_hupe_swheel", "horn"]
            .iter()
            .filter_map(|n| var(n))
            .chain(horn_sound_vars)
            .collect();
        let mut key = String::new();
        for (tag, list) in [
            ("lamps", &lamps),
            ("switches", &switches),
            ("values", &values),
        ] {
            key.push_str(tag);
            key.push(':');
            for (n, _) in list.iter() {
                key.push_str(&n.to_ascii_lowercase());
                key.push(',');
            }
            key.push(';');
        }
        key.push_str(&format!("doors:{}", doors.len()));
        SyncTable {
            animations,
            hash: fnv1a(key.as_bytes()).max(1),
            lamps,
            switches,
            values,
            doors,
            horn,
            engine_n: var("engine_n"),
            ai_engine: var("AI_Engine"),
            ai_light: var("AI_Light"),
            ai_interior: var("AI_Interiorlight"),
            throttle: var("Throttle"),
            brake: var("Brake"),
            sounds,
            interior,
            part_sounds,
        }
    }

    pub fn describe(&self) -> String {
        let names =
            |l: &[(String, VarId)]| l.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(" ");
        format!(
            "{} lamps, {} switches, {} doors, values [{}], {} sound set(s) and {} of rear sections, table {:08X}",
            self.lamps.len(),
            self.switches.len(),
            self.doors.len(),
            names(&self.values),
            self.sounds.len(),
            self.part_sounds.len(),
            self.hash
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exterior_sound_supplements_keep_each_missing_trigger_without_duplicate_voices() {
        let entry = omsi_vehicle::SoundEntry {
            triggers: vec!["horn".into(), "door_hit".into(), "announcement".into()],
            ..Default::default()
        };
        let added = supplemental_sound_entry(&entry, &|t| t.eq_ignore_ascii_case("HORN")).unwrap();
        assert_eq!(added.triggers, vec!["door_hit", "announcement"]);
        assert!(supplemental_sound_entry(&entry, &|_| true).is_none());
        assert!(
            supplemental_sound_entry(&omsi_vehicle::SoundEntry::default(), &|_| false).is_none()
        );
    }
    #[test]
    fn all_variables_sync_by_name_independently_of_local_ids() {
        let mut a = omsi_script::Program::default();
        let mut b = omsi_script::Program::default();
        let names = [
            "door_0",
            "pax_custom",
            "AI_Light",
            "implicit",
            "Velocity",
            "engine_n",
        ];
        for name in names {
            a.declare_var(name);
        }
        for name in names.into_iter().rev() {
            b.declare_var(name);
        }
        a.declare_str_var("display");
        a.declare_str_var("path");
        b.declare_str_var("path");
        b.declare_str_var("DISPLAY");
        let ta = var_table(&a);
        let tb = var_table(&b);
        assert_eq!(ta.hash, tb.hash);
        assert_eq!(ta.floats.len(), names.len());
        for (&local_a, &local_b) in ta.floats.iter().zip(&tb.floats) {
            assert_eq!(
                a.var_names[local_a as usize].to_ascii_lowercase(),
                b.var_names[local_b as usize].to_ascii_lowercase()
            );
        }
        for (&local_a, &local_b) in ta.strings.iter().zip(&tb.strings) {
            assert_eq!(
                a.str_var_names[local_a as usize].to_ascii_lowercase(),
                b.str_var_names[local_b as usize].to_ascii_lowercase()
            );
        }
        let mut different_kind = omsi_script::Program::default();
        different_kind.declare_str_var("door_0");
        let mut numeric = omsi_script::Program::default();
        numeric.declare_var("door_0");
        assert_ne!(var_table(&numeric).hash, var_table(&different_kind).hash);
    }
    #[test]
    fn engine_fed_names() {
        for n in [
            "Wheel_RotationSpeed_1_R",
            "Velocity",
            "AI_Light",
            "door_0",
            "StreetCond",
            "Timegap",
        ] {
            assert!(engine_fed(n), "{n}");
        }
        for n in [
            "engine_n",
            "engine_throttle_injection",
            "M_Wheel",
            "doorSpeed_0",
            "wiperpos",
            "cockpit_hupe_volume",
            "lights_stand",
        ] {
            assert!(!engine_fed(n), "{n}");
        }
    }
}
