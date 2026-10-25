@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    var unused = 0.0;
    return shade_vanilla(in, &unused, camera.cam_pos.xyz);
}

// Shared with Enhanced and the wheel splash mask: pools spread as the road soaks.
fn road_puddle_coverage(world: vec3<f32>, normal: vec3<f32>, wet: f32) -> f32 {
    let xy = world_pattern_xy(world);
    let pn = vnoise_f(xy, 0.35, vec2<f32>(17.3, -9.1)) * 0.6
        + vnoise_f(xy, 1.6, vec2<f32>(-4.0, 8.0)) * 0.3
        + vnoise_f(xy, 5.0, vec2<f32>(2.7, 11.3)) * 0.1;
    // The pools spread from the lowest spots as the road soaks, but they stay pools: a road
    // wet through has standing water on about a fifth of it (PUDDLE_SPREAD in enhanced.wgsl,
    // the same in `omsi-app/src/puddles.rs`) and wet asphalt between.
    let threshold = 1.0 - wet * PUDDLE_SPREAD;
    return smoothstep(threshold - 0.02, threshold + 0.02, pn) * smoothstep(0.75, 0.95, normal.z);
}

@fragment
fn fs_vanilla_reflections(in: FsIn) -> EnhancedOut {
    var weight = 0.0;
    let c = shade_vanilla(in, &weight, camera.cam_pos.xyz);
    let coverage = select(c.a, 0.0, in.params2.w > 1.5);
    var out: EnhancedOut;
    out.color = c;
    out.mask = vec4<f32>(0.0, weight * 0.49, weight, coverage);
    return out;
}

fn shade_vanilla(in: FsIn, puddle_weight: ptr<function, f32>, eye: vec3<f32>) -> vec4<f32> {
    // (the pane's water, read once for the uses below)
    let film_water = window_wetness(in);
    if (material.emissive.w > 1.5) {
        // a pane's film of water: drops, not the sliding texture
        let v = normalize(camera.cam_pos.xyz - in.world);
        let in_cab = inside_vehicle(camera.cam_pos.xyz) * near_player_vehicle(in.world) > 0.5;
        let g = rain_glass(in.world, in.uv - in.params.zw, in.normal, film_water, camera.post.y, in_cab, in.wipe_uv);
        if (g.cover <= 0.001 && g.mist <= 0.001) { return vec4<f32>(0.0); }
        let through = rain_through(g, v);
        let seen = select(rain_behind(in.world, through, rain_env_vanilla(normalize(through)), 1.0, g.mist), vec3<f32>(0.0), dot(through, through) < 1e-4);
        let d = rain_light(g, v, through, rain_env_vanilla(reflect(-v, g.n)), seen, rain_env_vanilla(vec3<f32>(0.0, 0.0, 0.5)) * 0.9, camera.sun_color.rgb * camera.sun_dir.w);
        // (fading out with the distance: see enhanced.wgsl)
        let near = 1.0 - smoothstep(5.0, 15.0, distance(in.world, camera.cam_pos.xyz));
        return vec4<f32>(min(d.rgb, vec3<f32>(1.5)), d.a * near);
    }
    var duv = tex_address(in.uv);
    if (material.extra.x > 0.5) {
        // terrain: uv is tile space; the ground texture repeats extra.z times per tile
        duv = in.uv * material.extra.z;
    }
    var tex = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, duv, camera.lod.x), duv);
    if (is_snow_pane()) {
        tex = snow_on_pane(in, film_water);
    }
    // The texture coordinates without the [texcoordtransX/Y] offset: in Omsi.exe's
    // fixed-function pipeline the texture transform is the diffuse stage's alone, the
    // transmap, night map and light map stay in place under a scrolling roller blind.
    let buv = tex_address(in.uv - in.params.zw);
    if (material.extra.x > 0.5 && material.extra.y > 0.0) {
        // the ground texture's detail texture, repeated finer than the texture itself and
        // modulated over it as the original's terrain pass does. The stock detail maps are
        // bright (noise_low averages 242, gras_det 179), so they are meant to be multiplied
        // in plainly: doubling like a grey-centred D3D detail map blows the ground out.
        let det = textureSampleBias(t_light, s_diffuse, in.uv * material.extra.y, camera.lod.x);
        tex = vec4<f32>(clamp(tex.rgb * det.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), tex.a);
    }
    if (material.params.z > 0.5) {
        // [matl_transmap]: alpha comes from a separate map, its alpha channel (a map without one
        // is opaque, as D3D samples it: the WH UK AI cars' paint layer has a black 24-bit
        // `transmap_null.tga`, read as luminance the paint was invisible);
        // for terrain the map is the per-tile surface mask in tile space
        let tm = sample_transmap(buv);
        tex.a = select(1.0, tm.a, material.params.w > 0.5);
        if (material.extra.x > 0.5 && material.params.x > 1.5) {
            // A painted ground layer. The brush mask is coarse (0.6-3 m per texel) and
            // binary; the loader smooths it into a soft ramp around a smooth curve
            // (`smooth_paint_mask` in scene.rs). Sharpen only that coverage ramp:
            // diffuse/detail mip colours change with viewing angle, and including
            // their luminance made covered ground fade into lower layers at a slant.
            tex.a = smoothstep(0.32, 0.68, tex.a);
        }
    }
    // One night-map read supplies both coverage and lighting. Keep the tile map
    // in its north-up UVs; vehicle maps use the unscrolled material UVs.
    var night_sample = vec4<f32>(0.0);
    if (material.extra.w > 0.5) {
        let nuv = select(buv, vec2<f32>(in.uv.x, 1.0 - in.uv.y), material.extra.x > 0.5);
        night_sample = sample_nightmap(nuv);
    }
    tex.a = night_coverage(tex.a, night_sample.a);
    let mode = material.params.x;
    // Mip-filtered alpha is the fraction of the pixel covered by leaves or fence wires.
    // Tighten the transition around the cutout edge before MSAA turns it into sample
    // coverage. The depth prepass leaves these draws out so its binary cutoff cannot hide
    // the scene behind samples that the colour pass leaves open.
    if (ALPHA_TEST && mode > 0.5 && mode < 1.5) {
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
    let n = safe_normal(in.normal);
    let ndl = max(dot(n, camera.sun_dir.xyz), 0.0);
    // three lights like the original: direct sun (A), light from above (B), ambient (C)
    let from_above = 0.5 + 0.5 * n.z;
    let shadow = sun_shadow(in.world, n, mode > 0.5 && mode < 1.5);
    // screen-space ambient occlusion darkens the indirect light (sky and ambient) in
    // corners, under the bus, between the seats - not the sun, which the shadow map handles
    var ao = 1.0;
    // (not on a blended surface: the AO is the opaque depth's, and a translucent door
    // showed the shade of what stood behind it)
    if (camera.clouds.w > 0.5 && mode < 1.5) {
        ao = ao_at(in.clip.xy, in.world);
    }
    var diffuse = camera.sun_color.rgb * camera.sun_dir.w * ndl * shadow + (camera.sky_color.rgb * from_above * (0.6 + 0.4 * shadow) + camera.ambient.xyz) * ao;
    var albedo = tex.rgb;
    if (camera.flags.x > 0.5 && (material.extra.x > 0.5 || in.params2.w > 0.5)) {
        // detail texturing (setting): procedural grain on the ground and the roads, fading out with distance
        let dist0 = distance(in.world, camera.cam_pos.xyz);
        let k = clamp(1.0 - (dist0 - 25.0) / 120.0, 0.0, 1.0);
        let nz = detail_noise(world_pattern_xy(in.world));
        albedo = albedo * (1.0 + (nz - 0.5) * 0.42 * k);
    }
    // ([nomaplighting]: params.y 0.25 - not lit by the map's lamps)
    // (a light-mapped road or plate takes the map's lamps only in Vanilla+: as OMSI 2 shows
    // it, the vanilla picture lights it from the tile light map alone)
    let lm_only = light_map_mapped(material.params) && camera.sky_color.w > 0.5;
    // (and a [tree]'s leaf cards, params.y 0.15: OMSI 2 leaves a tree dark even right under
    // a street lamp, where the lamp's 40 m core lit the crown up yellow-green)
    let tree_unlamped = camera.sky_color.w > 0.5 && material.params.y > 0.1 && material.params.y < 0.2;
    let map_lamps = select(1.0, 0.0, (material.params.y > 0.2 && material.params.y < 0.3) || lm_only || tree_unlamped);
    let lamp_light = point_lights(in.world, n, map_lamps);
    // (the headlight, lit at the vertices: see `classic_spot`)
    var light = diffuse + lamp_light + in.spot_diffuse + in.spot_ambient;
    // D3D lights the material's diffuse colour with the sun, the light from above and the
    // lamps, and its ambient colour with the ambient light (C) and the headlight's ambient
    // light. Omsi.exe makes every o3d slot's ambient white (0x7c62f8), so a white sign
    // texture on a green material shows white in the shade, not green.
    let ambient_light = camera.ambient.xyz * ao + in.spot_ambient;
    let mat_light = material.color.rgb * (light - ambient_light) + material.ambient.rgb * ambient_light;
    let light_mapped = material.params2.x > 0.5 && material.extra.x < 0.5;
    // The low-range Vanilla+ target has no exposure/tonemapping. Saturate the
    // diffuse light before modulating the texture, as the classic path does:
    // overlapping lamps must not clip a coloured texture into a white patch.
    // The light is OMSI's - the envir.cfg's sun, sky and ambient and its lamps - and those
    // numbers are what Direct3D multiplies into the gamma-encoded texture: it is laid on the
    // encoded texture here too, as in the classic picture below. Multiplied in linear light,
    // a light L showed as L^(1/2.2): the night's 0.06 as 0.28, every night, dusk and shade
    // in Vanilla+ several times as bright as in OMSI 2 (#300, #1612).
    var lit = srgb_decode(srgb_encode(albedo) * clamp(mat_light, vec3<f32>(0.0), vec3<f32>(1.0)));
    // The vanilla picture: Omsi.exe's texture stages multiply the gamma-encoded texture by
    // the vertex light (clamped at 1); here the texture is sampled linear and the target
    // encodes again, so multiplied here a light L showed as L^(1/2.2) - a night at 0.06
    // looked like 0.28, a late dusk (#300). So the product is made on the encoded texture
    // and decoded again, with the light map laid on encoded as well. (Through the sRGB curve
    // itself: taken as a power of 2.2, t v^2.2, the curve's linear foot below 0.0031 put a
    // night wall at a quarter of OMSI 2's - a texture of 0.66 under a light of 0.05 came out
    // at 2 of 255 instead of 8.)
    let classic = camera.sky_color.w > 0.5;
    // (the terrain's night map is its tile light map: light, not a glow - see below)
    let terrain_map = material.params.y < 0.5
        && material.extra.x > 0.5
        && material.extra.w > 0.5
        && material.extra.w < 1.5;
    let terrain_night = classic && terrain_map;
    if (classic && material.params.y < 0.5) {
        var v = clamp(material.emissive.rgb + mat_light + material.color.rgb * interior_lamps(in.world, n, in.params2.z), vec3<f32>(0.0), vec3<f32>(1.0));
        if (light_mapped) {
            let lm = srgb_encode(textureSampleBias(t_light, s_diffuse, buv, camera.lod.x).rgb) * clamp(in.params2.x, 0.0, 1.0);
            v = v + lm * (vec3<f32>(1.0) - v);
        }
        if (terrain_night) {
            // the terrain's tile light map lights the ground as the lamps' own light: added
            // to the vertex light before the texture is multiplied in. Laid over the lit
            // ground instead, as a night map glows, its faint fringe (0.01, linear) put
            // one beige veil over cobbles and grass alike, a whole car park the colour of
            // sand where OMSI 2 shows it dark grey.
            let nm = srgb_encode(night_sample.rgb);
            v = min(v + nm * camera.sun_color.w * clamp(in.params2.y, 0.0, 1.0), vec3<f32>(1.0));
        }
        if (lm_only) {
            // a road, a plate or a [LightMapMapping] object: the same tile light map, the
            // same way as the ground beside it (Omsi.exe lights both from it alone - its
            // `[maplight]`s only bake the map, 0x7903e0). Added after the texture in linear
            // light instead, the pools that lay bright on the verges hardly showed on the
            // asphalt between them: the street lamps lit everything but the road (#847).
            let lm = srgb_encode(light_map_at(in.world));
            v = min(v + lm * camera.sun_color.w, vec3<f32>(1.0));
        }
        lit = srgb_decode(srgb_encode(albedo) * v);
    } else if (light_mapped) {
        // [matl_lightmap], as Omsi.exe's texture stages have it (0x7fe4d3..0x7fe604): the
        // light map is laid onto the vertex light with D3DTOP_ADDSMOOTH (light + map x (1 -
        // light)) before the texture is multiplied in - a lit saloon glows at night and
        // hardly shows in daylight. The vertex light is D3D's: the material's emissive and
        // its colour times every light - the saloon lamps (`[interiorlight]`, D3D lights
        // too, 0x5fa8f0) among them - clamped at 1. (Added once more after the map, the
        // saloon lamps lit a cabin twice over, flat white where the map was full; and the
        // material's colour took the map down with it.)
        let lm = srgb_encode(textureSampleBias(t_light, s_diffuse, buv, camera.lod.x).rgb) * clamp(in.params2.x, 0.0, 1.0);
        let v = clamp(material.emissive.rgb + mat_light + material.color.rgb * interior_lamps(in.world, n, in.params2.z), vec3<f32>(0.0), vec3<f32>(1.0));
        lit = srgb_decode(srgb_encode(albedo) * (v + lm * (vec3<f32>(1.0) - v)));
    }
    // the D3D material's own highlight (specular colour and power of the o3d file or a
    // [matl_allcolor]), lit at the vertices (see `vertex_specular`) and added after the
    // texture as D3D's specular is; the sun's not in its shadow
    let specular = in.spec_sun * shadow + in.spec_sky;
    if (classic) {
        // D3D adds its vertex specular after the encoded texture stages. Adding
        // it in linear light encoded it again at the target and washed out dusk.
        lit = srgb_decode(min(srgb_encode(lit) + specular, vec3<f32>(1.0)));
    } else {
        lit = lit + specular;
    }
    if (material.params.y > 0.5) {
        lit = tex.rgb * material.color.rgb;
    }
    if ((!light_mapped && !classic) || material.params.y > 0.5) {
        lit = lit + tex.rgb * material.emissive.rgb;
    }
    // [interiorlight]: the saloon lamps on the meshes and passengers they illuminate (in
    // a light-mapped material's vertex light already, above)
    if (!light_mapped && !classic) {
        lit = lit + tex.rgb * interior_lamps(in.world, n, in.params2.z);
    }
    // (Vanilla+: the map's lamps light the ground, as they light the roads)
    if (material.extra.w > 0.5 && !terrain_map) {
        // [matl_nightmap]: self-illumination that fades in with the night
        // terrain: the tile light map in tile space (north at the top row)
        // a [matl_item] night map is switched by its variable (warning lamps, displays):
        // it glows whenever that is on, by day as well; the others fade in with the night
        let night = select(camera.sun_color.w, 1.0, material.extra.w > 1.5);
        lit = lit + night_sample.rgb * night * select(clamp(in.params2.y, 0.0, 1.0), 1.0, material.extra.w > 1.5);
    }
    if (material.params2.y > 0.0) {
        // [matl_envmap]: sphere map reflection, masked by the diffuse alpha like the original
        let vdir = normalize(in.world - eye);
        var env_uv = in.env_uv;
        if (material.bump.y > 0.5) {
            env_uv = env_uv + bump_offset(duv);
        }
        let env = textureSample(t_env, s_diffuse, env_uv);
        let diffuse_a = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, duv, camera.lod.x), duv).a;
        if (material.params.x < 1.5) {
            // rain: a painted body goes darker and glossier when it is wet, so the bus
            // stands out against a grey street instead of fading into it
            let env_night = select(1.0 - 0.85 * clamp(camera.sun_color.w, 0.0, 1.0), 1.0, camera.sky_color.w > 0.5);
            let env_light = clamp(camera.sun_color.r * camera.sun_dir.w + camera.sky_color.r + camera.ambient.r, 0.05, 1.0) * env_night;
            let wet = camera.shadow.w * weather_outside_n(in.world, n, false, in.params2.w);
            lit = lit * (1.0 - 0.30 * wet);
            // the water film mirrors the (overcast) sky a little, mostly at grazing
            // angles; kept faint - a strong rim read as a white outline round the bus
            let facing = clamp(abs(dot(vdir, n)), 0.0, 1.0);
            let sheen = wet * min(material.params2.y, 1.0) * (0.05 + 0.22 * pow(1.0 - facing, 5.0));
            lit = mix(lit, camera.sky_color.rgb * env_light, sheen);
        }
        // Omsi.exe's environment stage (0x7fd6c4 at 0x7ff626, fixed function), for paint
        // and glass alike: the sphere map's colour as it is, over what the stages before
        // made of the lit texture - D3DTOP_LERP by the texture factor when the material has
        // neither a [matl_transmap] nor a [matl_envmap_mask], D3DTOP_BLENDCURRENTALPHA when
        // it has one - and the alpha left as the stages before made it (ALPHAARG1 CURRENT,
        // SELECTARG1, 0x7ff98a): a clear pane shows its reflection as faintly as it shows
        // itself. (A glass path of our own - a quarter reflection at least, a Fresnel rim
        // and the pane made opaque where it reflected - mirrored the street in every
        // window far more than the original does.) The factor is [matl_envmap]'s times
        // the ambient light plus `g` (the light the vehicle stands in: the lamps of the
        // tile's light map and the sky), each at most 1, packed into a D3DCOLOR without
        // saturation (0x7ff8ed: a factor over 1 spills its bits into the next channel, as
        // there). The alpha it blends by is the mask's, or the diffuse texture's times `g`
        // (0x7feacc, 0x7ff092) - never the diffuse alpha of a material without one of
        // them: taken as a reflection mask, a body whose texture carries an alpha channel
        // for other purposes was mirrored (dark) where that alpha was high.
        // `g` (0x861ca0, set per vehicle at 0x7d8735): the mean of the light the tile's
        // light map throws on it plus the day's light A (weather +0xac = the mean of
        // lightcolor A, 0x75333f - by it the stars fade), at most 1
        let g = clamp(dot(lamp_light, vec3<f32>(1.0 / 3.0)) + dot(camera.sun_color.rgb, vec3<f32>(1.0 / 3.0)), 0.0, 1.0);
        var kk = vec3<f32>(0.0);
        if (has_transmap_declared() || has_env_mask()) {
            let a = select(diffuse_a, textureSampleBias(t_envmask, s_diffuse, duv, camera.lod.x).a, has_env_mask());
            kk = vec3<f32>(a * g);
        } else {
            kk = omsi_texture_factor(material.params2.y, camera.ambient.rgb + vec3<f32>(g));
        }
        // The lerp is made on the encoded values, as the stage makes it, like the texture x
        // light product above: made on linear ones it showed the reflection about twice as
        // bright over a dark surface - since the nights got dark (#300) the instrument glass
        // of the MAN NL/NG (Fenster.tga, factor 0.5, the sky at the sphere map's bottom) lay
        // milky white over the unlit gauges. (Vanilla+ as well: its linear lerp laid the
        // sphere map as a grey sheen over every dark window band and chassis, #780.)
        // The sphere map's colour as it is, as the stage takes it: the night already weakens
        // the reflection through the factor (`kk`, the ambient light and `g`). Darkened once
        // more by that light, as a photograph lit by the night, the reflection went twice
        // and the paint and the windows lost their shine at dusk and by night, which they
        // keep in OMSI (#2284).
        lit = srgb_decode(mix(srgb_encode(lit), srgb_encode(env.rgb), kk * camera.cam_pos.w));
    }
    // wet road: a surface whose texture carries [moisture] darkens under rain and starts
    // to mirror the sky, strongest where you look along it (the Fresnel sheen that makes a
    // wet street read as wet)
    let outside = weather_outside_n(in.world, n, material.extra.x > 0.5, in.params2.w);
    let wet = camera.shadow.w * material.params2.z * outside * (1.0 - clamp(camera.ambient.w, 0.0, 1.0));
    if (wet > 0.0) {
        let vdir = normalize(in.world - eye);
        let facing = clamp(-dot(vdir, n), 0.0, 1.0);
        let fresnel = pow(1.0 - facing, 4.0);
        let puddle = road_puddle_coverage(in.world, n, wet);
        let water = 0.02 + 0.98 * pow(1.0 - facing, 5.0);
        let weight = clamp(mix(fresnel * wet * 0.85, water * wet, puddle), 0.0, 0.9);
        let sheen = camera.sky_color.rgb * 0.5 + camera.sun_color.rgb * camera.sun_dir.w * 0.35;
        if (classic) {
            // Vanilla blends encoded weather colours, but asphalt darkening
            // must stay in linear light to avoid turning the road black.
            lit = srgb_decode(mix(srgb_encode(lit * mix(1.0, 0.75, wet)), sheen, weight));
        } else {
            lit = mix(lit * mix(1.0, 0.55, wet), sheen, weight);
        }
        *puddle_weight = weight * puddle;
    }
    // snow: the ground, the roads and every upward-facing surface whiten under it
    // (not on a shadow blob: whitened, it lit the snow under the bus instead of shading it)
    // (not in vanilla: OMSI 2 shows snow only through the season's WinterSnow textures)
    // (nor on a texture that is the season's snow picture: the map's own WinterSnow
    // textures show the snow as OMSI 2 does, and whitened over, the snowy grass and the
    // grey road went one flat white, the lane markings left standing in it, #879)
    let snow = camera.ambient.w * outside * select(1.0, 0.0, in.params2.w > 1.5 || camera.sky_color.w > 0.5 || material.ambient.w > 0.5);
    if (snow > 0.0) {
        let up = clamp(n.z, 0.0, 1.0);
        let ground = select(0.0, 1.0, material.extra.x > 0.5 || material.params2.z > 0.0);
        // (a road kept clear - "snow on road" off - stays asphalt, #1362)
        let cleared = select(1.0, 0.0, camera.post.z > 0.5 && material.extra.x < 0.5 && material.params2.z > 0.0);
        // only surfaces that really face up get a cover; a soft threshold keeps the snow
        // off the sides and off the grazing rims that showed as a white outline
        let cover = cleared * snow * clamp(max(ground, smoothstep(0.78, 0.95, up) * 0.8), 0.0, 1.0);
        let light = camera.sun_color.rgb * camera.sun_dir.w * ndl * shadow * 0.6 + camera.sky_color.rgb * 0.7 + camera.ambient.xyz;
        let white = vec3<f32>(0.92, 0.94, 0.98) * light * ao;
        lit = mix(lit, white, cover * (0.55 + 0.35 * tex.a));
    }
    let dist = distance(in.world, camera.cam_pos.xyz);
    let f = 1.0 - exp(-fog_distance(in.world) * camera.fog.w);
    *puddle_weight *= 1.0 - clamp(f, 0.0, 1.0);
    var rgb = mix(lit, camera.fog.xyz, clamp(f, 0.0, 1.0));
    if (classic) {
        // D3D's fixed-function fog blends the encoded vertex fog colour too.
        // Treating that colour as linear raised a mid-grey fog from 128 to 188.
        rgb = srgb_decode(mix(srgb_encode(lit), camera.fog.xyz, clamp(f, 0.0, 1.0)));
    }
    if (camera.flags.z > 0.0) {
        // Never taken: flags.z (the old enhanced look's aerial perspective) is always 0
        // now - the enhanced path has its own fragment shader (enhanced.wgsl). The branch
        // stays because Metal's fast-math contracts the fog mix above differently without
        // it, and the vanilla picture is to stay what it was to the last code value.
        let vdir = normalize(in.world - camera.cam_pos.xyz);
        let towards_sun = clamp(dot(vdir, camera.sun_dir.xyz), 0.0, 1.0);
        let k = (1.0 - exp(-dist * 0.00045)) * camera.flags.z * (0.55 + 0.45 * clamp(camera.sun_dir.w, 0.0, 1.0));
        let scatter = mix(camera.sky_color.rgb * 1.4 + camera.ambient.rgb * 0.5, camera.sun_color.rgb * camera.sun_dir.w + camera.sky_color.rgb, towards_sun * towards_sun);
        rgb = mix(rgb, scatter, clamp(k * 0.6, 0.0, 0.5));
    }
    // (a NaN anywhere above - a zero-length vector normalised, 0 x infinity - was a black
    // patch on Windows, e.g. an EN92's headlights while off, and invisible on the Mac)
    rgb = finite_or(rgb, finite_or(albedo * material.color.rgb * diffuse, albedo));
    var a = tex.a * material.color.a;
    if (mode < 0.5) {
        a = 1.0;
    }
    if (!is_snow_pane()) {
        a = a * clamp(film_water, 0.0, 1.0);
    }
    if (mode > 1.5 && min(material.emissive.r, min(material.emissive.g, material.emissive.b)) > 0.99 && material.emissive.w < 1.5) {
        a = encoded_blend_alpha(rgb, a, light);
    }
    return vec4<f32>(rgb, a);
}

// What lies under a self-lit layer, for `encoded_blend_alpha`: a surface of this encoded
// brightness lit by the light at the layer (the road's asphalt under the KVB Citaro's
// light cone is 0.44 - 0.49 encoded).
const BLEND_GROUND_ENCODED: f32 = 0.45;

// A self-lit blended layer (emissive white: a light cone laid on the road, a glow) as
// Omsi.exe blends it: Direct3D 9 mixes the gamma-encoded colours, the sRGB target here mixes
// linear ones, and a faint layer came out far brighter - the KVB Citaro's `lichtkegel.o3d`
// (alpha 0.14 on average) a flat beige sheet over the road where OMSI 2 shows a soft pool,
// 120 of 255 against its 85. The target cannot be read, so what is under the layer is taken
// to be ground lit by the light here (`light`, the vertex light the layer would have):
// the alpha is the one that gives Omsi.exe's encoded mix over it, never more than the
// layer's own.
fn encoded_blend_alpha(rgb: vec3<f32>, a: f32, light: vec3<f32>) -> f32 {
    let w = vec3<f32>(0.2126, 0.7152, 0.0722);
    let s = max(rgb, vec3<f32>(0.0));
    if (a <= 0.0) {
        return a;
    }
    let d_enc = clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)) * BLEND_GROUND_ENCODED;
    let d = dot(srgb_decode(d_enc), w);
    let over = dot(srgb_decode(srgb_encode(s) * a + d_enc * (1.0 - a)), w);
    let span = dot(s, w) - d;
    if (span < 1e-4) {
        return a;
    }
    return clamp((over - d) / span, 0.0, a);
}
