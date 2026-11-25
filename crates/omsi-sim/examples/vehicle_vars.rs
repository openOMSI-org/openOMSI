//! Trace script variables of a vehicle.
//! usage: vehicle_vars <root> <bus> <frames> var1,var2,... [trigger1,trigger2@frame,...] [throttle]
//! A trigger written as `name@N` fires at frame N (default 0).
use std::path::Path;
fn main() {
    omsi_sim::image_decode::install(decode_picture);
    let a: Vec<String> = std::env::args().collect();
    let root = Path::new(&a[1]);
    omsi_cfg::add_content_root(root.to_path_buf());
    let archives = root.join("Archives");
    if archives.is_dir() {
        omsi_cfg::vfs::mount_dir_zips(&archives);
    }
    let orig_dir = omsi_cfg::flags::OMSI_ORIGINAL
        .live_os()
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    let orig = orig_dir.as_path();
    if orig.is_dir() {
        omsi_cfg::add_content_root(orig.to_path_buf());
    }
    let vt = std::sync::Arc::new(
        omsi_sim::VehicleType::load(root, &omsi_cfg::resolve_path(root, &a[2])).unwrap(),
    );
    let frames: usize = a[3].parse().unwrap();
    let vars: Vec<&str> = a[4].split(',').collect();
    let triggers: Vec<(String, usize)> = a
        .get(5)
        .map(|t| {
            t.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| match s.split_once('@') {
                    Some((n, f)) => (n.to_string(), f.parse().unwrap_or(0)),
                    None => (s.to_string(), 0),
                })
                .collect()
        })
        .unwrap_or_default();
    let throttle: f32 = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let mut v =
        omsi_sim::VehicleInstance::new(vt.clone(), omsi_sim::VehicleHost::new(Default::default()));
    // optional audio: AUDIO=1 attaches the vehicle's sound config and reports voices
    let audio = std::env::var_os("AUDIO").map(|_| omsi_audio::AudioEngine::new());
    let mut sounds = audio.as_ref().and_then(|a| {
        let rel = vt.def.sound.clone()?;
        let path = omsi_cfg::resolve_path(vt.def.dir(), &rel);
        let cfg = omsi_vehicle::SoundCfg::load(&path).ok()?;
        Some(omsi_audio::SoundSet::new(a, &cfg, path.parent().unwrap()))
    });
    let show = |v: &omsi_sim::VehicleInstance, tag: &str| {
        let vals: Vec<String> = vars
            .iter()
            .map(|n| {
                format!(
                    "{n}={}",
                    v.var(n).map(|x| format!("{x:.3}")).unwrap_or("?".into())
                )
            })
            .collect();
        println!("{tag}: {}", vals.join("  "));
    };
    show(&v, "after init");
    for i in 0..frames {
        for (t, f) in &triggers {
            if *f == i {
                if let Some((name, val)) = t.split_once('=') {
                    if !v.set_var(name, val.parse().unwrap_or(0.0)) {
                        println!("  variable {name} not found");
                    }
                } else if !v.trigger(t) {
                    println!("  trigger {t} not found");
                }
            }
        }
        v.set_controls(omsi_sim::Controls {
            throttle,
            ..Default::default()
        });
        v.update(1.0 / 30.0);
        if let (Some(a), Some(ss)) = (audio.as_ref(), sounds.as_mut()) {
            let fired = std::mem::take(&mut v.host.fired_triggers);
            let xf = v.world_transform();
            ss.update(a, &|n| v.var(n), &xf, &fired);
            if i % 30 == 0 {
                println!("   audio voices: {}", a.voice_count());
            }
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        if i < 5 || i % 30 == 0 || i + 1 == frames {
            show(
                &v,
                &format!(
                    "frame {i} ({:.1} s) speed {:.1} km/h",
                    i as f32 / 30.0,
                    v.physics.velocity_kmh()
                ),
            );
        }
    }
    println!(
        "sound triggers: {:?}",
        v.host
            .fired_triggers
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
    );
    println!("messages: {:?}", v.host.messages);
}

/// The game's picture decoder for the simulation (as omsi-app registers it at start).
fn decode_picture(path: &std::path::Path) -> Result<omsi_sim::image_decode::DecodedImage, String> {
    omsi_texture::decode_file(path)
        .map(|i| omsi_sim::image_decode::DecodedImage {
            width: i.width,
            height: i.height,
            rgba: i.rgba,
            has_alpha: i.has_alpha,
        })
        .map_err(|e| e.to_string())
}
