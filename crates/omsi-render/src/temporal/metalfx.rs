//! Apple's MetalFX temporal scaler, macOS on Apple silicon: the same inputs as DLSS and FSR 3
//! (`super::Inputs`), and the same sRGB-encoded picture back.
//!
//! MetalFX is part of macOS 13 and newer. It is not linked, since the game also starts on
//! macOS 11 and 12: the framework is opened when the upscaler is asked for, and its classes are
//! called by name. Its work goes into a command buffer of wgpu's own Metal queue, right after
//! the scene's: a Metal queue runs its buffers in the order they were committed.

use super::{metalfx_render_size, FrameConstants, Inputs, TemporalQuality};
use anyhow::{anyhow, bail, Result};
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::{msg_send, sel, Message};
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLDevice, MTLPixelFormat, MTLTexture, MTLTextureUsage};
use std::ffi::CStr;
use std::sync::OnceLock;

const FRAMEWORK: &CStr = c"/System/Library/Frameworks/MetalFX.framework/MetalFX";

/// The largest scale of a Mac whose MetalFX does not tell (macOS 13).
const MAX_SCALE_UNKNOWN: f32 = 2.0;

type Texture = Retained<ProtocolObject<dyn MTLTexture>>;

fn descriptor_class() -> Result<&'static AnyClass> {
    static CLASS: OnceLock<Option<&'static AnyClass>> = OnceLock::new();
    CLASS
        .get_or_init(|| {
            let opened = unsafe { libc::dlopen(FRAMEWORK.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
            if opened.is_null() {
                return None;
            }
            AnyClass::get(c"MTLFXTemporalScalerDescriptor")
        })
        .ok_or_else(|| anyhow!("this macOS has no MetalFX (it comes with macOS 13 and newer)"))
}

fn texture(t: &wgpu::Texture, what: &str) -> Result<Texture> {
    let hal = unsafe { t.as_hal::<wgpu_hal::api::Metal>() }.ok_or_else(|| anyhow!("no Metal texture under MetalFX's {what}"))?;
    Ok(hal.raw_handle().retain())
}

/// The textures of one frame, in the order of `Scaler::formats`.
struct Frame {
    color: Texture,
    depth: Texture,
    motion: Texture,
    mask: Texture,
    output: Texture,
}

impl Frame {
    fn of(inputs: &Inputs) -> Result<Frame> {
        Ok(Frame {
            color: texture(inputs.color, "picture")?,
            depth: texture(inputs.depth, "depth")?,
            motion: texture(inputs.motion, "motion vectors")?,
            mask: texture(inputs.mask, "mask")?,
            output: texture(inputs.output, "result")?,
        })
    }

    fn formats(&self) -> [MTLPixelFormat; 5] {
        [self.color.pixelFormat(), self.depth.pixelFormat(), self.motion.pixelFormat(), self.mask.pixelFormat(), self.output.pixelFormat()]
    }
}

/// Apple's scaler for one pair of sizes: both are fixed when it is made.
struct Scaler {
    raw: Retained<AnyObject>,
    render: (u32, u32),
    output: (u32, u32),
    formats: [MTLPixelFormat; 5],
    /// The reactive mask came with macOS 14.4: an older MetalFX runs without it.
    mask: bool,
}

pub struct Upscaler {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    max_scale: f32,
    scaler: Option<Scaler>,
    last: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
    clamp_told: bool,
}

// (used from the render thread only)
unsafe impl Send for Upscaler {}

impl Upscaler {
    pub(crate) fn wait(&mut self) {
        if let Some(buffer) = self.last.take() {
            buffer.waitUntilCompleted();
        }
    }

    pub fn load(device: &wgpu::Device) -> Result<Upscaler> {
        let hal = unsafe { device.as_hal::<wgpu_hal::api::Metal>() }.ok_or_else(|| anyhow!("the game is not running on Metal"))?;
        let raw = hal.raw_device().clone();
        drop(hal);
        let class = descriptor_class()?;
        let supported: bool = unsafe { msg_send![class, supportsDevice: &*raw] };
        if !supported {
            bail!("this Mac's graphics chip cannot run MetalFX's temporal scaler");
        }
        // (macOS 14 and newer tell how far this chip scales up)
        let tells: bool = unsafe { msg_send![class, respondsToSelector: sel!(supportedInputContentMaxScaleForDevice:)] };
        let max_scale: f32 = if tells { unsafe { msg_send![class, supportedInputContentMaxScaleForDevice: &*raw] } } else { MAX_SCALE_UNKNOWN };
        let max_scale = if max_scale.is_finite() && max_scale >= 1.0 { max_scale } else { MAX_SCALE_UNKNOWN };
        log::info!("MetalFX upscaling: Apple's temporal scaler, up to {max_scale}x on this Mac");
        Ok(Upscaler { device: raw, max_scale, scaler: None, last: None, clamp_told: false })
    }

    /// The size to draw at: the quality's, but no smaller than MetalFX scales up from here.
    pub fn render_size(&mut self, quality: TemporalQuality, out: (u32, u32)) -> (u32, u32) {
        let (size, clamped) = metalfx_render_size(quality, out, self.max_scale);
        if clamped && !self.clamp_told {
            self.clamp_told = true;
            log::warn!("MetalFX upscaling: {} would scale up {:.1}x, this Mac's MetalFX does {}x at most; drawing at {}x{}", quality.as_str(), 1.0 / quality.scale(), self.max_scale, size.0, size.1);
        }
        size
    }

    fn scaler_for(&mut self, frame: &Frame, render: (u32, u32), output: (u32, u32)) -> Result<&Scaler> {
        let formats = frame.formats();
        if self.scaler.as_ref().is_some_and(|s| s.render != render || s.output != output || s.formats != formats) {
            self.scaler = None;
        }
        if self.scaler.is_none() {
            let class = descriptor_class()?;
            let desc: Retained<AnyObject> = unsafe { msg_send![class, new] };
            let mask: bool = unsafe { msg_send![&*desc, respondsToSelector: sel!(setReactiveMaskTextureEnabled:)] };
            let made: Option<Retained<AnyObject>> = unsafe {
                let _: () = msg_send![&*desc, setColorTextureFormat: formats[0]];
                let _: () = msg_send![&*desc, setDepthTextureFormat: formats[1]];
                let _: () = msg_send![&*desc, setMotionTextureFormat: formats[2]];
                let _: () = msg_send![&*desc, setOutputTextureFormat: formats[4]];
                let _: () = msg_send![&*desc, setInputWidth: render.0 as usize];
                let _: () = msg_send![&*desc, setInputHeight: render.1 as usize];
                let _: () = msg_send![&*desc, setOutputWidth: output.0 as usize];
                let _: () = msg_send![&*desc, setOutputHeight: output.1 as usize];
                // (the picture is the finished, tone-mapped one: there is no exposure to find)
                let _: () = msg_send![&*desc, setAutoExposureEnabled: false];
                if mask {
                    let _: () = msg_send![&*desc, setReactiveMaskTextureEnabled: true];
                    let _: () = msg_send![&*desc, setReactiveMaskTextureFormat: formats[3]];
                }
                msg_send![&*desc, newTemporalScalerWithDevice: &*self.device]
            };
            let raw = made.ok_or_else(|| anyhow!("MetalFX made no temporal scaler for {}x{} to {}x{} ({:?})", render.0, render.1, output.0, output.1, formats))?;
            // (what MetalFX needs of each texture; a texture without it would stop the game
            // in Metal's own check instead of here)
            let mut needs: Vec<(&str, MTLTextureUsage, &Texture)> = unsafe {
                vec![
                    ("picture", msg_send![&*raw, colorTextureUsage], &frame.color),
                    ("depth", msg_send![&*raw, depthTextureUsage], &frame.depth),
                    ("motion vectors", msg_send![&*raw, motionTextureUsage], &frame.motion),
                    ("result", msg_send![&*raw, outputTextureUsage], &frame.output),
                ]
            };
            if mask {
                // (the mask's own usage: `reactiveMaskTextureUsage` from macOS 27, before it the
                // same under its older name, `reactiveTextureUsage`, from 14.4)
                let newer: bool = unsafe { msg_send![&*raw, respondsToSelector: sel!(reactiveMaskTextureUsage)] };
                let need: MTLTextureUsage = if newer { unsafe { msg_send![&*raw, reactiveMaskTextureUsage] } } else { unsafe { msg_send![&*raw, reactiveTextureUsage] } };
                needs.push(("mask", need, &frame.mask));
            }
            for (what, need, have) in needs {
                if have.usage().0 & need.0 != need.0 {
                    bail!("MetalFX cannot use the {what} texture (usage {:#x}, it needs {:#x})", have.usage().0, need.0);
                }
            }
            log::info!("MetalFX upscaling: {}x{} to {}x{}{}", render.0, render.1, output.0, output.1, if mask { "" } else { ", without the mask (macOS 14.4 and newer take one)" });
            self.scaler = Some(Scaler { raw, render, output, formats, mask });
        }
        Ok(self.scaler.as_ref().expect("made above"))
    }

    pub fn upscale(&mut self, queue: &wgpu::Queue, inputs: &Inputs, c: &FrameConstants) -> Result<()> {
        autoreleasepool(|_| {
            let frame = Frame::of(inputs)?;
            let queue_hal = unsafe { queue.as_hal::<wgpu_hal::api::Metal>() }.ok_or_else(|| anyhow!("no Metal queue under wgpu's"))?;
            let buffer = queue_hal.as_raw().commandBuffer().ok_or_else(|| anyhow!("Metal gave no command buffer for MetalFX"))?;
            drop(queue_hal);
            let scaler = self.scaler_for(&frame, inputs.render_size, inputs.output_size)?;
            let none: Option<&ProtocolObject<dyn MTLTexture>> = None;
            unsafe {
                let s = &*scaler.raw;
                let _: () = msg_send![s, setColorTexture: &*frame.color];
                let _: () = msg_send![s, setDepthTexture: &*frame.depth];
                let _: () = msg_send![s, setMotionTexture: &*frame.motion];
                let _: () = msg_send![s, setOutputTexture: &*frame.output];
                if scaler.mask {
                    let _: () = msg_send![s, setReactiveMaskTexture: &*frame.mask];
                }
                let _: () = msg_send![s, setInputContentWidth: inputs.render_size.0 as usize];
                let _: () = msg_send![s, setInputContentHeight: inputs.render_size.1 as usize];
                // (the jitter as the picture was drawn with it, in pixels with y down, the sign
                // FSR 3 takes too: negated, on either axis, a still picture settles twice as
                // far from the full-sized one - `metalfx_settles_on_the_full_sized_picture`)
                let _: () = msg_send![s, setJitterOffsetX: c.jitter[0]];
                let _: () = msg_send![s, setJitterOffsetY: c.jitter[1]];
                // (`motion.wgsl` writes where each pixel was a frame ago, in pixels of the small
                // picture with y down: MetalFX's own convention)
                let _: () = msg_send![s, setMotionVectorScaleX: 1.0f32];
                let _: () = msg_send![s, setMotionVectorScaleY: 1.0f32];
                let _: () = msg_send![s, setDepthReversed: true];
                let _: () = msg_send![s, setPreExposure: 1.0f32];
                let _: () = msg_send![s, setReset: c.reset];
                let _: () = msg_send![s, encodeToCommandBuffer: &*buffer];
                // (the command buffer holds the textures from here; the scaler would keep the
                // last frame's alive past a change of the window's size)
                let _: () = msg_send![s, setColorTexture: none];
                let _: () = msg_send![s, setDepthTexture: none];
                let _: () = msg_send![s, setMotionTexture: none];
                let _: () = msg_send![s, setOutputTexture: none];
                if scaler.mask {
                    let _: () = msg_send![s, setReactiveMaskTexture: none];
                }
            }
            buffer.commit();
            self.last = Some(buffer);
            Ok(())
        })
    }
}

impl Drop for Upscaler {
    fn drop(&mut self) {
        self.wait();
    }
}
