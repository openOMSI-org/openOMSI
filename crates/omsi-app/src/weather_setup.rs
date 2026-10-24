//! The weather of a session: its `.owt` file, the sky and clouds it makes, and how wet the streets are.

use super::*;

// (the cloud kinds, a hand-set weather's text and the METAR airports: omsi_content::weather_choice)
pub(crate) use omsi_content::weather_choice::{cloud_kind_index, cloud_kind_nearest, cover_of, custom_weather, dew_point_c, metar_airports, relative_humidity, CustomWeather, CLOUD_KINDS};
pub(crate) use omsi_weather::conditions::{cloud_drift_at, cloud_drift_step, initial_wetness, precip_of, road_wetness, street_condition};


/// Weather from `--weather`, else the clear-sky default.
pub(crate) fn load_weather(args: &Args) -> omsi_content::weather::Weather {
    load_weather_run(args).0
}

/// [`load_weather`], and the physical model's run when the weather is the model's, for the
/// session to keep in its `SkyState` and carry on with its clock.
pub(crate) fn load_weather_run(args: &Args) -> (omsi_content::weather::Weather, Option<crate::weather_model::ModelRun>) {
    if crate::weather_tomorrow::selected(args.weather.as_deref()) {
        let w = CustomWeather::default().to_weather();
        omsi_sim::host::set_ambient_weather(w.temp.0, w.temp.1);
        publish_page_weather(&w);
        return (w, None);
    }
    // no weather chosen, or `natural`: the physical model (weather_model.rs)
    if crate::weather_model::is_natural(args.weather.as_deref()) {
        let (run, w) = crate::weather_model::ModelRun::start(&crate::situation::start_clock(args));
        omsi_sim::host::set_ambient_weather(w.temp.0, w.temp.1);
        crate::weather_setup::publish_page_weather(&w);
        return (w, Some(run));
    }
    (load_chosen_weather(args), None)
}

/// A chosen weather: a hand-set one, a METAR report, an `.owt` file.
fn load_chosen_weather(args: &Args) -> omsi_content::weather::Weather {
    let rel = args
        .weather
        .clone()
        .filter(|w| !crate::weather_cycle::is_cycle(Some(w)))
        .unwrap_or_else(|| "Weather/#CAVOK.owt".into());
    if let Some(w)=CustomWeather::parse(&rel).map(|c|c.to_weather()){
        log::info!("weather custom: fog range {} m, precip {:?}, temp {:?}",w.fog.0,w.precip,w.temp);
        omsi_sim::host::set_ambient_weather(w.temp.0,w.temp.1);
        crate::weather_setup::publish_page_weather(&w);
        return w;
    }
    // OMSI 2's current weather: `metar:<ICAO>` fetches the airport's report
    let loaded = if rel.starts_with(REPORT) {
        // a report the host or server tells (see `report_wire`): its values, no download
        Ok(from_report(&rel).unwrap_or_else(|| omsi_content::weather::from_metar("", "CAVOK")))
    } else {
        match rel.strip_prefix("metar:").or_else(|| rel.strip_prefix("METAR:")) {
            Some(icao) => Ok(fetch_metar(icao.trim())),
            None => omsi_content::weather::Weather::load(&omsi_cfg::resolve_path(&args.root, &rel)),
        }
    };
    match loaded {
        Ok(w) => {
            log::info!(
                "weather {}: fog range {} m, precip {:?}, temp {:?}",
                w.name,
                w.fog.0,
                w.precip,
                w.temp
            );
            let mut w = w;
            // No weather chosen: the clear default (#CAVOK, no cloud at all) gets a few fair-
            // weather cumulus clouds, as long as clouds are wanted - a sky without a single
            // cloud was the first thing that looked wrong.
            if args.weather.is_none() && args.session_settings.clouds && w.clouds.0.trim().starts_with("-1") {
                w.clouds = ("Cumulus 1".into(), 100.0);
            }
            // the vehicles' {init} reads the temperature
            omsi_sim::host::set_ambient_weather(w.temp.0, w.temp.1);
            crate::weather_setup::publish_page_weather(&w);
            w
        }
        Err(e) => {
            log::warn!("weather {rel}: {e}");
            omsi_content::weather::Weather {
                fog: (50000.0, 1.0),
                ..Default::default()
            }
        }
    }
}

/// Sky gradient textures from envir.cfg uploaded into the scene.
pub(crate) fn setup_sky(
    args: &Args,
    renderer: &Renderer,
    scene: &mut Scene,
    envir: Option<&omsi_content::Envir>,
    weather: Option<&omsi_content::weather::Weather>,
    sky_clouds: &mut SkyClouds,
) {
    const STOCK: [&str; 3] = ["Texture\\himmel01.bmp", "Texture\\himmel04.bmp", "Texture\\himmel05.bmp"];
    let names = envir.map(|e| e.sky_textures.clone()).unwrap_or_else(|| STOCK.map(String::from));
    let mut ids = Vec::new();
    for (n, stock) in names.iter().zip(STOCK) {
        let p = omsi_cfg::resolve_path(&args.root, n);
        // a sky pack's picture that cannot be read leaves the stock one in its place: giving
        // up here took the clouds with it, whatever the weather (#749)
        let img = omsi_texture::decode_file(&p).or_else(|e| {
            log::warn!("sky texture {}: {e}", p.display());
            omsi_texture::decode_file(&omsi_cfg::resolve_path(&args.root, stock))
        });
        match img {
            Ok(img) => ids.push(renderer.add_texture(scene, &img, false)),
            Err(e) => {
                log::warn!("sky texture {stock}: {e}");
                return;
            }
        }
    }
    // the weather's cloud type (Weather/clouds.cfg: Cumulus 1..3, Overcast 1) with its own
    // texture, whose alpha is the clouds' shape; `Texture\clouds.tga` when there is none
    let kind = weather.map(|w| w.clouds.0.trim().to_string()).unwrap_or_default();
    let typed = cloud_texture(&args.root, &kind);
    let cover = typed.or_else(|| omsi_texture::decode_file(&omsi_cfg::resolve_path(&args.root, "Texture\\clouds.tga")).ok());
    let t = std::time::Instant::now();
    let field = cloud_field(cover.as_ref());
    log::debug!("cloud field made in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    let clouds = Some(renderer.add_texture_data(scene, &cloud_field_levels(field)));
    // the classic sky draws the cloud type's texture itself, its alpha the cover (the
    // renderer's mip chain weighs the colour by it, as for any cut-out texture)
    let ty = cloud_type(&args.root, &kind);
    let vanilla = ty.as_ref().and_then(|t| {
        omsi_texture::decode_file(&omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&args.root, "Texture"), &t.texture))
            .map_err(|e| log::warn!("cloud texture {}: {e}", t.texture))
            .ok()
    });
    *sky_clouds = ty.filter(|_| vanilla.is_some()).map(|t| (kind.clone(), t));
    let vanilla = vanilla.map(|img| renderer.add_texture(scene, &img, true));
    let sun = omsi_texture::decode_file(&omsi_cfg::resolve_path(&args.root, "Texture\\sonne.bmp"))
        .map_err(|e| log::warn!("sun texture: {e}"))
        .ok()
        .map(|img| renderer.add_texture(scene, &img, false));
    renderer.set_sky_textures_vanilla(scene, [ids[0], ids[1], ids[2]], clouds, vanilla, sun);
}

/// A `[cloudtype]` of `Weather/clouds.cfg`: its texture (under `Texture\`), the metres of
/// ground a tile of it covers, and whether it is an `ovc` deck (else `sct`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CloudType {
    pub texture: String,
    pub size: f32,
    pub overcast: bool,
}

/// The cloud type the sky was last set up with (its name and entry): the classic sky's
/// cloud layer is drawn by it while the weather names it.
pub(crate) type SkyClouds = Option<(String, CloudType)>;

/// What a scene's sky is drawn from besides the weather itself, kept by whoever owns the
/// scene (the window's session, the offscreen run, the showroom's look) and handed to
/// `weather_lighting`: the cloud type `setup_sky` set its textures up with, and the
/// physical weather model when the weather is the model's (`load_weather_run`).
#[derive(Default)]
pub(crate) struct SkyState {
    pub(crate) clouds: SkyClouds,
    pub(crate) model: Option<crate::weather_model::ModelRun>,
    /// The graphics' kind and whether clouds are wanted (see `startup::SessionSettings`).
    pub(crate) look: crate::startup::SessionSettings,
}

/// The `[cloudtype]` entry named `kind` (`-1` or empty: none), read as Omsi.exe reads it
/// (0x754664): name, texture, size, then `ovc` or `sct`.
pub(crate) fn cloud_type(root: &Path, kind: &str) -> Option<CloudType> {
    if kind.is_empty() || kind.starts_with("-1") {
        return None;
    }
    let cfg = omsi_cfg::vfs::read(&root.join("Weather").join("clouds.cfg")).ok()?;
    cloud_type_in(&omsi_cfg::codepage::decode(&cfg), kind)
}

fn cloud_type_in(text: &str, kind: &str) -> Option<CloudType> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    (0..lines.len()).find_map(|i| {
        if !lines[i].eq_ignore_ascii_case("[cloudtype]") || !lines.get(i + 1).is_some_and(|n| n.eq_ignore_ascii_case(kind)) {
            return None;
        }
        Some(CloudType {
            texture: lines.get(i + 2)?.to_string(),
            size: lines.get(i + 3).and_then(|v| v.parse::<f32>().ok()).filter(|v| *v > 0.0).unwrap_or(2000.0),
            overcast: lines.get(i + 4).is_some_and(|v| v.eq_ignore_ascii_case("ovc")),
        })
    })
}

/// Edge of the cloud field texture (texels); it tiles.
const FIELD: usize = 512;

/// The texture both sky shaders draw their clouds from, seamless in both directions:
/// R the weather's own cloud picture (Weather/clouds.cfg) - where its clouds are, the sky
/// has more; G the shape of the cumulus, fractal noise bent by more noise, equalised so
/// that a threshold of 1 - f covers exactly the fraction f of the sky; B billows for the
/// cauliflower edges; A how tall each cloud grows. Stored in sRGB bytes so that the
/// shader reads the values back as they were made.
pub(crate) fn cloud_field(cover: Option<&omsi_texture::Image>) -> omsi_texture::Image {
    let n = FIELD;
    let lattice = |x: i64, y: i64, period: i64, seed: u32| -> f32 {
        let (x, y) = (x.rem_euclid(period) as u32, y.rem_euclid(period) as u32);
        let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 65535.0
    };
    // value noise with `period` cells across the tile, at a point of the tile (0..1)
    let noise = |u: f32, v: f32, period: i64, seed: u32| -> f32 {
        let (x, y) = (u * period as f32, v * period as f32);
        let (ix, iy) = (x.floor() as i64, y.floor() as i64);
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let a = lattice(ix, iy, period, seed) + (lattice(ix + 1, iy, period, seed) - lattice(ix, iy, period, seed)) * sx;
        let b = lattice(ix, iy + 1, period, seed) + (lattice(ix + 1, iy + 1, period, seed) - lattice(ix, iy + 1, period, seed)) * sx;
        a + (b - a) * sy
    };
    let fbm = |u: f32, v: f32, period: i64, octaves: u32, seed: u32| -> f32 {
        let (mut sum, mut amp, mut norm, mut p) = (0.0, 0.5, 0.0, period);
        for o in 0..octaves {
            sum += amp * noise(u, v, p, seed + o * 101);
            norm += amp;
            amp *= 0.5;
            p *= 2;
        }
        sum / norm
    };
    let mut shape = vec![0f32; n * n];
    let mut billow = vec![0f32; n * n];
    let mut tall = vec![0f32; n * n];
    // (rows in parallel: 512 x 512 texels of a few octaves each)
    use rayon::prelude::*;
    shape
        .par_chunks_mut(n)
        .zip(billow.par_chunks_mut(n))
        .zip(tall.par_chunks_mut(n))
        .enumerate()
        .for_each(|(y, ((s_row, b_row), t_row))| {
            let v = y as f32 / n as f32;
            for x in 0..n {
                let u = x as f32 / n as f32;
                // bend the cumulus field with a coarser one, so that clouds are not blobs on a grid
                let wu = fbm(u, v, 4, 3, 11) - 0.5;
                let wv = fbm(u, v, 4, 3, 23) - 0.5;
                s_row[x] = fbm(u + wu * 0.12, v + wv * 0.12, 6, 5, 37);
                let mut b = 0.0;
                let mut amp = 0.5;
                let mut p = 24;
                for o in 0..3 {
                    b += amp * (1.0 - (noise(u, v, p, 53 + o) * 2.0 - 1.0).abs());
                    amp *= 0.5;
                    p *= 2;
                }
                b_row[x] = b / 0.875;
                t_row[x] = fbm(u, v, 3, 2, 71);
            }
        });
    // equalise the shape: its rank, so that a threshold is a fraction of the sky
    let mut order: Vec<u32> = (0..(n * n) as u32).collect();
    order.par_sort_unstable_by(|a, b| shape[*a as usize].total_cmp(&shape[*b as usize]));
    let mut equal = vec![0f32; n * n];
    for (rank, &i) in order.iter().enumerate() {
        equal[i as usize] = rank as f32 / (n * n - 1) as f32;
    }
    let stretch = |v: &mut Vec<f32>| {
        let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        for x in v.iter_mut() {
            *x = (*x - lo) / (hi - lo).max(1e-6);
        }
    };
    stretch(&mut billow);
    stretch(&mut tall);
    let to_srgb = |x: f32| -> u8 {
        let x = x.clamp(0.0, 1.0);
        let s = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0 + 0.5) as u8
    };
    // the weather's picture, sampled bilinearly (it tiles as well), its brightness the cover
    let weather = |u: f32, v: f32| -> f32 {
        let Some(img) = cover else { return 0.5 };
        let (w, h) = (img.width as i64, img.height as i64);
        let (x, y) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
        let (ix, iy) = (x.floor() as i64, y.floor() as i64);
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let px = |xx: i64, yy: i64| {
            let i = ((yy.rem_euclid(h) * w + xx.rem_euclid(w)) * 4) as usize;
            (img.rgba[i] as f32 + img.rgba[i + 1] as f32 + img.rgba[i + 2] as f32) / (3.0 * 255.0)
        };
        let a = px(ix, iy) + (px(ix + 1, iy) - px(ix, iy)) * fx;
        let b = px(ix, iy + 1) + (px(ix + 1, iy + 1) - px(ix, iy + 1)) * fx;
        a + (b - a) * fy
    };
    let mut rgba = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let o = i * 4;
            rgba[o] = to_srgb(weather(x as f32 / n as f32, y as f32 / n as f32));
            rgba[o + 1] = to_srgb(equal[i]);
            rgba[o + 2] = to_srgb(billow[i]);
            // (alpha is not sRGB-coded)
            rgba[o + 3] = (tall[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    omsi_texture::Image { width: n as u32, height: n as u32, rgba, has_alpha: true }
}

/// The cloud field with its mip chain, each level the plain mean of four texels of the one
/// above. The renderer's own chain (`add_texture`) weighs a texel's colour by its alpha, which
/// keeps the colour of a transparent texel out of a leaf's silhouette - but the field's
/// alpha is how tall the cloud grows, not a coverage: weighed by it, the cumulus shape (G)
/// of a smaller level was the shape of its tall clouds alone, so the clouds changed their
/// outline from one level to the next, and a cloud on a low patch vanished where the sky
/// went over to the next level - a visible line across it. The colour channels hold linear
/// values in sRGB bytes (see `cloud_field`), so they are averaged decoded, as the GPU reads them.
pub(crate) fn cloud_field_levels(field: omsi_texture::Image) -> omsi_texture::TextureData {
    let decode = |b: u8| -> f32 {
        let s = b as f32 / 255.0;
        if s <= 0.040_45 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    let encode = |x: f32| -> u8 {
        let x = x.clamp(0.0, 1.0);
        let s = if x <= 0.003_130_8 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
        (s * 255.0 + 0.5) as u8
    };
    let (w, h) = (field.width as usize, field.height as usize);
    let mut levels = vec![field.rgba];
    let (mut lw, mut lh) = (w, h);
    while lw > 1 || lh > 1 {
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let prev = levels.last().expect("level");
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                // (the field tiles: a texel past an odd edge wraps round)
                let px = [(2 * x) % lw, (2 * x + 1) % lw];
                let py = [(2 * y) % lh, (2 * y + 1) % lh];
                let texels = [(px[0], py[0]), (px[1], py[0]), (px[0], py[1]), (px[1], py[1])];
                for c in 0..4 {
                    let sum: f32 = texels
                        .iter()
                        .map(|&(tx, ty)| {
                            let v = prev[(ty * lw + tx) * 4 + c];
                            if c < 3 { decode(v) } else { v as f32 / 255.0 }
                        })
                        .sum();
                    let mean = sum / 4.0;
                    next[(y * nw + x) * 4 + c] = if c < 3 { encode(mean) } else { (mean * 255.0 + 0.5) as u8 };
                }
            }
        }
        levels.push(next);
        lw = nw;
        lh = nh;
    }
    omsi_texture::TextureData { width: w as u32, height: h as u32, format: omsi_texture::PixelFormat::Rgba8, levels, has_alpha: true, gpu_mips: false }
}

/// The texture of a cloud type named in `Weather/clouds.cfg` (`[cloudtype]` name, texture,
/// size in metres, sct/ovc), as the sky shaders read it: the cover in the colour channels.
/// A scattered type's texture is white with the clouds in its alpha; an overcast one is a
/// picture of the cloud deck, its brightness is the cover.
fn cloud_texture(root: &Path, kind: &str) -> Option<omsi_texture::Image> {
    let file = cloud_type(root, kind)?.texture;
    let mut img = omsi_texture::decode_file(&omsi_cfg::resolve_path(&omsi_cfg::resolve_path(&root, "Texture"), &file)).ok()?;
    if img.has_alpha {
        for px in img.rgba.chunks_mut(4) {
            let a = px[3];
            px[0] = a;
            px[1] = a;
            px[2] = a;
            px[3] = 255;
        }
        img.has_alpha = false;
    }
    log::info!("clouds: {kind} ({file})");
    Some(img)
}

/// Cloud cover of a weather file: `[clouds] type density`, type -1 = clear, density up to
/// ~300 (Cumulus 3) - mapped to 0..1; the cover drifts with the wind.
pub(crate) fn clouds_of(w: &omsi_content::weather::Weather, drift: [f64; 4], clouds: bool) -> (f32, [f32; 2]) {
    if !clouds {
        return (0.0, [0.0; 2]);
    }
    // the cover decides, not the name (`cover_of`: a hand-set weather's own, else its type's)
    (cover_of(w), [drift[0] as f32, drift[1] as f32])
}



/// The classic sky's weather (see `omsi_render::VanillaSky`).
pub(crate) fn vanilla_sky(w: &omsi_content::weather::Weather, drift: [f64; 4], sky_clouds: &SkyClouds, clouds: bool) -> omsi_render::VanillaSky {
    // the visibility Omsi.exe fogs with (0x753120): the fog range, shortened by rain to
    // 100000 m / its intensity and by snow to 20000 m / its intensity
    let range = w.fog.0.max(1.0);
    let intensity = w.precip.get(1).copied().unwrap_or(0.0).max(1.0);
    let visibility = match w.precip.first().copied().unwrap_or(0.0) as i32 {
        1 => range.min(100_000.0 / intensity),
        2 => range.min(20_000.0 / intensity),
        _ => range,
    };
    let kind = w.clouds.0.trim();
    let ty = sky_clouds
        .as_ref()
        .filter(|(k, _)| k.eq_ignore_ascii_case(kind) && clouds)
        .map(|(_, t)| t.clone());
    omsi_render::VanillaSky {
        cloud_height: w.clouds.1,
        fog_range: w.fog.0,
        visibility,
        cloud_size: ty.as_ref().map_or(0.0, |t| t.size),
        overcast: ty.as_ref().is_some_and(|t| t.overcast),
        cloud_offset: [drift[2] as f32, drift[3] as f32],
    }
}


/// The renderer's lighting for this weather at this moment: the daylight, then what the
/// cloud cover, the rain and the snow make of it.
pub(crate) fn weather_lighting(
    daylight: &omsi_sim::Daylight,
    w: &omsi_content::weather::Weather,
    cloud_drift: [f64; 4],
    wetness: f32,
    snow_cover: f32,
    shadows: bool,
    sky: &SkyState,
) -> omsi_render::Lighting {
    let mut lighting = lights::lighting_from(daylight, w.fog.0);
    lighting.enhanced = sky.look.enhanced;
    lighting.classic = sky.look.classic;
    let (density, offset) = clouds_of(w, cloud_drift, sky.look.clouds);
    lighting.cloud_density = density;
    lighting.cloud_offset = offset;
    lighting.vanilla_sky = vanilla_sky(w, cloud_drift, &sky.clouds, sky.look.clouds);
    let model_sky = sky.model.as_ref().map(|m| m.sky());
    let (kind, rate) = precip_of(w);
    lights::apply_weather(
        &mut lighting,
        density,
        kind,
        rate,
        // (the weather's own snow cover, or what the snowfalls have laid so far)
        if w.snow { 1.0 } else { snow_cover },
    );
    // (the roads are kept clear unless the weather says the snow lies on them: the traffic
    // and the gritters keep them black while the roofs and the verges turn white)
    lighting.roads_clear = !w.snow_on_road;
    // a thunderstorm: towering clouds over a warm shower (thunder in a snowfall is a rarity
    // not worth making up); the physical model says how active it is itself, below
    {
        let c = w.clouds.0.to_ascii_lowercase();
        let thundery = (c.contains("cumulus 3") || c.contains("cumulonimbus") || c.contains("gewitter") || c.contains("thunder")) && kind == 1 && rate > 0.45 && w.temp.0 > 8.0;
        lighting.storm = if thundery { ((rate - 0.25) * 1.3).clamp(0.3, 1.0) } else { 0.0 };
    }
    if let Some(custom)=CustomWeather::parse(&w.path.to_string_lossy()){
        let k=custom.brightness;
        lighting.sun_intensity*=k;
        // (Enhanced: the automatic exposure takes the picture back up to the eye's level
        // whatever the weather's brightness, so there it only takes the sun away. The
        // mirrors, drawn with the plain shading and graded by no exposure, went dark with
        // the light instead - black at 0 % beside a window still in daylight, #1018.)
        if !lighting.enhanced {
            lighting.secondary*=k;
            lighting.ambient*=k;
            lighting.sky_color*=k;
            lighting.fog_color*=k;
        }
    }
    lighting.wetness = wetness;
    // [wind] direction (deg) and speed (m/s): the snowfall drifts with it
    lighting.wind = glam::Vec3::new(w.wind.0.to_radians().sin() * w.wind.1, w.wind.0.to_radians().cos() * w.wind.1, 0.0);
    omsi_sim::particles::set_wind(lighting.wind);
    // Omsi.exe hides the sun under an 'ovc' cloud type (the Overcast ones in clouds.cfg) and
    // draws no sun shadows below 350 m visibility
    let overcast = w.clouds.0.trim().to_ascii_lowercase().starts_with("overcast");
    lighting.shadows = shadows && !overcast && w.fog.0 > 350.0;
    // (a street lamp casts its shadow in any weather)
    lighting.lamp_shadows = shadows;
    // the physical model: how much of which cloud there is and what the air holds, which
    // the enhanced atmosphere turns into light (the `.owt` values above stay for the rest)
    if let Some(m) = model_sky.filter(|_| lighting.enhanced) {
        let closed = ((m.deck - 0.85) / 0.15).clamp(0.0, 1.0);
        if sky.look.clouds {
            lighting.cloud_density = m.cumulus.max(m.deck * 0.95);
        }
        lighting.overcast = m.deck;
        lighting.sun_intensity = 1.0 - closed;
        lighting.veil = m.veil;
        lighting.air = Some([m.haze, m.angstrom, m.aerosol_height]);
        lighting.cloud_layer = Some([m.cu_base, m.cu_top]);
        lighting.alto = m.alto;
        lighting.deck_tau = Some(m.deck_tau);
        lighting.storm = m.storm;
        lighting.shadows = shadows && closed < 0.5 && w.fog.0 > 350.0;
    }
    lighting
}


/// Tell the htmltexture pages the weather (`omsi.weather`), with the temperature the
/// vehicles get.
pub(crate) fn publish_page_weather(w: &omsi_content::weather::Weather) {
    let (kind, rate) = precip_of(w);
    omsi_sim::vehicle_api::set_page_weather(omsi_sim::vehicle_api::PageWeather {
        temperature: w.temp.0,
        abs_humidity: w.temp.1,
        visibility: w.fog.0,
        clouds: w.clouds.0.clone(),
        precip: (kind as f32, rate * 255.0),
    });
}

pub(crate) fn apply_weather(
    v: &mut omsi_sim::VehicleInstance,
    w: &omsi_content::weather::Weather,
    wetness: f32,
) {
    let (kind, rate) = precip_of(w);
    v.host.precip_type = kind as f32;
    v.host.precip_rate = rate;
    v.host.wind = crate::rain::weather_wind(w);
    v.host.street_cond = street_condition(w, wetness);
    v.set_var("PrecipType", kind as f32);
    v.set_var("PrecipRate", rate);
    v.host.temperature = w.temp.0;
    v.host.abs_humidity = w.temp.1;
    omsi_sim::host::set_ambient_weather(w.temp.0, w.temp.1);
    crate::weather_setup::publish_page_weather(&w);
}

/// `OMSI_DEBUG_SOUND[=seconds]`: how often the mixer says what every sound entry of the
/// player's bus is doing, and what the environment sounds play (5 s by default).
pub(crate) fn debug_sound_every() -> Option<f32> {
    static EVERY: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *EVERY.get_or_init(|| {
        omsi_cfg::flags::OMSI_DEBUG_SOUND
            .var()
            .map(|v| v.parse::<f32>().ok().filter(|s| *s > 0.0).unwrap_or(5.0))
    })
}





/// The weather of an airport's METAR report (aviationweather.gov), or a clear day when it
/// cannot be had (no network, an unknown station).
pub(crate) fn fetch_metar(icao: &str) -> omsi_content::weather::Weather {
    match try_metar(icao) {
        Some(w) => w,
        None => {
            log::warn!("current weather at {icao}: no METAR report could be had; a clear day instead");
            omsi_content::weather::from_metar(icao, "CAVOK")
        }
    }
}

/// The weather of an airport's METAR report, None when it cannot be had.
pub(crate) fn try_metar(icao: &str) -> Option<omsi_content::weather::Weather> {
    // (Tegel, OMSI's Berlin default, closed in 2020: Berlin's airport now reports)
    let icao = match icao.to_ascii_uppercase().as_str() {
        "EDDT" | "EDDI" | "" => "EDDB".to_string(),
        other => other.to_string(),
    };
    let url = format!("https://aviationweather.gov/api/data/metar?ids={icao}&format=raw");
    let text = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .map(|t| t.lines().next().unwrap_or("").trim().to_string())
        .filter(|t| !t.is_empty());
    let t = text?;
    log::info!("current weather at {icao}: {t}");
    let mut w = omsi_content::weather::from_metar(&icao, &t);
    w.path = std::path::PathBuf::from(format!("metar:{icao}"));
    Some(w)
}

/// The start of a weather text that carries a METAR report's values (not a file, not a
/// download): what a host or server with the METAR sync tells the players, who make the
/// weather from it themselves and need no sync of their own.
pub(crate) const REPORT: &str = "metar-report:";

/// `w` (made from a METAR report) as the text the session tells the players: the station and
/// the report up to its forecast, which `from_metar` does not read either.
pub(crate) fn report_wire(w: &omsi_content::weather::Weather) -> Option<String> {
    let path = w.path.to_string_lossy();
    let icao = path.strip_prefix("metar:")?;
    let mut raw: Vec<&str> = Vec::new();
    for t in w.description.split_whitespace() {
        if matches!(t.trim_end_matches('='), "TEMPO" | "BECMG" | "NOSIG" | "RMK" | "PROB30" | "PROB40") {
            break;
        }
        raw.push(t);
    }
    let raw = raw.join(" ");
    if icao.is_empty() || raw.is_empty() {
        return None;
    }
    // (the network's weather field holds 260 characters)
    Some(format!("{REPORT}{icao} {raw}").chars().take(250).collect())
}

/// The weather of a text made by `report_wire`.
pub(crate) fn from_report(s: &str) -> Option<omsi_content::weather::Weather> {
    let (icao, raw) = s.trim().strip_prefix(REPORT)?.split_once(' ')?;
    if !(3..=5).contains(&icao.len()) || !icao.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let mut w = omsi_content::weather::from_metar(icao, raw);
    w.path = std::path::PathBuf::from(format!("metar:{icao}"));
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tomorrow_starts_with_fallback_without_a_natural_model() {
        let args = Args::try_parse_from(["openomsi", "--weather", "tomorrow"]).unwrap();
        let (weather, model) = load_weather_run(&args);
        assert!(model.is_none(), "natural weather must not overwrite observations");
        assert_eq!(weather.name, "Custom weather");
        assert!(!weather.snow);
        assert!(weather.temp.0.is_finite());
    }

    #[test]
    fn the_weather_drifts_the_classic_clouds_over_the_renderers_tiling() {
        assert_eq!(omsi_weather::conditions::CLOUD_PERIOD, omsi_render::VANILLA_CLOUD_PERIOD);
    }

    #[test]
    fn cloud_types_are_read_as_omsi_reads_them() {
        let cfg = "\tHier werden alle Wolkentypen definiert:\n\n\t[cloudtype]\n\tname\n\ttexture\n\n[cloudtype]\nCumulus 2\nCumulus_2.tga\n2000\nsct\n\n[cloudtype]\nOvercast 1\nOVC_1.bmp\n1000\novc\n";
        assert_eq!(cloud_type_in(cfg, "cumulus 2"), Some(CloudType { texture: "Cumulus_2.tga".into(), size: 2000.0, overcast: false }));
        assert_eq!(cloud_type_in(cfg, "Overcast 1"), Some(CloudType { texture: "OVC_1.bmp".into(), size: 1000.0, overcast: true }));
        assert_eq!(cloud_type_in(cfg, "Cumulus 9"), None);
    }

    #[test]
    fn the_classic_clouds_drift_downwind_by_omsis_angle() {
        let w = omsi_content::weather::Weather { wind: (100.0, 5.0), ..Default::default() };
        let d = cloud_drift_at(&w, 10.0);
        // 100 degrees taken as 100 x pi/200: due east, 50 m in 10 s
        assert!((d[2] - 50.0).abs() < 1e-3 && d[3].abs() < 1e-3, "{d:?}");
    }

    /// An older `custom:` text kept the index of a cloud type in `c=` as a whole number,
    /// where the cover is written with three decimals: `c=1` reads back as Cumulus 1's cover,
    /// not as the 1.0 of a closed deck.
    #[test]
    fn an_old_custom_texts_cloud_index_reads_back_as_its_kinds_cover() {
        for (index, cover) in [0.0, 0.35, 0.55, 0.75, 1.0].into_iter().enumerate() {
            let text = format!("custom:vis=20000;br=1.00;c={index}");
            let c = CustomWeather::parse(&text).expect(&text);
            assert!((c.cloud_cover - cover).abs() < 1e-6, "{text}: {} not {cover}", c.cloud_cover);
        }
        for (text, cover) in [("custom:vis=20000;c=0.000", 0.0), ("custom:vis=20000;c=0.550", 0.55), ("custom:vis=20000;c=1.000", 1.0)] {
            let c = CustomWeather::parse(text).expect(text);
            assert!((c.cloud_cover - cover).abs() < 1e-6, "{text}: {}", c.cloud_cover);
        }
    }

    /// What `encode` writes parses back to the same cover, and the weather it makes names a
    /// kind with clouds for any cover above none.
    #[test]
    fn a_custom_weathers_cover_survives_its_own_text() {
        let c = CustomWeather { cloud_cover: 0.12, ..CustomWeather::default() };
        let back = CustomWeather::parse(&c.encode()).expect("its own text");
        assert!((back.cloud_cover - 0.12).abs() < 1e-6, "{}", back.cloud_cover);
        let w = back.to_weather();
        assert_eq!(w.clouds.0, "Cumulus 1");
        assert!((cover_of(&w) - 0.12).abs() < 1e-6);
        assert_eq!(cover_of(&CustomWeather::default().to_weather()), 0.0);
    }

    #[test]
    fn the_cloud_kinds_are_found_by_every_name() {
        assert_eq!(cloud_kind_index("Overcast 1"), Some(4));
        assert_eq!(cloud_kind_index("overcast"), Some(4));
        assert_eq!(cloud_kind_index("Broken"), Some(3));
        assert_eq!(cloud_kind_index("-1"), Some(0));
        assert_eq!(cloud_kind_index("Cirrus 7"), None);
        assert_eq!(cloud_kind_nearest(0.01), 1);
        assert_eq!(cloud_kind_nearest(0.0), 0);
        assert_eq!(cloud_kind_nearest(0.9), 4);
    }
}
