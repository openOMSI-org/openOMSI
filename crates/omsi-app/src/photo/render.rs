//! The photo's pictures: a quick one while the camera moves, and once it stands the pictures
//! of the shot one after another (as many as fit a frame's time), added up and developed -
//! what is shown is what is saved. "Take photo" waits for the last of them, at the size
//! asked for, and writes the PNG.

use std::time::Instant;

use super::develop::{self, Accum, Grade};
use super::lens::{self, Shot};
use super::{exposure, Photo, ASPECTS, BLADES, BRACKETS, QUALITY, SIZES};
use omsi_photo::optics;

/// Where the photo stands.
#[derive(Default)]
pub(crate) struct State {
    accum: Option<Accum>,
    /// What the pictures added up were taken with: another camera, lens or light starts the
    /// shot again.
    key: Option<String>,
    /// The pictures wanted for the shot as it stands.
    target: u32,
    /// "Take photo" was asked for: saved once the shot is complete.
    pub capture: bool,
    /// The developed picture is out of date (a darkroom setting changed).
    grade: Option<super::Grade>,
    crop: Option<(u32, u32, u32, u32)>,
    /// The last developed picture (w, h, RGBA).
    pub developed: Option<(u32, u32, Vec<u8>)>,
    /// Since when the camera stands still (None: it moves). While it moves the window shows
    /// the live picture at the game's own frame rate; once it has stood a moment the photo's
    /// pictures are taken.
    still_since: Option<Instant>,
    /// The developed picture is of the camera as it stands now (else the live picture shows).
    ready: bool,
    /// The distance along the view of what each pixel of the shot shows (m; its first
    /// picture's, the size of the shot's pictures).
    pub depth: Option<(u32, u32, Vec<f32>)>,
    /// How much the metering mode moves the reading from the whole picture's (stops).
    pub meter_shift: f32,
    /// The developed picture's histogram.
    pub hist: Option<[[u32; 64]; 4]>,
    /// The picture the window shows: the developed one with the live view's aids over it
    /// (None: the developed one itself).
    pub shown: Option<Vec<u8>>,
    /// What the overlays were drawn with (zebra, focus peaking).
    aids: (bool, bool),
}

/// How long the camera stands before the photo is taken over the live picture.
const SETTLE_SECS: f32 = 0.2;

impl State {
    pub(crate) fn reset(&mut self) {
        self.key = None;
    }

    /// The distance of what the picture shows at (x, y) (shares of it): the middle of the
    /// distances round it, the edges of things not taken for them.
    pub(crate) fn depth_at(&self, x: f32, y: f32) -> Option<f32> {
        let (w, h, d) = self.depth.as_ref()?;
        let (cx, cy) = ((x * *w as f32) as i64, (y * *h as f32) as i64);
        let mut v: Vec<f32> = Vec::new();
        for dy in -3..=3 {
            for dx in -3..=3 {
                let (px, py) = ((cx + dx * 2).clamp(0, *w as i64 - 1), (cy + dy * 2).clamp(0, *h as i64 - 1));
                v.push(d[(py * *w as i64 + px) as usize]);
            }
        }
        v.sort_by(f32::total_cmp);
        let m = v[v.len() / 2];
        (m.is_finite() && m > 0.0 && m < 1e5).then_some(m)
    }

    /// How far the shot is (pictures taken, wanted).
    pub(crate) fn progress(&self) -> (u32, u32) {
        (self.accum.as_ref().map(|a| a.n).unwrap_or(0), self.target)
    }

    /// The window shows the developed photo (else the live picture): the camera stands and
    /// the photo of it is there.
    pub(crate) fn showing_photo(&self) -> bool {
        (self.ready && !self.moving()) || (self.capture && self.ready)
    }

    /// The camera moves (or has only just stopped): the window shows the live picture.
    pub(crate) fn moving(&self) -> bool {
        !self.capture && self.still_since.is_none_or(|t| t.elapsed().as_secs_f32() < SETTLE_SECS)
    }
}

impl Photo {
    /// How the shot is taken now.
    pub(crate) fn shot(&self) -> Shot {
        let s = &self.settings;
        let sensor_h = s.sensor_h(self.pro);
        Shot {
            dof: s.dof,
            fstop: self.fstop(),
            focus: self.focus_distance(),
            blades: BLADES[s.blades.min(BLADES.len() - 1)].0,
            // (the bus is moved for a second at most: longer, its copies stood apart)
            motion: (s.motion && self.bus_velocity.length() > 0.2).then(|| (self.bus_velocity, self.shutter_time().min(1.0))),
            tripod: s.tripod,
            sensor_h,
            tilt: if self.pro { s.tilt } else { [0.0; 2] },
            shift: if self.pro { [s.shift[0] / sensor_h, s.shift[1] / sensor_h] } else { [0.0; 2] },
            highlights: if s.dof { s.highlights } else { 0.0 },
        }
    }

    /// The darkroom's settings with the camera's laid in: the advanced mode's exposure, its
    /// noise, white balance, picture profile, the lens as it comes and its diffraction.
    pub(crate) fn grade(&self, h_px: u32) -> Grade {
        let s = &self.settings;
        let mut g = s.grade;
        if !self.pro {
            return g;
        }
        let p = exposure::PROFILES[s.profile.min(exposure::PROFILES.len() - 1)];
        g.contrast = (g.contrast + p.1).clamp(-1.0, 1.0);
        g.saturation = (g.saturation + p.2).clamp(-1.0, 1.0);
        g.temperature = (g.temperature + p.3).clamp(-1.0, 1.0);
        let wb = exposure::WHITE_BALANCE[s.wb.min(exposure::WHITE_BALANCE.len() - 1)].1;
        g.kelvin = if wb < 0.0 { s.kelvin } else { wb };
        if let Some(m) = self.metered {
            g.camera_ev = m.offset;
            g.iso = if s.noise { m.iso } else { 0.0 };
            if g.camera_ev == 0.0 && g.iso == 0.0 {
                // (the sensor's white all the same)
                g.camera_ev = 1e-6;
            }
        }
        let fstop = self.fstop();
        if s.lens_raw {
            let (k1, vig) = exposure::lens_character(lens::focal_mm(s.fov), fstop);
            g.distortion += k1;
            g.lens_vignette = vig;
            g.aberration = (g.aberration + 0.15 * (35.0 / lens::focal_mm(s.fov)).min(1.5)).min(1.0);
        }
        g.diffraction = exposure::diffraction_px(fstop, s.sensor_h(true), h_px as f32);
        g
    }

    /// The frame cut by the chosen aspect ratio.
    pub(crate) fn aspect(&self) -> Option<f32> {
        ASPECTS[self.settings.aspect.min(ASPECTS.len() - 1)].0
    }
}

impl crate::App {
    /// The photo mode's pictures for this frame, developed and handed to the screens.
    pub(crate) fn photo_render(&mut self, lighting: &omsi_render::Lighting) {
        let (Some(ph), Some(s), Some(r), Some(scene)) = (self.photo.as_mut(), self.gfx.surface.as_ref(), self.renderer.as_mut(), self.scene.as_mut()) else { return };
        let (ww, wh) = (s.config.width, s.config.height);
        // the light meter: the scene's light, and where the metering mode looks
        let luma = |c: glam::Vec3| 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
        let sun = lighting.sun_intensity * luma(lighting.sun_color) * lighting.sun_dir.z.max(0.0);
        let sky = luma(lighting.ambient);
        ph.scene_ev = exposure::scene_ev(sun, sky, lighting.night) + ph.render.meter_shift;
        ph.meter_now();
        let shot = ph.shot();
        let cam = ph.cam;
        // what the shot is: the camera to the millimetre and the tenth of a degree, the lens,
        // the clock (the light), the window
        let key = format!(
            "{:.3},{:.3},{:.3},{:.2},{:.2},{:.2},{:.3}|{:?}|{}|{}x{}",
            cam.position.x, cam.position.y, cam.position.z, cam.yaw, cam.pitch, cam.roll, cam.fov_deg, shot, self.clock.time as i64, ww, wh
        );
        let changed = ph.render.key.as_deref() != Some(key.as_str());
        let camera_moved = ph.render.key.as_ref().is_none_or(|k| k.split('|').next() != key.split('|').next());
        if camera_moved {
            ph.render.still_since = None;
        }
        if ph.render.still_since.is_none() && !camera_moved {
            ph.render.still_since = Some(Instant::now());
        }
        if camera_moved {
            // (the live picture shows: nothing is taken, the shot starts again once it stands)
            ph.render.key = Some(key);
            ph.render.accum = None;
            ph.render.target = 1;
            ph.render.ready = false;
            return;
        }
        if ph.render.moving() {
            return;
        }
        let samples = if shot.dof || shot.motion.is_some() { QUALITY[ph.settings.quality.min(QUALITY.len() - 1)].0 } else { 1 };
        let sx = ((shot.shift[0] * wh as f32).abs().ceil() as u32, (shot.shift[1] * wh as f32).abs().ceil() as u32);
        // (the size: the window's, the saved photo's while one is being taken)
        let scale = if ph.render.capture { SIZES[ph.settings.size.min(SIZES.len() - 1)].0 } else { 1.0 };
        let max = r.device.limits().max_texture_dimension_2d.min(8192) as f32;
        let k = (scale).min(max / ww.max(1) as f32).min(max / wh.max(1) as f32);
        let (w, h) = (((ww as f32 * k).round() as u32).max(1), ((wh as f32 * k).round() as u32).max(1));
        let target = samples;
        if changed || ph.render.accum.as_ref().is_none_or(|a| a.w != w || a.h != h) {
            ph.render.accum = Some(Accum::new(w, h));
            ph.render.key = Some(key);
            ph.render.depth = None;
            if ph.pro {
                log::info!("photo mode: the light EV {:.1} (sun {sun:.3}, sky {sky:.3}, night {:.2}, metering {:+.1}), taken at {:?}", ph.scene_ev, lighting.night, ph.render.meter_shift, ph.metered);
            }
        }
        let _ = sx;
        ph.render.target = target;
        let aspect = ph.aspect();
        let Some(acc) = ph.render.accum.as_mut() else { return };
        let mut added = false;
        if acc.n < target {
            // Keep the bus's occupants and loose cabin objects in its shutter frame (#2611).
            let parts: Vec<(usize, glam::DVec3, glam::Mat4)> = match (shot.motion.is_some(), self.player.as_ref()) {
                (true, Some(p)) => {
                    let trailers: Vec<usize> = p.trailer_renders.iter().flat_map(|t| t.instances.iter().copied()).collect();
                    let people = &self.gfx.sim_view.people.people;
                    let mut riders: Vec<usize> = self.session.humans.iter().flat_map(|h| h.people.iter()).filter(|q| q.inside(crate::humans::BusId::Player)).filter_map(|q| people.get(&q.id)).flat_map(|v| v.meshes.iter().map(|m| m.1)).collect();
                    riders.extend(p.driver.iter().flat_map(|d| d.instances()));
                    riders.extend(self.session.humans.iter().flat_map(|h| h.sim.money.iter()).flat_map(|m| m.placed.iter().map(|coin| coin.0)));
                    riders.extend(self.gfx.sim_view.people.ticket_blocks.iter().flat_map(|b| b.instances()));
                    omsi_photo::motion_parts(&p.render.instances, &trailers, &riders).into_iter().filter_map(|i| scene.instances.get(i).map(|x| (i, x.origin, x.transform))).collect()
                }
                _ => Vec::new(),
            };
            // and its wheels, turning as far as they roll in that time
            let spin: Vec<Option<(glam::Mat4, glam::Vec3, f32)>> = match (shot.motion.is_some(), self.player.as_ref()) {
                (true, Some(p)) => wheel_spin(&p.vehicle, p.render.instances.len()),
                _ => Vec::new(),
            };
            let speed = shot.motion.map(|m| m.0.length()).unwrap_or(0.0);
            let t0 = Instant::now();
            // as many pictures as fit some 40 ms (one at least)
            let tan = (cam.fov_deg.to_radians() * 0.5).tan();
            let pix = 2.0 * tan / h as f32;
            let radius_m = lens::aperture_radius(cam.fov_deg, &shot);
            let gap = optics::gap(target);
            // (a shifted lens: a wider view of the same pixels, cut where the lens moved)
            let (ex, ey) = ((shot.shift[0] * h as f32).round() as i64, (shot.shift[1] * h as f32).round() as i64);
            let (rw, rh) = (w + 2 * ex.unsigned_abs() as u32, h + 2 * ey.unsigned_abs() as u32);
            let (ox, oy) = ((rw - w) as i64 / 2 + ex, (rh - h) as i64 / 2 - ey);
            let (su, sv) = (shot.shift[0] * 2.0 * tan, shot.shift[1] * 2.0 * tan);
            let tilted = shot.dof && (shot.tilt[0] != 0.0 || shot.tilt[1] != 0.0);
            let focal_m = lens::focal_on(cam.fov_deg, shot.sensor_h) / 1000.0;
            while acc.n < target && (acc.n == 0 || t0.elapsed().as_secs_f32() < 0.040 || ph.render.capture) {
                let smp = lens::sample(&cam, acc.n, &shot, h as f32);
                let (mut c, t) = (smp.cam, smp.t);
                // (the camera travels with the bus: the bus stays sharp, the street passes;
                // on a tripod it stays and the bus passes)
                let d = lens::travel(&shot, t);
                if !shot.tripod {
                    c.position += d;
                }
                if (rw, rh) != (w, h) {
                    c.fov_deg = (2.0 * (tan * rh as f32 / h as f32).atan()).to_degrees();
                }
                for (k, (i, o, tr)) in parts.iter().enumerate() {
                    let tr = match spin.get(k).copied().flatten() {
                        // (about the axle through the wheel's centre, in the body's frame)
                        Some((body, centre, radius)) => {
                            let turn = glam::Mat4::from_translation(centre) * glam::Mat4::from_rotation_x(speed * t / radius.max(0.1)) * glam::Mat4::from_translation(-centre);
                            body * turn * body.inverse() * *tr
                        }
                        None => *tr,
                    };
                    r.set_transform(scene, *i, *o + d, tr);
                }
                let wants_depth = shot.dof || acc.n == 0;
                let pic = if wants_depth { r.render_to_image_with_depth(scene, rw, rh, &c, lighting) } else { r.render_to_image(scene, rw, rh, &c, lighting).map(|px| (px, None)) };
                match pic {
                    Ok((px, depth)) => {
                        // cut to the frame
                        let (px, depth) = if (rw, rh) != (w, h) {
                            let mut out = Vec::with_capacity((w * h * 4) as usize);
                            let mut dout = depth.as_ref().map(|_| Vec::with_capacity((w * h) as usize));
                            for y in 0..h as i64 {
                                let row = (y + oy).clamp(0, rh as i64 - 1) as usize;
                                let a = row * rw as usize + ox.clamp(0, (rw - w) as i64) as usize;
                                out.extend_from_slice(&px[a * 4..(a + w as usize) * 4]);
                                if let (Some(dd), Some(src)) = (dout.as_mut(), depth.as_ref()) {
                                    dd.extend_from_slice(&src[a..a + w as usize]);
                                }
                            }
                            (out, dout)
                        } else {
                            (px, depth)
                        };
                        // the direction (tangents: right, up) of a pixel
                        let uv = |x: f32, y: f32| (((x + 0.5) - w as f32 * 0.5) * pix + su, (h as f32 * 0.5 - (y + 0.5)) * pix + sv);
                        // a tilted lens: the picture laid onto the leaning plane of focus
                        let (px, depth) = match depth {
                            Some(dp) if tilted && smp.aperture != glam::Vec2::ZERO => {
                                let a = smp.aperture;
                                let s = shot.focus.max(0.05);
                                let (p2, d2) = optics::warp(&px, &dp, w, h, |x, y| {
                                    let (u, v) = uv(x, y);
                                    // (how far the leaning plane is from the upright one, in 1/m)
                                    let k = lens::inv_focus_at(&shot, focal_m, u, v) - 1.0 / s;
                                    (a.x * k / pix, -a.y * k / pix)
                                });
                                (p2, Some(d2))
                            }
                            other => (px, other),
                        };
                        let mut lin = optics::linear(&px, w, h, shot.highlights);
                        if let (true, Some(dp)) = (shot.dof && target > 1, depth.as_ref()) {
                            let radius: Vec<f32> = (0..(w * h) as usize).map(|i| {
                                let (u, v) = uv((i % w as usize) as f32, (i / w as usize) as f32);
                                lens::coc_px(&shot, radius_m, focal_m, dp[i], u, v, pix) * gap
                            }).collect();
                            optics::soften(&mut lin, w, h, &radius);
                        }
                        if acc.n == 0 {
                            ph.render.depth = depth.map(|d| (w, h, d));
                        }
                        acc.add_linear(&lin);
                        if acc.n == 1 {
                            ph.render.meter_shift = exposure::metering_shift(ph.settings.exposure.metering, &acc.luma(), w, h);
                        }
                    }
                    Err(e) => {
                        log::warn!("photo mode: the picture could not be rendered: {e}");
                        break;
                    }
                }
                added = true;
                // (a photo being taken goes on in the next frame: the window stays alive)
                if ph.render.capture && t0.elapsed().as_secs_f32() > 0.25 {
                    break;
                }
            }
            for (i, o, tr) in &parts {
                r.set_transform(scene, *i, *o, *tr);
            }
        }
        let crop = Some(develop::crop_rect(w, h, aspect)).filter(|c| (c.2, c.3) != (w, h));
        let grade = ph.grade(h);
        let aids = (ph.pro && ph.settings.zebra, ph.pro && ph.settings.peaking);
        let Some(acc) = ph.render.accum.as_ref() else { return };
        if added || ph.render.grade != Some(grade) || ph.render.crop != crop || ph.render.aids != aids {
            let px = develop::develop(acc, &grade, crop);
            ph.render.grade = Some(grade);
            ph.render.crop = crop;
            ph.render.aids = aids;
            ph.render.hist = Some(develop::histogram(&px, w, crop.unwrap_or((0, 0, w, h))));
            // the live view's aids: over the picture shown, not the one saved
            let shown = if (aids.0 || aids.1) && !ph.render.capture {
                let mut v = px.clone();
                let focus: Option<Vec<f32>> = match (aids.1, ph.render.depth.as_ref()) {
                    (true, Some((dw, dh, d))) if (*dw, *dh) == (w, h) => {
                        let tan = (cam.fov_deg.to_radians() * 0.5).tan();
                        let pix = 2.0 * tan / h as f32;
                        let peak = Shot { dof: true, ..shot };
                        let rm = lens::aperture_radius(cam.fov_deg, &peak);
                        let focal_m = lens::focal_on(cam.fov_deg, shot.sensor_h) / 1000.0;
                        let (su, sv) = (shot.shift[0] * 2.0 * tan, shot.shift[1] * 2.0 * tan);
                        Some((0..(w * h) as usize).map(|i| {
                            let (x, y) = ((i % w as usize) as f32, (i / w as usize) as f32);
                            let (u, v) = (((x + 0.5) - w as f32 * 0.5) * pix + su, (h as f32 * 0.5 - (y + 0.5)) * pix + sv);
                            lens::coc_px(&peak, rm, focal_m, d[i], u, v, pix)
                        }).collect())
                    }
                    _ => None,
                };
                develop::overlays(&mut v, w, h, aids.0, focus.as_deref());
                v
            } else {
                px.clone()
            };
            ph.render.shown = (aids.0 || aids.1).then(|| shown.clone());
            self.shell.set_picture(w, h, shown);
            ph.render.developed = Some((w, h, px));
            ph.render.ready = true;
        }
        let Some(acc) = ph.render.accum.as_ref() else { return };
        if ph.render.capture && acc.n >= target {
            ph.render.capture = false;
            log::info!("photo mode: {} pictures of {w}x{h} in the photo", acc.n);
            let meta = ph.metadata();
            let shots = BRACKETS[ph.settings.bracket.min(BRACKETS.len() - 1)].0;
            let step = ph.settings.bracket_step;
            let mut note = Ok(std::path::PathBuf::new());
            for k in 0..shots {
                let ev = (k as f32 - (shots - 1) as f32 * 0.5) * step;
                let g = Grade { exposure: grade.exposure + ev, ..grade };
                let suffix = if shots > 1 { format!("_{}{:.1}EV", if ev >= 0.0 { "+" } else { "" }, ev) } else { String::new() };
                let r = if ph.settings.png16 && ph.pro {
                    save16(&self.args.root, w, h, &develop::develop16(acc, &g, crop), crop, &meta, &suffix)
                } else if shots == 1 {
                    match ph.render.developed.as_ref() {
                        Some((w, h, px)) => save(&self.args.root, *w, *h, px, crop, &meta, &suffix),
                        None => Err("nothing to save".into()),
                    }
                } else {
                    save(&self.args.root, w, h, &develop::develop(acc, &g, crop), crop, &meta, &suffix)
                };
                // (the middle photo of a bracket is the one shown)
                if k == shots / 2 || r.is_err() {
                    note = r;
                }
            }
            ph.saved = None;
            ph.note = Some(match note {
                Ok(path) => {
                    log::info!("photo mode: saved {}", path.display());
                    crate::plugins::queue_event(&mut self.integrations.plugin_events, "screenshot", vec![omsi_plugin::InfoValue::Text(path.to_string_lossy().into_owned())]);
                    crate::platform::to_gallery(&path);
                    ph.saved = Some(path);
                    (omsi_ui::tr(if crate::platform::MOBILE { "Photo saved to the gallery" } else { "Photo saved" }).into_owned(), 8.0)
                }
                Err(e) => {
                    log::warn!("photo mode: the photo could not be saved: {e}");
                    (format!("{} {}", omsi_ui::tr("Not saved:"), omsi_ui::tr(&e.to_string())), 6.0)
                }
            });
        }
    }
}

/// For each mesh of the vehicle (in the order of its render's instances): a wheel's body
/// rotation, its centre in the body's frame and its radius - the meshes a `Wheel_Rotation_*`
/// variable turns, the tallest of each giving the centre - else None.
fn wheel_spin(v: &omsi_sim::VehicleInstance, n: usize) -> Vec<Option<(glam::Mat4, glam::Vec3, f32)>> {
    let ty = &v.ty;
    let body = v.body_rotation();
    let inv = body.inverse();
    // (by variable: the centre and half height of its tallest mesh, at rest)
    let mut by_var: std::collections::HashMap<String, (usize, glam::Vec3, f32)> = std::collections::HashMap::new();
    let mut var_of: Vec<Option<String>> = vec![None; n];
    for (i, vm) in ty.meshes.iter().enumerate().take(n) {
        let def = &ty.model.meshes[vm.def_index];
        let Some(an) = def.animations.iter().find(|a| a.variable.to_ascii_lowercase().starts_with("wheel_rotation_")) else { continue };
        let Some(data) = ty.mesh_data(i) else { continue };
        if data.positions.is_empty() {
            continue;
        }
        let (lo, hi) = data.positions.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let half = (hi.z - lo.z) * 0.5;
        let key = an.variable.to_ascii_lowercase();
        var_of[i] = Some(key.clone());
        let e = by_var.entry(key).or_insert((i, (lo + hi) * 0.5, half));
        if half > e.2 {
            *e = (i, (lo + hi) * 0.5, half);
        }
    }
    var_of
        .iter()
        .map(|var| {
            let (i, c, r) = *by_var.get(var.as_ref()?)?;
            // (the centre where the wheel is now: in the body's frame)
            let centre = (inv * v.mesh_local_transform(i)).transform_point3(c);
            Some((body, centre, r))
        })
        .collect()
}

impl Photo {
    /// The camera's settings as a photo's metadata: the XMP packet (EXIF's names, as photo
    /// programs read them) written into the PNG.
    pub(crate) fn metadata(&self) -> String {
        let s = &self.settings;
        let real = s.focal(self.pro);
        let eq = lens::focal_mm(s.fov);
        let t = self.shutter_time();
        let exposure_time = if t >= 1.0 { format!("{t:.1}") } else { format!("1/{:.0}", 1.0 / t) };
        let iso = self.metered.map(|m| m.iso).unwrap_or(100.0);
        let model = if self.pro { exposure::SENSORS[s.sensor.min(exposure::SENSORS.len() - 1)].0 } else { "Photo mode" };
        format!(
            "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:tiff=\"http://ns.adobe.com/tiff/1.0/\" xmlns:exif=\"http://ns.adobe.com/exif/1.0/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" tiff:Make=\"openOMSI\" tiff:Model=\"{model}\" xmp:CreatorTool=\"openOMSI {ver}\" exif:FNumber=\"{f}\" exif:ExposureTime=\"{exposure_time}\" exif:FocalLength=\"{real:.1}\" exif:FocalLengthIn35mmFilm=\"{eq:.0}\" exif:SubjectDistance=\"{focus:.2}\"><exif:ISOSpeedRatings><rdf:Seq><rdf:li>{iso:.0}</rdf:li></rdf:Seq></exif:ISOSpeedRatings></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
            ver = crate::updater::current_version(),
            f = self.fstop(),
            focus = self.focus_distance(),
        )
    }
}

/// A free path for a photo in the content folder's `Screenshots`, named by the date and time.
fn photo_path(root: &std::path::Path, suffix: &str) -> Result<std::path::PathBuf, String> {
    let dir = crate::startup::content_dir().unwrap_or_else(|| root.to_path_buf()).join("Screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = chrono_like_stamp();
    let mut path = dir.join(format!("openOMSI_photo_{stamp}{suffix}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("openOMSI_photo_{stamp}_{n}{suffix}.png"));
        n += 1;
    }
    Ok(path)
}

/// A PNG with the camera's metadata (8 or 16 bits a channel, RGBA; 16 bit samples big-endian).
fn write_png(path: &std::path::Path, w: u32, h: u32, depth16: bool, data: &[u8], meta: &str) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(if depth16 { png::BitDepth::Sixteen } else { png::BitDepth::Eight });
    enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    enc.add_itxt_chunk("XML:com.adobe.xmp".into(), meta.into()).map_err(|e| e.to_string())?;
    enc.add_text_chunk("Software".into(), format!("openOMSI {}", crate::updater::current_version())).map_err(|e| e.to_string())?;
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(data).map_err(|e| e.to_string())?;
    wr.finish().map_err(|e| e.to_string())
}

/// The photo written as a PNG into the content folder's `Screenshots`; cut to its frame.
fn save(root: &std::path::Path, w: u32, h: u32, px: &[u8], crop: Option<(u32, u32, u32, u32)>, meta: &str, suffix: &str) -> Result<std::path::PathBuf, String> {
    let path = photo_path(root, suffix)?;
    let (cw, ch, data) = match crop {
        Some(c) => (c.2, c.3, develop::cropped(px, w, c)),
        None => (w, h, px.to_vec()),
    };
    write_png(&path, cw, ch, false, &data, meta)?;
    Ok(path)
}

/// The photo with 16 bits a channel.
fn save16(root: &std::path::Path, w: u32, h: u32, px: &[u16], crop: Option<(u32, u32, u32, u32)>, meta: &str, suffix: &str) -> Result<std::path::PathBuf, String> {
    let path = photo_path(root, suffix)?;
    let (x0, y0, cw, ch) = crop.unwrap_or((0, 0, w, h));
    let mut data = Vec::with_capacity((cw * ch * 8) as usize);
    for y in y0..y0 + ch {
        for v in &px[((y * w + x0) * 4) as usize..((y * w + x0 + cw) * 4) as usize] {
            data.extend_from_slice(&v.to_be_bytes());
        }
    }
    write_png(&path, cw, ch, true, &data, meta)?;
    Ok(path)
}

/// The local date and time as `YYYY-MM-DD_HH-MM-SS` (the seconds since 1970 where the
/// device's calendar cannot be read).
fn chrono_like_stamp() -> String {
    match crate::real_time::now() {
        Some(n) => {
            let s = n.secs as i64;
            format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", n.year, n.month, n.day, s / 3600, s % 3600 / 60, s % 60)
        }
        None => {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            format!("{secs:019}")
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stamp_reads_as_a_date_and_time() {
        let s = super::chrono_like_stamp();
        assert_eq!(s.len(), 19, "{s}");
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[10..11], "_");
    }
}
