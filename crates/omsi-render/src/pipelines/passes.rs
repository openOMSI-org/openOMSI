//! The main pass's pipelines for each of the three ways the scene is drawn: vanilla into
//! the target, enhanced into the high-range picture, and vanilla with reflections.

use super::coronas::{Coronas, Precip, ADDITIVE, ALPHA_BLEND, SCREEN};
use super::parallel::{compile_all, Job};
use super::scene::SceneBase;
use super::sky::SkyBase;
use crate::*;

/// What the main pass's pipelines are made from.
pub(crate) struct PassKit<'a> {
    pub device: &'a wgpu::Device,
    pub scene: &'a SceneBase,
    pub coronas: &'a Coronas,
    pub snow: &'a Precip,
    pub rain: &'a Precip,
    pub sky: &'a SkyBase,
    pub msaa: u32,
}

/// The fragment entry points (and the coronas' blending) of one way of drawing.
#[derive(Clone, Copy)]
struct PassShaders<'a> {
    scene: &'a str,
    corona: &'a str,
    corona_blend: wgpu::BlendState,
    smoke: &'a str,
    snow: &'a str,
    rain: &'a str,
    sky: &'a str,
}

pub(crate) struct Passes {
    pub pass: PassPipelines,
    pub hdr_pass: Option<PassPipelines>,
    pub sky_mirror_pipeline: Option<wgpu::RenderPipeline>,
    pub reflection_pass: Option<PassPipelines>,
    pub leave_out_enhanced: bool,
}

/// Decide before making shader modules and cloud/probe resources, not only before the
/// draw pipelines: WGSL validation still processes unused entry points, and the probe's
/// unused pipelines would invoke the native shader compiler.
pub(crate) fn leave_out_enhanced(options: &RenderOptions, adapter_name: &str) -> bool {
    options.preview_only
        || sixteen_texture_units()
        || (options.no_enhanced
            && (cfg!(target_os = "android")
                || adapter_name.to_ascii_lowercase().contains("opengl")
                || gl_backend()))
}

type Made = Option<wgpu::RenderPipeline>;

fn one(made: &mut impl Iterator<Item = Made>) -> wgpu::RenderPipeline {
    made.next().flatten().expect("a pipeline of the main pass")
}

fn take(made: &mut impl Iterator<Item = Made>) -> PassPipelines {
    PassPipelines {
        pipelines: (0..SceneBase::PIPELINES).map(|_| one(made)).collect(),
        rain_pipelines: (0..SceneBase::PIPELINES).map(|_| one(made)).collect(),
        corona_pipeline: one(made),
        smoke_pipeline: one(made),
        snow_pipeline: made.next().flatten(),
        rain_fall_pipeline: made.next().flatten(),
        sky_pipeline: one(made),
    }
}

impl PassKit<'_> {
    fn pass<'s>(&'s self, f: wgpu::TextureFormat, fs: PassShaders<'s>, jobs: &mut Vec<Job<'s, Made>>) {
        let device = self.device;
        for samples in [self.msaa, 1] {
            for i in 0..SceneBase::PIPELINES {
                jobs.push(Box::new(move || Some(self.scene.pipeline_at(device, f, fs.scene, samples, i))));
            }
        }
        jobs.push(Box::new(move || Some(self.coronas.pipeline(device, f, fs.corona, fs.corona_blend, self.msaa))));
        jobs.push(Box::new(move || Some(self.coronas.pipeline(device, f, fs.smoke, ALPHA_BLEND, self.msaa))));
        jobs.push(Box::new(move || (!basic_pipelines()).then(|| optional_pipeline(device, "the snowfall", || self.snow.pipeline(device, f, fs.snow, self.msaa))).flatten()));
        jobs.push(Box::new(move || rainfall_pipeline().then(|| optional_pipeline(device, "the rainfall", || self.rain.pipeline(device, f, fs.rain, self.msaa))).flatten()));
        jobs.push(Box::new(move || Some(self.sky.pipeline(device, f, fs.sky, self.msaa))));
    }

    pub(crate) fn build(&self, format: wgpu::TextureFormat, hdr_format: wgpu::TextureFormat, leave_out_enhanced: bool) -> Passes {
        let enhanced = !leave_out_enhanced;
        let reflections = enhanced && !GL_BACKEND.load(std::sync::atomic::Ordering::Relaxed);
        let mut jobs: Vec<Job<Made>> = Vec::new();
        self.pass(format, PassShaders { scene: "fs_main", corona: "fs_main", corona_blend: SCREEN, smoke: "fs_smoke", snow: "fs_snow", rain: "fs_rain", sky: "fs_main" }, &mut jobs);
        if enhanced {
            jobs.push(Box::new(move || Some(self.sky.pipeline(self.device, format, "fs_enhanced_mirror", self.msaa))));
            self.pass(hdr_format, PassShaders { scene: "fs_enhanced", corona: "fs_enhanced", corona_blend: ADDITIVE, smoke: "fs_smoke_enhanced", snow: "fs_snow_enhanced", rain: "fs_rain_enhanced", sky: "fs_enhanced" }, &mut jobs);
        }
        if reflections {
            self.pass(hdr_format, PassShaders { scene: "fs_vanilla_reflections", corona: "fs_main", corona_blend: SCREEN, smoke: "fs_smoke", snow: "fs_snow", rain: "fs_rain", sky: "fs_main" }, &mut jobs);
        }
        let mut made = compile_all(self.device, jobs).into_iter();
        let pass = take(&mut made);
        let sky_mirror_pipeline = enhanced.then(|| one(&mut made));
        let hdr_pass = enhanced.then(|| take(&mut made));
        let reflection_pass = reflections.then(|| take(&mut made));
        Passes { pass, hdr_pass, sky_mirror_pipeline, reflection_pass, leave_out_enhanced }
    }
}
