use super::*;

impl World {
    /// Keep a traffic lamp (or another object a crossing's light program drives) for the
    /// traffic to switch.
    pub(super) fn keep_light_object(
        &self,
        pl: &mut Placing,
        obj: &ObjectCx,
        lamp: (i64, usize, bool),
        made: ObjectMade,
    ) {
        let ObjectCx {
            ot,
            pos,
            xf,
            script_strings,
            key: tile,
            ..
        } = *obj;
        let ObjectMade {
            lamp_instances,
            lamp_slots,
            object_variants,
            lamp_texts,
            ..
        } = made;
        let (parent, index, any_light) = lamp;
        let names: Mutex<Vec<LightSwitch>> = Mutex::new(Vec::new());
        let lights = model_lights_owned(
            &ot.model,
            &|_| xf,
            pos,
            &|var| {
                names.lock().push(LightSwitch::parse(var));
                1.0
            },
            &[],
        );
        let names = names.into_inner();
        let sources = model_light_sources(&ot.model);
        let (coronas, corona_mesh): (
            Vec<(omsi_render::Corona, String)>,
            Vec<(usize, glam::Vec3, glam::Vec3)>,
        ) = lights
            .into_iter()
            .filter_map(|(c, k)| match names.get(k) {
                Some(LightSwitch::Variable(v)) => Some((
                    (c, v.clone()),
                    sources
                        .get(k)
                        .copied()
                        .unwrap_or((0, glam::Vec3::ZERO, glam::Vec3::ZERO)),
                )),
                _ => None,
            })
            .unzip();
        // (the script goes to the simulation's scenery state, the lamp keeps its key)
        let mut animated = false;
        let script = ot.program.as_ref().map(|p| {
            let inst = omsi_sim::scenery::SceneryInstance::new(
                p.clone(),
                &ot.mesh_defs(),
                self.script_clock(),
                script_strings,
            );
            animated = inst.animated();
            let inbox = self.scripted.lock().inbox.clone();
            let key = inbox.next_key();
            inbox.push(omsi_sim::scenery::SceneryChange::Lamp(
                omsi_sim::scenery::LampScript { key, tile, inst },
            ));
            key
        });
        let lit = vec![0.0; coronas.len()];
        let sound = ot.sco.sound.as_ref().map(|rel| {
            let dir = ot
                .sco
                .path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            omsi_cfg::resolve_path(&dir, rel)
        });
        pl.light_objects.push(LightObject {
            parent,
            index,
            any_light,
            instances: lamp_instances,
            slots: lamp_slots,
            variants: object_variants,
            pos,
            script,
            coronas,
            corona_mesh,
            lit,
            xf,
            animated,
            sound,
            sounds: Default::default(),
            shown: None,
            texts: lamp_texts,
        });
    }

    /// Keep an object whose script changes it (its meshes, materials, texts, pages or sound)
    /// for `update_scripted`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn keep_scripted_object(
        &self,
        renderer: &Renderer,
        scene: &mut Scene,
        gpu: &mut GpuCache,
        obj: &ObjectCx,
        mut inst: omsi_sim::scenery::SceneryInstance,
        made: ObjectMade,
        mesh_instances: usize,
    ) {
        let ObjectCx {
            ot,
            pos,
            xf,
            lamp,
            map_id,
            controller,
            strings,
            var_parent,
            key,
            tkey,
            images,
            ..
        } = *obj;
        let ObjectMade {
            mut all_instances,
            object_variants,
            script_texts,
            html_pages,
            ..
        } = made;
        let texture_selection = scenery_texture_selection(ot, &|n| inst.var(n));
        if !ot.dynamic_textures.is_empty() {
            if let Some(rows) = gpu.dynamic_texture_variant(
                renderer,
                scene,
                tkey,
                &texture_selection,
                &self.root,
                images,
            ) {
                for (mi, row) in rows.iter().enumerate() {
                    let Some(&mesh_inst) = all_instances.get(mi) else {
                        continue;
                    };
                    for (slot, pair) in row.iter().enumerate() {
                        let Some((base, item)) = pair else { continue };
                        let item_on = object_variants
                            .iter()
                            .find(|v| v.0 == mesh_inst && v.1 == slot)
                            .map(|v| {
                                v.4.trim()
                                    .parse::<f32>()
                                    .ok()
                                    .or_else(|| inst.var(&v.4))
                                    .is_some_and(change_picks_item)
                            })
                            .unwrap_or(false);
                        renderer.set_material(
                            scene,
                            mesh_inst,
                            slot,
                            if item_on { *item } else { *base },
                        );
                    }
                }
            }
        }
        // `[alphascale]` on the object's own slots (#1299: only the traffic
        // lights' were read, a bus stop sign faded by its script stood there
        // whole): as its script's {init} leaves the variables, and then
        // every frame for a script that changes them
        let alpha_slots: Vec<LampSlots> = if lamp.is_none() {
            all_instances
                .iter()
                .take(mesh_instances)
                .enumerate()
                .map(|(mi, &id)| {
                    let count = scene
                        .instances
                        .get(id)
                        .map(|i| i.materials.len())
                        .unwrap_or(0);
                    let mut l = ot
                        .meshes
                        .get(mi)
                        .map(|(_, o3d_mats, overrides)| {
                            LampSlots::of_mesh(o3d_mats, overrides, count)
                        })
                        .unwrap_or_default();
                    l.light.clear();
                    l
                })
                .collect()
        } else {
            Vec::new()
        };
        let faded = alpha_slots
            .iter()
            .any(|l| !l.alpha.is_empty() || !l.uv.is_empty());
        let mut alpha_last = Vec::new();
        if faded {
            for (l, &id) in alpha_slots.iter().zip(&all_instances) {
                let value = |v: &str| v.trim().parse::<f32>().ok().or_else(|| inst.var(v));
                let (a, uv) = (l.values(&value).0, l.uv_values(&value));
                let visible = scene.instances[id].visible;
                renderer.set_params(scene, id, &a, visible, &uv);
                alpha_last.push((a, uv));
            }
        }
        let script_lamps = if lamp.is_none() {
            crate::place::lamps_switched_by_script(ot, pos, xf)
        } else {
            Vec::new()
        };
        // (a Lua script drives it: it is stepped every frame and kept for them)
        inst.lua_driven = !ot.sco.scripts.lua.is_empty();
        if inst.is_dynamic()
            || !script_lamps.is_empty()
            || !object_variants.is_empty()
            || ot.sco.sound.is_some()
            || !script_texts.is_empty()
            || !html_pages.is_empty()
            || !ot.dynamic_textures.is_empty()
            || ot.has_mouse_events
        {
            let arrivals = inst.wants_arrivals();
            // (a scripted object with [terrainmapping] slots had more instances
            // than its script has meshes: "index out of bounds", #111)
            all_instances.truncate(mesh_instances);
            self.scripted.lock().push(ScriptedObject {
                ty: ot.clone(),
                pos,
                xf,
                instances: all_instances,
                inst,
                controller,
                light_index: 0,
                light_parent: if controller.is_none() && lamp.is_none() {
                    light_child_of(&self.index().traffic_light_parents, var_parent, strings)
                } else {
                    None
                },
                map_id,
                variants: object_variants,
                sounds: None,
                tile: key,
                var_parent,
                texts: script_texts,
                arrivals,
                htmls: html_pages,
                alpha_slots: if faded { alpha_slots } else { Vec::new() },
                alpha_last,
                script_lamps,
            });
        }
    }
}
