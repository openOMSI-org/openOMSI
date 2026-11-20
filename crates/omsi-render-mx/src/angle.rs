//! ANGLE/D3D11 instance creation shared by the game and the Windows regression test.

#[cfg(any(windows, test))]
const SERIAL_COMPILER_FEATURES: [&str; 3] = [
    "compileJobIsThreadSafe",
    "linkJobIsThreadSafe",
    "alwaysRunLinkSubJobsThreaded",
];

#[cfg(any(windows, test))]
fn serial_compiler_overrides(existing: &str) -> String {
    let mut features: Vec<&str> = existing.split(':').filter(|f| !f.is_empty()).collect();
    for feature in SERIAL_COMPILER_FEATURES {
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    features.join(":")
}

/// Create an ANGLE instance, keeping shader translation and linking on the calling
/// thread. The packaged ANGLE reproduced a native worker stack overflow on the first
/// scene pipeline in Windows/WARP CI. Its frontend feature overrides select its existing
/// synchronous pools without changing shader contents or rendered effects.
///
/// Call during startup, before any ANGLE display is initialized: ANGLE caches these
/// overrides. Renderer pipeline creation then uses `compile`'s managed stack, rather
/// than relying on the stack size chosen by ANGLE or the host's entry thread.
pub fn instance() -> mx::Instance {
    #[cfg(windows)]
    {
        let existing = std::env::var("ANGLE_FEATURE_OVERRIDES_DISABLED").unwrap_or_default();
        unsafe {
            std::env::set_var(
                "ANGLE_FEATURE_OVERRIDES_DISABLED",
                serial_compiler_overrides(&existing),
            );
        }
        log::info!(
            "ANGLE: native shader worker jobs disabled; renderer compilation uses a managed {COMPILER_STACK_MB} MB stack"
        );
    }
    // (on Windows mX's OpenGL is ANGLE's libEGL.dll, shipped beside the game)
    mx::Instance::with_backends(mx::Backends::GL)
}

/// The compiler thread's stack. (256 MB overflowed on WARP as fast as 32 MB did, in the
/// scene pipeline: a larger stack does not help that crash.)
#[cfg(windows)]
const COMPILER_STACK_MB: usize = 32;

/// A single worker for the entire pipeline build, not one thread per pipeline. With
/// ANGLE's native jobs disabled, translation and D3D compilation inherit this stack.
/// Reserving stack address space does not commit it at thread creation: only the pages
/// the compiler reaches are.
pub(crate) fn compile<F, T>(device: &mx::Device, operation: F) -> T
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    #[cfg(windows)]
    {
        no_parallel_compiles(device);
        let context = crate::auxiliary_context();
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("ANGLE shader compiler".into())
                .stack_size(COMPILER_STACK_MB << 20)
                .spawn_scoped(scope, move || {
                    crate::with_auxiliary_context(context, operation)
                })
                .expect("ANGLE shader compiler thread")
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }
    #[cfg(not(windows))]
    {
        let _ = device;
        operation()
    }
}

/// The feature overrides alone leave the link's HLSL compiles on ANGLE's thread pool:
/// with `alwaysRunLinkSubJobsThreaded` off, its sub-jobs go to the shader compile pool,
/// which stays the multi-threaded one while GL_KHR_parallel_shader_compile allows any
/// compiler threads (the default). ANGLE's native threads have the executable's default
/// 1 MB stack, where WARP's first scene pipeline overflowed it. No compiler threads at
/// all puts every compile and link on the calling, managed thread.
#[cfg(windows)]
fn no_parallel_compiles(device: &mx::Device) {
    use mx::mx_gl::glow::HasContext;
    let Some(device) = device.as_gl() else { return };
    device.with_gl(|gl| {
        if gl
            .supported_extensions()
            .contains("GL_KHR_parallel_shader_compile")
        {
            // SAFETY: the extension is there, so the KHR entry point is loaded
            unsafe { gl.max_shader_compiler_threads(0) };
            log::info!(
                "ANGLE: no parallel shader compiler threads (GL_KHR_parallel_shader_compile)"
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_compiler_overrides_use_angles_colon_separator() {
        assert_eq!(
            serial_compiler_overrides(""),
            "compileJobIsThreadSafe:linkJobIsThreadSafe:alwaysRunLinkSubJobsThreaded"
        );
    }

    #[test]
    fn serial_compiler_overrides_preserve_user_features_and_do_not_duplicate() {
        let original = "otherWorkaround:compileJobIsThreadSafe";
        let combined = serial_compiler_overrides(original);
        assert!(combined.starts_with(original));
        assert_eq!(combined.matches("compileJobIsThreadSafe").count(), 1);
        assert_eq!(serial_compiler_overrides(&combined), combined);
    }
}
