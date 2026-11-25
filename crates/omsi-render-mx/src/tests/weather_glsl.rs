use super::*;

#[test]
fn the_rain_and_snow_shaders_translate_to_glsl_with_a_float_roof_map() {
    use naga;
    use naga::back::glsl;
    let glsl_of = |src: &str| -> Result<(), String> {
        let module = naga::front::wgsl::parse_str(&crate::shaders::with_heap(src))
            .map_err(|e| e.emit_to_string(src))?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| format!("{e:?}"))?;
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &module,
            &info,
            None,
            &Default::default(),
        )
        .map_err(|e| format!("{e:?}"))?;
        for version in [
            glsl::Version::Embedded {
                version: 310,
                is_webgl: false,
            },
            glsl::Version::Desktop(430),
        ] {
            let options = glsl::Options {
                version,
                ..Default::default()
            };
            for entry in &module.entry_points {
                let pipeline = glsl::PipelineOptions {
                    shader_stage: entry.stage,
                    entry_point: entry.name.clone(),
                    multiview: None,
                };
                let mut out = String::new();
                glsl::Writer::new(
                    &mut out,
                    &module,
                    &info,
                    &options,
                    &pipeline,
                    Default::default(),
                )
                .and_then(|mut w| w.write())
                .map_err(|e| format!("{version:?} {}: {e:?}", entry.name))?;
            }
        }
        Ok(())
    };
    let depth = |src: String| {
        src.replace(
            "var roof_map: texture_2d<f32>;",
            "var roof_map: texture_depth_2d;",
        )
        .replace(
            "return textureLoad(roof_map, t, 0).x; }",
            "return textureLoad(roof_map, t, 0); }",
        )
    };
    for src in [
        rain_shader_source(),
        snow_shader_source(),
        lean_rain(rain_shader_source()),
        lean_snow(snow_shader_source()),
    ]
    .map(depth)
    {
        // (mX's naga loads a depth texture in GLSL too since mX 650b244; the float roof map
        // stays the form the GL devices draw with)
        glsl_of(&src).unwrap();
        glsl_of(&roof_as_float(src)).unwrap();
    }
}
