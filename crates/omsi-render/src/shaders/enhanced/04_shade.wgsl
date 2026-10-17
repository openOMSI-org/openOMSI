fn shade_enhanced(in: FsIn, puddle_weight: ptr<function, vec2<f32>>, capture: bool, eye: vec3<f32>) -> vec4<f32> {
    // (the pane's water, read once for the uses below)
    let film_water = window_wetness(in);
    if (material.emissive.w > 1.5) {
        // a pane's film of water: drops, not the sliding texture (see `rain_glass`), each a
        // lens that mirrors the sky probe and shows it upside down through itself
        let v = camera.cam_pos.xyz - in.world;
        let vn = normalize(v);
        let in_cab = inside_vehicle(camera.cam_pos.xyz) * near_player_vehicle(in.world) > 0.5;
        let g = rain_glass(in.world, in.uv - in.params.zw, in.normal, film_water, camera.post.y, in_cab, in.wipe_uv);
        if (g.cover <= 0.001 && g.mist <= 0.001) { return vec4<f32>(0.0); }
        let through = rain_through(g, vn);
        let valid = dot(through, through) > 1e-4;
        // (the picture behind is as the HDR pass drew it: exposed already)
        let seen = select(vec3<f32>(0.0), rain_behind(in.world, through, rain_env_enhanced(normalize(select(g.out, through, valid)), 2.0), 1.0 / max(enh.exposure.x, 1e-6), g.mist), valid);
        let mirrored = rain_env_enhanced(reflect(-vn, g.n), 1.0);
        let d = rain_light(g, vn, through, mirrored, seen, sh_irradiance(g.out) / PI * 0.9, enh.sun.rgb / PI);
        let aer = air(-normalize(v), fog_distance(in.world), camera.cam_pos.z - enh.fog.z, in.world.z - enh.fog.z);
        // Drops a few pixels across are a pane's sparkle close up; further off each
        // darker rim was a black fleck, and a bus seen from the pavement in the rain wore
        // windows peppered black. Drops smaller than a pixel give way to the mist now
        // (`rain_dome`), and the rest fade out over the first fifteen metres.
        let near = 1.0 - smoothstep(5.0, 15.0, length(v));
        return vec4<f32>(d.rgb * enh.exposure.x * aer.a, d.a * near);
    }
    // --- the surface's texture and alpha, exactly as the vanilla pass reads them
    let terrain = material.extra.x > 0.5;
    var duv = tex_address(in.uv);
    if (terrain) {
        duv = in.uv * material.extra.z;
    }
    // (without the [texcoordtransX/Y] offset: the transmap, night map and light map stay
    // in place, see fs_main)
    let buv = tex_address(in.uv - in.params.zw);
    let msk_lod = led_lod(buv, vec2<f32>(textureDimensions(t_trans)));
    // Empty paint also needs no tiled diffuse or detail samples. Its brush mask
    // replaces diffuse alpha, so coverage can be checked before those reads.
    if (TERRAIN_PAINT && material.params.x > 1.5 && material.params.z > 0.5
        && material.params.w > 0.5 && enh.debug.x <= 0.5 && material.ambient.w <= 1.5) {
        var coverage = sample_transmap(buv).a;
        if (material.emissive.w < -1.5 && enh.led.y < msk_lod) {
            coverage = textureSampleLevel(t_trans, s_diffuse, buv, enh.led.y).a;
        }
        coverage = smoothstep(0.32, 0.68, coverage);
        if (coverage * material.color.a * in.params.x == 0.0) {
            discard;
        }
    }
    // An LED panel is sampled at the level its screen footprint asks for, held at
    // `enh.led.y` (`Led mip strength`): its dots keep their gaps much further out than the
    // full chain allows, and the shimmer is a fraction of a full-resolution sample's. The
    // levels are worked out here and not inside the branch: derivatives are undefined in
    // non-uniform control flow, and which of the two samples runs is per draw. When the
    // setting does not bite, the plain (anisotropic) sample of the hardware is the better
    // one and stays.
    let pic_lod = led_lod(duv, vec2<f32>(textureDimensions(t_diffuse)));
    let led_pic = material.emissive.w < -1.5 && enh.led.y < pic_lod;
    var tex = diffuse_border(textureSample(t_diffuse, s_diffuse, duv), duv);
    if (is_snow_pane()) {
        tex = snow_on_pane(in, film_water);
    }
    if (led_pic) {
        tex = diffuse_border(textureSampleLevel(t_diffuse, s_diffuse, duv, enh.led.y), duv);
    }
    let diffuse_a = tex.a;
    if (terrain && material.extra.y > 0.0) {
        let det = textureSample(t_light, s_diffuse, in.uv * material.extra.y);
        tex = vec4<f32>(clamp(tex.rgb * det.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), tex.a);
    }
    if (material.params.z > 0.5) {
        // (an LED panel's `\S:n` mask is taken the same way: the dots stay dots when the
        // panel is small, without the full-resolution shimmer)
        var tm = sample_transmap(buv);
        if (material.emissive.w < -1.5 && enh.led.y < msk_lod) {
            tm = textureSampleLevel(t_trans, s_diffuse, buv, enh.led.y);
        }
        tex.a = select(1.0, tm.a, material.params.w > 0.5);
        if (terrain && material.params.x > 1.5) {
            // Coverage belongs to the brush mask, not the angle-dependent diffuse mip.
            tex.a = smoothstep(0.32, 0.68, tex.a);
        }
    }
    let mode = material.params.x;
    // Keep the filtered fractional coverage, but tighten its transition around the cutout
    // edge before MSAA turns it into sample coverage. The MSAA depth prepass skips these
    // draws so uncovered samples keep the depth and colour of the scene behind them.
    if ((ALPHA_TEST || capture) && mode > 0.5 && mode < 1.5) {
        if (ALPHA_TO_COVERAGE) {
            let aa = max(fwidth(tex.a) * 0.5, 1.0 / 255.0);
            // (not `ALPHA_REF - aa`: a layer whose clear glass is alpha 128 kept a third of
            // its samples there, a moire of the layer's paint over every window)
            if (tex.a < ALPHA_REF) {
                discard;
            }
            tex.a = smoothstep(ALPHA_REF - aa, ALPHA_REF + aa, tex.a);
        } else if (tex.a < ALPHA_REF) {
            discard;
        }
    }
    // (a blended slot that keeps Omsi.exe's alpha test, `MaterialExtra::alpha_cut`: a
    // livery's clear glass is cut away, its tinted glass blended)
    tex.a = select(tex.a, 0.0, mode > 2.5 && tex.a < ALPHA_REF);
    var alpha = tex.a * material.color.a;
    if (mode < 0.5) {
        alpha = 1.0;
    }
    if (!is_snow_pane()) {
        alpha = alpha * clamp(film_water, 0.0, 1.0);
    }
    // Sparse brush masks still cover the whole tile mesh. Empty pixels contribute
    // neither colour nor reflection coverage, so avoid lighting them. Keep fractional
    // edges, debug views, and water (whose Fresnel can raise zero alpha) unchanged.
    if (TERRAIN_PAINT && alpha == 0.0 && enh.debug.x <= 0.5 && material.ambient.w <= 1.5) {
        discard;
    }
    let pre = enh.exposure.x;
    let to_cam = eye - in.world;
    let dist = length(to_cam);
    let v = to_cam / max(dist, 1e-4);
    let h_cam = camera.cam_pos.z - enh.fog.z;
    let h_pt = in.world.z - enh.fog.z;
    // (no fog inside the cab: only the part of the way outside the bus is misty)
    // (the lamps' light in the fog is added over the whole picture afterwards: fog_lamps.wgsl)
    let aer = air(-v, fog_distance(in.world), h_cam, h_pt);
    if (in.params2.w > 1.5) {
        // A vehicle's shadow blob is no surface: it is the sky light the body keeps off the
        // road, a layer that only darkens what lies under it (as OMSI blends it). Shaded as
        // a black surface it took the sky, the wet road's sheen and the lamps on top and
        // hardly darkened anything. The air in front of it stays (the road's own fog, under
        // the blob, is darkened with it as in the original).
        return vec4<f32>(aer.rgb * pre, alpha * aer.a);
    }
    if (material.params.y > 0.5) {
        // unlit (mirror glass, script and text textures): shown at their own brightness,
        // under the tone curve's knee (`display_level`), and `exposure.y` undoes the
        // metering (see ExposureLog in lib.rs)
        let t = tex.rgb * material.color.rgb;
        // A mirror (params.y 0.9) is no display: its picture is the street drawn a moment
        // ago, dark at night. Brightened like a display by the metering (up to 1.6 in the
        // dark) it showed a street far brighter than the one through the windscreen.
        let lift = select(display_dim(enh.exposure.y), min(enh.exposure.y, 1.0), material.params.y < 0.95);
        // (no [matl_glow] here: an unlit slot is drawn at its own brightness already, and
        // reading the light map's slot in this branch failed Metal's shader compiler)
        let c = display_level(t) * lift;
        return vec4<f32>(c * aer.a + aer.rgb * pre, alpha);
    }
    let outside = weather_outside_n(in.world, safe_normal(in.normal), terrain, in.params2.w);
    // the normal as the content gives it: never turned towards the viewer (Direct3D does
    // not either, and OMSI's foliage points every leaf's normal up so the whole crown is lit
    // evenly - turned round, the crown went dark above the horizon line)
    var n = safe_normal(in.normal);
    // the map's water (`MaterialExtra::water`): small waves running over it ripple its
    // normal - fading with the distance, where they would only flicker - so that what it
    // mirrors breaks up as on a lake instead of standing in it as in a pane of glass (#841)
    let is_water = material.ambient.w > 1.5;
    if (is_water) {
        n = normalize(n + vec3<f32>(water_ripple(world_pattern_xy(in.world), camera.post.y) * clamp(1.0 - dist / 250.0, 0.2, 1.0), 0.0));
    }
    // Leaves and fences transmit light. A terrain road-cut mask only removes ground:
    // its remaining pixels must shade like the uncut ground on terrain-mapped splines.
    // (people's cut-out hair and clothes are bodies: material flag 8)
    let thin = !terrain && mode > 0.5 && mode < 1.5 && (u32(material.params2.w + 0.5) & 8u) == 0u;
    let has_env = material.params2.y > 0.0;
    // A blended transmap body is a masked paint surface, not glass. Traffic cars often
    // use this material layout for their body; depth-disabled blends remain glass.
    // Window assets are not consistent about carrying an envmap.  The reliable signal
    // is an explicitly named window, or a depth-disabled blended layer with env/transmap
    // data that identifies a pane rather than a dirt/text overlay. This keeps bus
    // panes on the reflection/transmission path after the material-depth repair.
    // Painted terrain also blends a transmap without writing depth. It is never glass:
    // Fresnel opacity on its empty mask pixels darkens every lower layer at grazing angles.
    let glass = !terrain && mode > 1.5 && (material.bump.z > 0.5 || material.emissive.w > 0.5) &&
        (has_env || material.params.z > 0.5 || material.emissive.w > 0.5);
    // An envmap on opaque vehicle paint is legacy material data, not a request to make
    // the whole body behave like glass. Keep the diffuse/sun response, but use the
    // ordinary rough dielectric path for the body.
    // Foliage and other alpha-tested assets are diffuse silhouettes, not polished surfaces.
    // Letting the generic probe term reflect them creates white sparkles at grazing angles;
    // the photographed envmap is reserved for materials that explicitly request it.
    // (opaque materials reflect as well - chrome handrails, bumpers and wheel trims are
    // opaque with a sphere map, and left out they showed no reflection at all in the
    // enhanced picture, #266 - but through the clear-coat path below: metal only where a
    // mask of its own says so)
    // (not the water: its sphere map is OMSI's stand-in for the sky it mirrors, which the
    // probe holds itself - drawn as paint's photo, it lay on the water as green specks)
    // (a body painted through a [matl_transmap] too - most mod buses' and cars' paint,
    // the W123's: its alpha is the transmap's, the envmap's mask the texture's own, as in
    // the vanilla picture; left out, their paint showed no reflection at all while their
    // windows did)
    let reflective_env = has_env && !thin && !is_water;
    // see-through glass is seen from either side: from inside the bus its normal points
    // away (the vanilla pass takes the angle either way round too); taken as it is, the
    // grazing Fresnel turned the whole windscreen into a milky mirror
    var geo_n = n;
    if (glass && dot(n, v) < 0.0) {
        n = -n;
        geo_n = n;
    }
    var face_n = n;
    var face_v = 0.0;
    var face_w = 1.0;
    if (reflective_env && !glass && !terrain) {
        let c = cross(dpdx(in.world), dpdy(in.world));
        if (dot(c, c) > 1e-14) {
            let face = normalize(c) * select(-1.0, 1.0, dot(c, v) >= 0.0);
            face_n = face;
            face_v = dot(face, v);
            face_w = smoothstep(0.5, 0.9, dot(n, face));
        }
    }
    if (reflective_env && material.bump.y > 0.5) {
        // [matl_bumpmap]: the height map's slope bends the normal (tangent frame from the
        // screen-space derivatives of position and texture coordinate)
        let slope = bump_offset(duv);
        let dp1 = dpdx(in.world);
        let dp2 = dpdy(in.world);
        let du1 = dpdx(duv);
        let du2 = dpdy(duv);
        let dp2p = cross(dp2, n);
        let dp1p = cross(n, dp1);
        let t = dp2p * du1.x + dp1p * du2.x;
        let b = dp2p * du1.y + dp1p * du2.y;
        let inv = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-12));
        n = normalize(n - (t * inv * slope.x + b * inv * slope.y) * 0.6);
    }
    var albedo = tex.rgb * material.color.rgb;
    // OMSI materials have separate diffuse and ambient colours. Some interiors have
    // black diffuse but white ambient: using diffuse for both made them pitch black.
    // Keep the direct response, and weather both colours with the same surface effects.
    // A slot without a texture (ambient.w -1) takes its diffuse colour for both: Omsi.exe's
    // white ambient of every o3d slot under a strong sky turned a coloured model white (#1737).
    var ambient_albedo = select(tex.rgb * material.ambient.rgb, albedo, material.ambient.w < -0.5);
    var detail_factor = 1.0;
    if (camera.flags.x > 0.5 && (terrain || in.params2.w > 0.5)) {
        let k = clamp(1.0 - (dist - 25.0) / 120.0, 0.0, 1.0);
        let pattern_xy = world_pattern_xy(in.world);
        detail_factor = 1.0 + (detail_noise(pattern_xy) - 0.5) * 0.42 * k;
        albedo = albedo * detail_factor;
        ambient_albedo = ambient_albedo * detail_factor;
    }
    // --- the material in physical terms
    var refl = 0.0;
    if (reflective_env) {
        // (with a mask of its own or a [matl_transmap] the share is the mask's alone, as
        // Omsi.exe's stage takes it - the factor counts only where there is none)
        let by_mask = material.params.z > 0.5 || (u32(material.params2.w + 0.5) & 1u) != 0u;
        refl = clamp(select(min(material.params2.y, 1.0), 1.0, by_mask) * reflection_mask(duv, diffuse_a), 0.0, 1.0);
    }
    var metal = 0.0;
    var f0 = vec3<f32>(0.04);
    var rough = 0.8;
    let probe_only = camera.clouds.w < 1.5;
    if (terrain) {
        rough = 0.92;
        f0 = vec3<f32>(0.03);
    } else if (glass) {
        // OMSI's window materials are deliberately very transparent. Raise the base
        // dielectric reflection slightly so the outside scene remains readable through
        // the pane instead of leaving a flat pale surface at normal incidence.
        rough = 0.04;
        // the pane's [matl_envmap] factor says how much it mirrors (its alpha is its
        // transparency, never a mask): from glass's own 4 % up to 12 % for a factor of 1
        // (0.4 on the Scania's panes). Up to 26 % as before, every window of every bus
        // was a mirror - far more than the original's panes reflect (issue #176).
        // ... measured against the vanilla picture: Omsi.exe lays the sphere map over the
        // pane by its factor and the pane shows that as faintly as it shows itself - the
        // share it mirrors is the pane's alpha times the factor (0.25 .. 0.5 on the stock
        // windows), at every angle, with no Fresnel term. Taken as the reflectance
        // averaged over the angles (Schlick's mean, f0 + (1 - f0) / 21, as for the paint),
        // the pane mirrors as much as there and rises towards grazing as glass does; 4 to
        // 12 % at the normal, and three quarters of that, left the windows showing the
        // saloon where the vanilla picture shows the street.
        let share = clamp(alpha * min(material.params2.y, 1.0), 0.0, 1.0) * camera.cam_pos.w;
        f0 = vec3<f32>(clamp((share - 1.0 / 21.0) * 21.0 / 20.0, 0.04, 0.9));
    } else if (reflective_env) {
        // Paint reflects its few per cent through a smooth clear coat; much more than a few
        // per cent is polished metal - but only where the model says so with a mask of its
        // own ([matl_envmap_mask]). Nearly every car body carries `[matl_envmap] … 1` with
        // no mask, which OMSI takes as "reflect the sphere map fully" and not as chrome:
        // read as metalness it made a Golf's bonnet a mirror, in which the envmap photo's
        // trees stood as contour lines across the paint at close range.
        //
        // The paint itself is a clear coat: a lacquer of refractive index 1.5 (4 % at normal
        // incidence, Fresnel's 100 % at grazing) polished smooth, which mirrors the street
        // as sharply as the windows do. How much of the sphere map OMSI lays over a
        // material says how glossy it is: the stock bodies carry 1 or 10, the cabin's
        // panels, frames and plastic trim 0.05 .. 0.2 - satin, a blurred sheen; the mask
        // (the texture's alpha) says where. (Roughness
        // 0.22 for every body, against the photo's contour lines on a car's bonnet, left
        // the paint matte beside its sharp windows.)
        let masked = (u32(material.params2.w + 0.5) & 1u) != 0u;
        let metal_ok = (u32(material.params2.w + 0.5) & 4u) != 0u;
        metal = select(0.0, smoothstep(0.3, 0.85, refl), masked || metal_ok);
        // The coat is as smooth as lets it mirror, averaged over the angles, the share
        // Omsi.exe lays over it: a polished 4 % coat mirrors 8.7 % on average (Fresnel's
        // rise at grazing), a rough one less (2.5 % at 0.8, its grazing light scattered and
        // shadowed by its own roughness). So a share of 8.7 % and more is smooth lacquer, the
        // stock bodies' few per cent - the MAN NL's 15 of 255 - a satin one (taken as rough
        // up to a share of a half they came out matte, #2067; smooth from 4 %, a body meant
        // to mirror barely shone like glass against the light, #2474), and where the mask
        // says nothing reflects - a tyre or a seal - the rough rubber or plastic it is.
        // Between those and chrome, the share says how much the paint mirrors, not how
        // sharply: Omsi.exe lays a soft sphere map over it, a blurred sheen of the sky. Taken
        // as polished glass from 8.7 % on (0.2.42), the many mod bodies whose texture alpha
        // holds a tenth to a half mirrored the grey sky like chrome - silver buses and AI
        // cars that glowed (#2563, #2301). Only a share near the whole is a sharp mirror.
        let coat_rough = max(clamp(0.06 + (0.087 - refl) * 11.9, 0.06, 0.8), mix(0.3, 0.06, smoothstep(0.5, 0.95, refl)));
        // Where the model says how much it mirrors - a [matl_envmap_mask], or the texture's
        // alpha under a [matl_transmap] (most mod bodies) - Omsi.exe lays the sphere map
        // over the paint by that share at every angle (shader.wgsl's environment stage):
        // OMSI has no Fresnel term, so the share is the reflectance averaged over the angles
        // the surface is seen at. A Fresnel reflectance with that average (Schlick's, whose
        // hemispherical mean is f0 + (1 - f0) / 21) mirrors as much as the vanilla picture
        // and rises towards grazing as a real coat does. (A plain 4 % coat left those
        // bodies far fainter than in the vanilla picture.) A body with neither keeps the
        // lacquer's 4 %: there Omsi.exe's lerp by the factor alone made every Golf a mirror.
        let authored = masked || material.params.z > 0.5;
        let coat = select(0.04, clamp((refl - 1.0 / 21.0) * 21.0 / 20.0, 0.04, 0.95), authored);
        f0 = mix(vec3<f32>(coat), mix(albedo, vec3<f32>(1.0), 0.4) * refl, metal);
        rough = mix(coat_rough, 0.14, metal);
    } else if (!thin && material.specular.w > 0.0 && dot(material.specular.rgb, vec3<f32>(1.0)) > 0.05) {
        // the o3d material's Blinn-Phong power as GGX roughness
        rough = clamp(sqrt(sqrt(2.0 / (material.specular.w + 2.0))), 0.4, 0.9);
    } else if (thin) {
        rough = 0.7;
    }
    if (is_water) {
        // water: a dielectric of 2 % at normal incidence, nearly a mirror where it is seen
        // flat, smooth but for its waves
        f0 = vec3<f32>(0.02);
        rough = 0.06;
        metal = 0.0;
    }
    // --- a PBR set beside the diffuse texture (`foo_n.png`, `foo_r` / `_m` / `_ao` or
    // `foo_orm`: see omsi_texture::pbr): the normal map bends the normal, the packed map
    // gives the occlusion, roughness and metalness in place of the guesses above
    var pbr_ao = 1.0;
    if (material.pbr.x > 0.5 && !terrain) {
        var tn = textureSample(t_pbr_normal, s_diffuse, duv).xyz * 2.0 - vec3<f32>(1.0);
        // (an OpenGL-style map, green up: `_gl` in its name)
        if (material.pbr.x > 1.5) {
            tn.y = -tn.y;
        }
        n = perturb_normal(n, in.world, duv, tn);
    }
    if (material.pbr.y + material.pbr.z + material.pbr.w > 0.5 && !terrain) {
        let orm = textureSample(t_pbr_orm, s_diffuse, duv).rgb;
        if (material.pbr.y > 0.5) {
            pbr_ao = orm.r;
        }
        if (material.pbr.z > 0.5) {
            rough = clamp(orm.g, 0.03, 1.0);
        }
        if (material.pbr.w > 0.5) {
            metal = orm.b;
            f0 = mix(vec3<f32>(0.04), albedo, metal);
        }
    }
    // rain: roads with [moisture] turn dark and smooth, everything outside a little glossier
    // (not under snow: a snowy road took the rain's gloss and the snow on it shone like
    // plastic in every headlight)
    let dry_snow = 1.0 - clamp(enh.weather.y, 0.0, 1.0);
    let wet_road = camera.shadow.w * material.params2.z * outside * dry_snow;
    let wet_any = camera.shadow.w * outside * select(0.35, 0.0, glass) * dry_snow;
    var puddle = 0.0;
    if (wet_road > 0.0) {
        albedo = albedo * mix(1.0, 0.5, wet_road);
        ambient_albedo = ambient_albedo * mix(1.0, 0.5, wet_road);
        rough = mix(rough, 0.12, wet_road);
        f0 = mix(f0, vec3<f32>(0.02), wet_road);
        // Puddles: standing water pools in low, flat spots rather than spreading evenly
        // over the whole carriageway the way the sheen above does. A world-space mask at a
        // frequency of its own (unrelated to the road's detail noise, so the two do not
        // look tied together) picks out patches that grow with the wetness and vanish again
        // as the road dries; a puddle goes glass-flat and mirrors what is above it. While it
        // is actually raining rather than merely wet, and never while it is snow settling
        // instead (`enh.weather.z` is fed by any precipitation, `.y` singles snow back out),
        // concentric rings cross a puddle where drops land, fading as they widen - the other
        // half of the wheel splashes in `puddles.rs`.
        let pattern_xy = world_pattern_xy(in.world);
        // (the pools: see `road_puddle_coverage`, shared with the classic picture)
        puddle = road_puddle_coverage(in.world, n, wet_road);
        if (puddle > 0.001) {
            // A drop is a few millimetres across and its ring dies away within a hand's
            // breadth, so the shared ripple grid is 12.5 cm wide and a ring grows
            // to RIPPLE_MAX of a cell, about 5 cm: at 2 m cells with rings a metre and a half
            // across, every drop looked like a puddle of its own. Cells run at their own pace
            // and only some of them carry a drop at a time, so the surface reads as many small
            // impacts rather than one pulsing pattern.
            let raining = enh.weather.z * (1.0 - enh.weather.y);
            let ripple = puddle_ripple(pattern_xy, camera.post.y, raining, puddle);
            // Standing water hides most of the fine asphalt grain. Keep dry and damp
            // asphalt's detail, but let the reflected image read across a filled pool.
            albedo = albedo * (1.0 - 0.68 * puddle) / mix(1.0, detail_factor, puddle * 0.8);
            ambient_albedo = ambient_albedo * (1.0 - 0.68 * puddle) / mix(1.0, detail_factor, puddle * 0.8);
            rough = clamp(mix(rough, 0.03, puddle) - ripple.z * 0.12, 0.02, 1.0);
            f0 = mix(f0, vec3<f32>(enh.debug.y), puddle);
            n = normalize(mix(n, geo_n, puddle) + vec3<f32>(ripple.xy, 0.0));
        }
    } else if (wet_any > 0.0) {
        // (a body's paint wears a film of water as smooth as glass: the street and the sky
        // mirror in a wet bus as sharply as in its windows)
        if (!terrain) {
            rough = mix(rough, select(rough * 0.6, min(rough, 0.04), reflective_env), select(wet_any, min(wet_any * 2.6, 1.0), reflective_env));
        }
        // Soaked by the rain, a porous surface - soil, a paving stone, plaster, bark -
        // turns darker and deeper in colour: the light that enters the water film and is
        // scattered back by the surface under it is partly reflected down again at the
        // film's top (total internal reflection), and meets the surface once more, which
        // absorbs a share of it each time (Lekner and Dorf, "Why some things are darker
        // when wet", Applied Optics 1988). Painted metal, glass and leaves shed the water.
        let soak = camera.shadow.w * outside * dry_snow * select(0.75, 1.0, terrain)
            * mix(0.45, 1.0, clamp(n.z, 0.0, 1.0)) * select(1.0, 0.0, reflective_env || glass || is_water || thin || metal > 0.5);
        albedo = mix(albedo, wet_albedo(albedo), soak);
        ambient_albedo = mix(ambient_albedo, wet_albedo(ambient_albedo), soak);
    }
    // snow lies on what faces up
    // (not on a shadow blob: whitened, it lit the snow under the bus instead of shading it)
    // (nor on a texture that is the season's snow picture, which shows the map's own snow
    // as OMSI 2 does: see the vanilla shader, #879)
    let snow = enh.weather.y * outside * select(1.0, 0.0, in.params2.w > 1.5 || material.ambient.w > 0.5);
    if (snow > 0.0) {
        let up = clamp(n.z, 0.0, 1.0);
        let ground = select(0.0, 1.0, terrain || material.params2.z > 0.0);
        // (a road kept clear - "snow on road" off - stays asphalt: whitened, it turned the
        // roads white exactly when the weather says they are cleared, #1362)
        let cleared = select(1.0, 0.0, camera.post.z > 0.5 && !terrain && material.params2.z > 0.0);
        let cover = cleared * snow * clamp(max(ground, smoothstep(0.78, 0.95, up) * 0.8), 0.0, 1.0) * (0.55 + 0.35 * tex.a);
        albedo = mix(albedo, vec3<f32>(0.82, 0.84, 0.88), cover);
        ambient_albedo = mix(ambient_albedo, vec3<f32>(0.82, 0.84, 0.88), cover);
        // fresh snow is all but matte: it scatters the light and shows no highlight
        rough = mix(rough, 0.95, cover);
        f0 = mix(f0, vec3<f32>(0.02), cover);
        metal = metal * (1.0 - cover);
    }
    // specular antialiasing: where the normal turns quickly across a pixel (a low-poly
    // bonnet close up, a curved body far away) the surface cannot reflect sharper than that
    // spread, or the interpolated normals show as bands in a mirror-like finish
    if (reflective_env && !glass) {
        let dndx = dpdx(n);
        let dndy = dpdy(n);
        let spread = min((dot(dndx, dndx) + dot(dndy, dndy)) * 2.0, 0.4);
        rough = sqrt(min(rough * rough + spread, 1.0));
    }
    let env_share = select(1.0, camera.cam_pos.w, reflective_env && !glass);
    metal = metal * env_share;
    var sf: Surface;
    sf.albedo = albedo * (1.0 - metal);
    ambient_albedo = ambient_albedo * (1.0 - metal);
    sf.f0 = f0;
    sf.rough = rough;
    let nv = clamp(mix(face_v, dot(n, v), face_w), 1e-4, 1.0);
    // (how wide a patch of the surface the pixel covers, m: see `relief_masking`)
    let relief_footprint = length(dpdx(in.world)) + length(dpdy(in.world));
    // --- the sun
    let s = camera.sun_dir.xyz;
    let nl = dot(n, s);
    var direct = vec3<f32>(0.0);
    if (enh.lights.w > 0.0 && max(enh.sun.r, enh.sun.g) > 1e-5) {
        var shadow = 0.0;
        // (Enhanced+ without a traced shadow for the surface: the far map's as well, below -
        // one call of it for both cases: two, each inlined, made the scene shader too big for
        // Apple's M4 compiler, whose service crashed on every scene pipeline)
        var far = false;
        if (glass && nl > 0.0) {
            // a pane lets nearly all of it through: the sun on it is its glint, for which one
            // compare says enough (the soft filter over a cab's windscreen, doors and side
            // windows was a millisecond of the frame)
            shadow = sun_shadow_hard(in.world, n);
            far = camera.clouds.w > 1.5;
        } else if (nl > 0.0 || thin) {
            // Enhanced+: the traced shadow where there is one for this surface
            // (whose cut-out leaves and fences the shadow map holds alone: see `render_inner`)
            let traced = rt_at(in.clip.xy, in.world);
            shadow = sun_shadow_soft(in.world, n, thin);
            if (traced.w > 0.5 && traced.z >= 0.0) {
                shadow = shadow * traced.z;
            } else {
                far = camera.clouds.w > 1.5 && traced.w < 0.5;
            }
        }
        if (far) {
            shadow = shadow * sun_shadow_far(in.world, n);
        }
        let e_sun = enh.sun.rgb * shadow;
        if (thin) {
            // foliage: a crown of leaves facing every way, whose normals OMSI points up
            // only to light it evenly - lit by the sun from any side (the shadow map
            // darkens its far side), a little through the leaves as well. A crown is
            // hundreds of leaves at every angle: on average about half of the sun falls on
            // them square on, whatever the sun's height (with the up-pointing normals alone a
            // low sun left every tree dull and dark while the walls beside it glowed). Seen
            // against the sun a leaf glows with what comes through it - yellow-green, the
            // light having passed the leaf's colour twice.
            let through = pow(clamp(dot(-v, s), 0.0, 1.0), 3.0);
            let leaf = sf.albedo / PI;
            direct = e_sun * (leaf * (0.42 + 0.3 * max(nl, 0.0) + 0.08 * max(-nl, 0.0))
                + leaf * min(sf.albedo * 2.2, vec3<f32>(1.0)) * through * 0.45);
        } else if (nl > 0.0) {
            // the sun is a disc, not a point: no highlight sharper than it
            let a = max(rough * rough, 0.012);
            let h = normalize(s + v);
            var spec = d_ggx(max(dot(n, h), 0.0), a) * v_smith(nv, nl, a) * f_schlick(f0, dot(v, h));
            // (a rough, unpolished surface's own relief hides its glint from a low sun: see
            // `relief_masking`; glass, paint, chrome, water and a wet film keep theirs)
            if (!glass && !reflective_env && !is_water && metal < 0.5) {
                spec = spec * relief_masking(nl, nv, rough, relief_footprint);
            }
            direct = e_sun * nl * (sf.albedo / PI * (vec3<f32>(1.0) - f_schlick(f0, nl)) + spec);
        }
    }
    // --- the moon: by night a directional light as the sun is, with its shadow when the
    // maps are drawn along it
    let em = enh.moon_light.rgb;
    if (max(em.r, em.g) > 1e-9) {
        let m = enh.moon.xyz;
        let nlm = dot(n, m);
        if (nlm > 0.0 || thin) {
            var ms = 1.0;
            if (enh.moon_light.w > 0.5) {
                ms = sun_shadow_soft(in.world, n, thin);
            }
            let leaf = sf.albedo / PI;
            if (thin) {
                direct = direct + em * ms * leaf * (0.42 + 0.3 * max(nlm, 0.0));
            } else {
                direct = direct + em * ms * nlm * leaf;
            }
        }
    }
    // --- sky and ground
    var ao = 1.0;
    if (camera.clouds.w > 0.5 && !capture) {
        ao = ao_at(in.clip.xy, in.world);
    }
    // AO is generated from the opaque depth buffer.  A transparent bus pane therefore
    // samples the seats/dashboard behind it and makes their dark outlines crawl across the
    // glass as the camera moves.  The pane has its own transmission/reflection path; it must
    // not inherit occlusion belonging to geometry on the far side of the window.
    // (nor any other blended surface: a van's translucent door showed the shade of what
    // stood behind it)
    if (glass || mode > 1.5) {
        ao = 1.0;
    }
    ao = ao * pbr_ao;
    let fr = f_schlick(f0, nv);
    // inside the player's vehicle the light comes in through the windows and off the
    // cabin's own walls: less of it, and neither as blue nor as directional as the sky's
    let in_cab = 1.0 - outside;
    // The cab's light is one even ambient already (cab_e below); the screen-space occlusion
    // on top of it went black in the hollow under the windscreen - the steering wheel and
    // the dashboard stood dark beside a brightly lit cash desk. Inside the cab it is taken
    // at a third of its strength.
    ao = mix(ao, 1.0 - (1.0 - ao) * 0.35, in_cab);
    let avg_e = enh.fog_color.rgb * (PI / 0.9);
    let cab_e = vec3<f32>(dot(avg_e, vec3<f32>(0.2126, 0.7152, 0.0722))) * vec3<f32>(1.0, 0.98, 0.95);
    let e_amb = mix(sh_irradiance(n), cab_e * CAB_AMBIENT, 0.6 * in_cab) * ao_bounce(ao, ambient_albedo);
    var ambient = e_amb * ambient_albedo / PI * (vec3<f32>(1.0) - fr * (1.0 - rough) * env_share);
    // --- reflections: the sky probe, as sharp as the surface is smooth
    var r = reflect(-v, normalize(mix(face_n, n, face_w)));
    let bend_n = normalize(mix(face_n, geo_n, face_w));
    // a reflection pointing into the surface would show the probe's ground: bend it up
    let below = dot(r, bend_n);
    if (below < 0.0) {
        r = normalize(r - bend_n * below * 1.02);
    }
    // A wet road, a wet pavement is no mirror: a soft, dim sheen of what stands round it, a
    // puddle a blurred picture of it (panes, envmapped paint and still water keep their own
    // smoothness). Its reflection is taken this rough, at this share.
    let wet_only = !(reflective_env || glass || is_water || ((material.pbr.z > 0.5 || material.pbr.w > 0.5) && !terrain));
    let refl_rough = select(rough, max(rough, mix(0.35, 0.16, puddle)), wet_only);
    let wet_share = select(1.0, mix(0.35, 0.6, puddle), wet_only);
    let lod = refl_rough * (enh.lights.x - 1.0);
    var env = textureSampleLevel(t_probe, s_lin, to_cube(r), lod).rgb * enh.fog_color.w;
    // the probe holds only the sky: in a street the low sky is hidden behind houses and
    // trees, which reflect about as much light as a quarter-white wall (without this every
    // surface wore a milky sheen of the bright horizon, strongest against a low sun)
    let surround = enh.fog_color.rgb * (0.25 / 0.9);
    let open = smoothstep(-0.05, 0.35, r.z);
    env = mix(surround, env, mix(open, 1.0, select(0.25, 0.5, reflective_env && !(glass && probe_only))));
    // the cabin reflects the cabin, not the sky - and so does the bus's own glass seen
    // from the driver's seat
    let cab_view = max(in_cab, near_player_vehicle(in.world) * inside_vehicle(camera.cam_pos.xyz));
    // From inside the player bus the pane is near the cabin, but it must still show the
    // outside sky/street. Using the opaque-surface cabin weight here made every window a
    // flat pale patch and hid the useful reflection while driving.
    // A pane is the boundary between the cabin and the outside. Mixing the cabin
    // irradiance into its reflection made every windscreen look like a pale grey sheet
    // from the driver's seat; the interior is already behind the alpha-blended pane.
    let cabin_reflection = select(0.85, 0.0, glass);
    env = mix(env, cab_e * 0.35 / PI, cabin_reflection * cab_view);
    // ... but the inner face of the bus's own glass cannot mirror the sky, which lies
    // behind it: it mirrors the cab, far darker than the street seen through it. Taken as
    // the sky, the doors (seen at a grazing angle from the driver's seat) went a milky
    // grey sheet in fog and snow, when the probe is bright all round.
    let own_pane = select(0.0, near_player_vehicle(in.world) * inside_vehicle(camera.cam_pos.xyz), glass);
    if (reflective_env) {
        // OMSI's photographed environment gives the reflection its structure (trees,
        // houses, the street), the probe its light
        // A pane looks it up by the reflection's direction in the world, not in the
        // camera's frame as Direct3D's sphere map does: taken from the camera, what a
        // standing bus's windows mirrored changed whenever the view turned, trees sliding
        // in from nowhere over the street seen through them. Height gives the photo's rows
        // (sky, the horizon's trees and houses, the ground), the compass direction a
        // gentle drift across it that stays put in the world.
        let az = atan2(r.y, r.x);
        var env_uv = vec2<f32>(0.5 + 0.3 * sin(az), 0.5 + 0.45 * clamp(r.z, -1.0, 1.0));
        // The paint reads the photo as Omsi.exe's environment stage lays it on: Direct3D's
        // sphere map of the reflection in the camera's frame (`sphere_map_uv`, 0x7ff670 sets
        // D3DTSS_TCI_SPHEREMAP and no texture matrix), so the picture on a body turns with
        // the camera's yaw and pitch as it does in OMSI. (A pane keeps the world's: through
        // it the street itself is seen, and the photo turning over it read as a smear.)
        if (!glass) {
            env_uv = in.env_uv;
        }
        if (material.bump.y > 0.5) {
            env_uv = env_uv + bump_offset(duv);
        }
        // (with the mip level its footprint needs on top of the blur: a fixed level showed
        // the photo's trees as contour lines across a car's curved bonnet close up, and
        // paint takes it as blurred as the vanilla path does)
        let photo = textureSampleBias(t_env, s_diffuse, env_uv, max(rough * 16.0, select(2.0, 0.0, glass))).rgb;
        let photo_avg = textureSampleLevel(t_env, s_diffuse, vec2<f32>(0.5, 0.5), 12.0).rgb;
        let lum_avg = max(dot(photo_avg, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.02);
        // (a tint, not a picture: an unbounded ratio drew the photo's trees as bands)
        // (the photo's own contrast, as the vanilla picture shows it, held only off black
        // and off a blinding white; clamped to 0.35 .. 2 and laid on at two thirds, the
        // windows mirrored a flat grey where the vanilla ones mirror trees and sky)
        let ratio = clamp(photo / lum_avg, vec3<f32>(0.12), vec3<f32>(select(3.5, 1.0, glass && probe_only)));
        let band = 1.0 - smoothstep(0.25, 0.7, abs(r.z));
        // Only a smooth surface mirrors the photo; a satin one a quarter of its contrast.
        // (A car's bonnet is a handful of triangles: the photo's mip level by its footprint
        // above keeps its trees from turning into contour lines across it close up.)
        let sharpness = 1.0 - smoothstep(0.05, 0.25, rough);
        // ... and only as far as the reflection is of the outside at all: the driver's own
        // windscreen reflects the cab, and the photo's trees drawn over that stood as
        // contour lines across everything seen through it (a car's bonnet close up)
        let outside_env = 1.0 - cab_view;
        // ... and not in a fog: there is nothing sharp out there to mirror. The photo's
        // trees stood as flat grey silhouettes on every pane in fog, in front of the real
        // (fogged) trees behind the glass - outlines of trees that are not there
        let clear_air = exp(-enh.fog.x * 150.0);
        env = env * mix(vec3<f32>(1.0), ratio, band * mix(0.25, 1.0, sharpness) * outside_env * clear_air * enh.debug.z);
    }
    // what the SSAO darkens, it also keeps reflections out of
    let spec_occ = clamp(pow(nv + ao, exp2(-16.0 * rough - 1.0)) - 1.0 + ao, 0.0, 1.0);
    // (a PBR set's roughness or metalness map says how it reflects: the probe, as for an envmap)
    let pbr_reflects = (material.pbr.z > 0.5 || material.pbr.w > 0.5) && !terrain;
    // (a wet road mirrors the sky probe as well; and what reflects nothing keeps the light
    // the Fresnel term took off its ambient above - at a grazing angle that term is near 1,
    // and the far road and ground went dark with no reflection in its place, #374)
    let reflects = reflective_env || glass || pbr_reflects || is_water || wet_road > 0.0;
    var refl_f = env_brdf(f0, rough, nv) * spec_occ * select(1.0, wet_road, !(reflective_env || glass || pbr_reflects || is_water)) * wet_share;
    if (!reflects) {
        ambient = e_amb * ambient_albedo / PI;
    }
    if (glass) {
        // Transparent bus panes need a readable outside reflection from the driver's
        // viewpoint; opaque paint must never receive this boost.
        refl_f = refl_f * (1.0 - 0.85 * own_pane);
    }
    refl_f = refl_f * env_share;
    var reflection = select(vec3<f32>(0.0), env * refl_f, reflects);
    // Enhanced+: traced instead (rough surfaces keep the probe, which is as good as a
    // ray there): the windows, the chrome and the paint's clear coat mirror the real street
    // round them - the coat as sharply as the glass, where it is polished (`coat_rough`)
    let traced = camera.clouds.w > 1.5 && reflects && refl_rough < 0.75 && !capture;
    if (traced) {
        var w = dot(refl_f, vec3<f32>(0.2126, 0.7152, 0.0722)) * aer.a;
        if (glass) {
            w = w * smoothstep(0.0, 0.05, alpha);
        } else if (mode > 1.5) {
            // (a blended layer reflects only where it is there: a painted ground's brush mask)
            w = w * clamp(alpha, 0.0, 1.0);
        }
        let rough_t = refl_rough;
        if (w > 0.002) {
            reflection = vec3<f32>(0.0);
            rt_gbuf = vec4<f32>(n * w, w);
            rt_aux = vec4<f32>(dist * w, rough_t * w, 0.0, w);
        }
    }
    // --- the lamps, the cabin light and what glows by itself
    // ([nomaplighting] objects are not lit by the map's lamps; light-mapped roads are, with
    // the tile light map on top)
    // (no shadows inside the player's own vehicle, whose cab the depth hardly shows, nor in
    // the probe's capture)
    let film = select(vec3<f32>(0.0), vec3<f32>(wet_road, refl_rough, wet_share), wet_only && wet_road > 0.0);
    // A clear pane scatters nothing: what lights it shows only in what it mirrors (the
    // lamps' glints, the sky, the street) and in the little its tint takes from the daylight
    // through it. Lit as a grey Lambertian layer, every lamp and the cab's own lamps laid a
    // milky veil over the windows at night, over everything seen through them.
    var sf_lit = sf;
    if (glass) {
        sf_lit.albedo = vec3<f32>(0.0);
    }
    let lamps = lamp_light(in.world, n, v, sf_lit, thin, !capture, film) * select(1.0, 0.0, material.params.y > 0.2 && material.params.y < 0.3);
    // [interiorlight]: OMSI adds its lamps' light to the lit meshes whatever the daylight,
    // so a switched-on saloon is brighter by day as well and only stands out more at night.
    // Taken as a lamp against the daylight exposure it vanished by day altogether.
    // (in the picture's own terms, as the vanilla pass adds it: scaled with the metering it
    // grew with the night and whitened the saloon)
    // (the lamps' light does not reach into the gaps under the seats and round the
    // handrails either: without the ambient occlusion on it they glowed through there)
    // A sphere-mapped pane or body mirrors the street round it: at night that is lit by the
    // lamps, not by the sky the probe holds (dark then). The street's radiance under them
    // (`lamp_ground`) is laid into the reflection where it points at the street and the
    // walls: a bus's windows and paint under a street lamp show the lit street, the ones on
    // an unlit road stay dark. (Not through the photo's structure: its bright rows are the
    // day's sky, which the lamps do not light.)
    if ((reflective_env || glass) && !traced) {
        let lit_round = lamp_ground * (1.0 - open);
        reflection = reflection + lit_round * refl_f * (1.0 - cab_view);
    }
    let cabin_light = interior_lamps(in.world, n, in.params2.z);
    let cabin = sf_lit.albedo * cabin_light * mix(1.0, ao, 0.85);
    var rgb = (direct + ambient + lamps) * pre + cabin;
    // A material's own emissive colour ([matl_allcolor], an .x's emissive) is the texture at
    // full brightness in Omsi.exe: shown at the screen's white, not scaled with the eye's
    // night adaptation - a texture lit that way by [matl_allcolor] glared at several times
    // white at night (#1228, #1236). (The night maps of lit windows keep their light.)
    // (not a pane's: glass gives off no light of its own, see `sf_lit`)
    var emit = select(tex.rgb * material.emissive.rgb * clamp(enh.exposure.z * 2.0, 0.8, 1.0), vec3<f32>(0.0), glass);
    // (the tile light map on the splines and [LightMapMapping] objects is the vanilla
    // path's: here the map's lamps light them, tinted from that map, as they light every
    // other surface - added on top it lit the roads twice, with a hard edge where a road
    // met a square that is an object)
    if (material.extra.w > 0.5) {
        let nuv = select(buv, vec2<f32>(in.uv.x, 1.0 - in.uv.y), terrain);
        let switched = material.extra.w > 1.5;
        let night = select(camera.sun_color.w, 1.0, switched);
        // (a switched one is the display's own state: not dimmed with the instance's night
        // lighting, which is 0 by day and left the Procity's pressure screen black)
        let nm = sample_nightmap(nuv).rgb * night * select(clamp(in.params2.y, 0.0, 1.0), 1.0, switched);
        if (terrain) {
            // the tile's light map: the lamps' light on the ground
            rgb = rgb + sf.albedo / PI * nm * enh.lights.y * 3.0 * pre;
        } else {
            // lit windows and signs; a switched lamp or display holds up against daylight
            // as the self-illuminated materials do
            // (a window lit from inside shows some 80 cd/m² - a room's 300 lux off its
            // walls and through a curtain - an illuminated sign more: over the dark street
            // round it, well over the screen's white, and the eye's glare blooms round it)
            emit = emit + nm * select(enh.exposure.z * 3.6, max(enh.exposure.z * 2.0, 0.8), switched);
        }
    }
    if (material.params2.x > 0.5 && !terrain && !glass) {
        // [matl_lightmap]: laid onto the light with ADDSMOOTH in Omsi.exe (see the vanilla
        // shader), so it adds what the surface's own light leaves: little by day, fully at
        // night, and less where the saloon lamps light the surface already. (As an emission
        // at 0.6 of the texture by day and on top of the lamps at night, a switched-on
        // cabin's light-mapped parts were flat white.)
        let lm = textureSample(t_light, s_diffuse, buv).rgb * clamp(in.params2.x, 0.0, 1.0);
        if (material.emissive.w < -1.5) {
            let night = clamp(camera.sun_color.w, 0.0, 1.0);
            let left = (vec3<f32>(1.0) - clamp(cabin_light, vec3<f32>(0.0), vec3<f32>(1.0))) * (0.12 + 0.88 * night);
            emit = emit + tex.rgb * lm * left * max(enh.exposure.z * 2.0, 0.6);
        } else {
            // (By day as by night: ADDSMOOTH lifts the surface to its texture's colour by the
            // map whatever light it has, and what the light already gives - the day's, the
            // saloon lamps' - is taken off below. Lifted by an eighth of that by day, a
            // display in the cab's shade was far darker than in Vanilla+, #2051.)
            // ... and as there it lifts the surface to its texture's own colour at the
            // most, shown as a display is (see the unlit path): taken as a light of the
            // night's level on top, a white light map (`weiss.bmp`, laid on a display so
            // it shows at night) went past the tone curve's knee and bleached its colours -
            // the Procity's red and blue gauges pink and lavender (#827). (An LED panel's
            // light map stays as it was: its dots are meant to burn above their colour.)
            // The map's script variable controls its backlight. Dimming every light-mapped
            // surface as a cab screen hid instrument backlights even with the lights on.
            emit = emit + max(display_level(tex.rgb) * enh.exposure.y - rgb, vec3<f32>(0.0)) * lm;
        }
    }
    if (material.emissive.w < -1.5) {
        // an LED panel (see MaterialExtra::led): the lit dots - the alpha the `\S:n` script
        // texture carries, in the colour of the panel's own texture - are the panel's own
        // light, drawn as bright as the settings ask for (`Led glow`, 16 levels, 0 = off).
        // The glow takes them where it leaves every other screen out of its source
        // (`post.wgsl`) and blooms a halo around the panel. (Kept at their own brightness
        // however the metering treats the scene, as a display's text is.) Its light is its
        // white light map's, so it goes out with that map's variable (the busbar, the
        // lights) as the Omsi.exe stage does.
        let lm_gate = select(1.0, clamp(in.params2.x, 0.0, 1.0), material.params2.x > 0.5);
        // (in the cab - a dashboard's LCD lit by a white map reads as a panel - it dims at night;
        // not a page's destination sign (-3), which is read from the street through the windscreen)
        let cab_dim = select(1.0 - outside, 0.0, material.emissive.w < -2.5);
        // (`material.glow.y`: the share of it the modeller gives the panel, `[led_glow_effect]`)
        emit = emit + tex.rgb * enh.led.x * material.glow.y * alpha * lm_gate * max(enh.exposure.z * 2.0, 0.8) * mix(1.0, display_dim(1.0), cab_dim);
    } else if (material.emissive.w < -0.5) {
        // a display's text (see MaterialExtra::display)
        emit = emit + tex.rgb * 0.35 * max(enh.exposure.z * 2.0, 0.8) * display_dim(1.0);
    }
    if (material.glow.x > 0.0) {
        // [matl_glow] (see `matl_glow_light`): the material is its own light, drawn in HDR.
        // The classic picture does not know the keyword (`params2.x` is off, see
        // `add_material_extra`).
        emit = emit + matl_glow_light(buv, tex.rgb);
    }
    rgb = rgb + emit;
    if (enh.debug.x > 0.5) {
        // OMSI_DEBUG_ENHANCED: one term alone, as it lands on the screen
        let dm = i32(enh.debug.x);
        var dc = vec3<f32>(0.0);
        switch dm {
            case 1: {
                let tr = rt_at(in.clip.xy, in.world);
                let sm = sun_shadow_soft(in.world, n, thin);
                dc = select(vec3<f32>(sm), vec3<f32>(tr.z * sm, tr.z * sm, tr.z * 0.6 + 0.4), tr.w > 0.5 && tr.z >= 0.0);
            }
            case 2: { dc = vec3<f32>(ao); }
            case 3: { dc = n * 0.5 + vec3<f32>(0.5); }
            case 4: { dc = vec3<f32>(aer.a); }
            case 5: { dc = e_amb * pre * 0.1; }
            case 6: { dc = reflection * pre; }
            case 7: { dc = sf.albedo; }
            case 8: { dc = direct * pre; }
            case 9: { dc = aer.rgb * pre; }
            case 11: { dc = vec3<f32>(fract(dist / 10.0), dist / 1000.0, in.clip.z * 100.0); }
            case 12: { dc = vec3<f32>(mode * 0.5, f32(terrain), in.params2.w); }
            case 13: { dc = vec3<f32>(in_cab, ao, spec_occ); }
            case 14: { dc = (ambient + direct) * pre; }
            case 15: { dc = lamps * pre; }
            case 16: { dc = emit; }
            case 17: { dc = vec3<f32>(rough, f0.g * 10.0, metal); }
            default: { dc = vec3<f32>(alpha, f32(glass), f32(has_env)); }
        }
        return vec4<f32>(dc, 1.0);
    }
    if (glass) {
        // see-through glass: the reflection is added on top of what shows through, so
        // the blend keeps it where the glass itself is faint - but not where the texture
        // is not there at all: Omsi.exe leaves the alpha as the texture has it, so a
        // see-through part of a blended layer (the clear ground of a sticker on the
        // cab's wall, #861) shows no reflection; made up to a quarter opaque by it, the
        // whole rectangle of the sticker mirrored the sky
        let cover = smoothstep(0.0, 0.05, alpha);
        let refl_rgb = reflection * pre * cover;
        let rl = dot(refl_rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        // (a pane passes most light: a reflection making it up to 60 % opaque laid a grey
        // veil over the destination display behind the windscreen and the saloon)
        var a2 = clamp(alpha + (1.0 - alpha) * clamp(rl * 0.25 + fr.g, 0.0, 0.25) * (1.0 - 0.85 * own_pane) * cover, alpha, 1.0);
        var c = (rgb * alpha + refl_rgb) / max(a2, 1e-3);
        // the condensation on the player's bus's panes (omsi-app condensation.rs): a film of
        // droplets, each many wavelengths wide, takes twice its shadow out of the light
        // (the extinction paradox): half of what it scatters is diffracted a degree or two
        // forward - what lies behind goes soft, in the tone mapping (post.wgsl `misted`) -
        // and half is refracted and reflected every way, a milky veil lit by the street
        // through the glass and, the part the droplets send back, by the cabin
        let fog = pane_condensation(in.world) * cover;
        glass_fog = fog;
        if (fog > 0.001) {
            // (of the widely scattered half, some 30 % goes back: a fully misted pane sends
            // some 15 % of the light back, Briscoe and Galvin 1991)
            let back = 0.3;
            let lit_street = avg_e / PI * pre;
            let lit_cabin = cab_e * CAB_AMBIENT / PI * pre + cabin_light;
            let milky = lit_street * (1.0 - back) + lit_cabin * back;
            let veil = 0.5 * fog;
            let a3 = a2 + (1.0 - a2) * veil;
            c = (c * a2 * (1.0 - veil) + milky * veil) / max(a3, 1e-3);
            a2 = a3;
        }
        return vec4<f32>(c * aer.a + aer.rgb * pre, a2);
    }
    rgb = rgb + reflection * pre;
    // The later screen-space pass replaces only this fraction of the sky reflection.
    // A miss adds zero, so the existing sky, Fresnel and fog stay the fallback. Colour
    // and coverage are resolved together, including terrain painting and glass over roads.
    if (!glass && !reflective_env) {
        let weight = clamp(puddle * env_brdf(f0, rough, nv).g
            * select(wet_road, 1.0, pbr_reflects) * wet_share * aer.a, 0.0, 1.0);
        *puddle_weight = vec2<f32>(weight, weight * spec_occ);
    }
    // water hides what lies under it as far as it reflects (Fresnel): see-through from
    // above, a mirror of the sky towards the horizon
    let a_out = select(alpha, clamp(alpha + (1.0 - alpha) * fr.g, alpha, 1.0), is_water);
    return vec4<f32>(rgb * aer.a + aer.rgb * pre, a_out);
}

