use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) type Job<'a, T> = Box<dyn FnOnce() -> T + Send + 'a>;

const STACK: usize = 32 << 20;

fn threads() -> usize {
    if crate::gl_backend() || omsi_cfg::flags::OMSI_SERIAL_PIPELINES.is_set() {
        return 1;
    }
    let most = if cfg!(any(target_os = "android", target_os = "ios")) { 4 } else { 8 };
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, most)
}

/// Run `jobs` on a few threads (one on OpenGL), their results in order. A job that panics -
/// mX refusing a GPU object (`.expect("mX: ...")`) - does not stop the others; once all are
/// done, the first panic goes on to the caller as it was (its `gpu_attempt` knows mX's).
pub(crate) fn compile_all<'a, T: Send>(_device: &mx::Device, jobs: Vec<Job<'a, T>>) -> Vec<T> {
    let n = jobs.len();
    let threads = threads().min(n);
    let jobs: Vec<Mutex<Option<Job<'a, T>>>> =
        jobs.into_iter().map(|j| Mutex::new(Some(j))).collect();
    let made: Vec<Mutex<Option<T>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let panicked: Mutex<Option<Box<dyn std::any::Any + Send>>> = Mutex::new(None);
    let next = AtomicUsize::new(0);
    let work = || loop {
        let k = next.fetch_add(1, Ordering::Relaxed);
        let Some(slot) = jobs.get(k) else { break };
        let Some(job) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            continue;
        };
        match omsi_cfg::panic_guard::catch_unwind(job) {
            Ok(out) => *made[k].lock().unwrap_or_else(|e| e.into_inner()) = Some(out),
            Err(p) => {
                panicked
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get_or_insert(p);
            }
        }
    };
    if threads <= 1 {
        work();
    } else {
        std::thread::scope(|s| {
            let context = crate::auxiliary_context();
            for i in 1..threads {
                let context = context.clone();
                let work = &work;
                let _ = std::thread::Builder::new()
                    .name(format!("renderer pipelines {i}"))
                    .stack_size(STACK)
                    .spawn_scoped(s, move || crate::with_auxiliary_context(context, work));
            }
            work();
        });
    }
    if let Some(p) = panicked.into_inner().unwrap_or_else(|e| e.into_inner()) {
        std::panic::resume_unwind(p);
    }
    made.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner()).expect("every pipeline job ran")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_device() -> mx::Device {
        let instance = mx::Instance::with_backends(mx::Backends::NOOP);
        let adapter = instance.default_adapter().unwrap();
        adapter
            .request_device(&mx::DeviceDescriptor::default()).unwrap().0
    }

    #[test]
    fn results_keep_the_order_of_the_jobs() {
        let device = noop_device();
        let jobs: Vec<Job<usize>> = (0..97usize)
            .map(|i| -> Job<usize> {
                Box::new(move || {
                    std::thread::sleep(std::time::Duration::from_micros(((i * 37) % 11) as u64 * 100));
                    i * 3
                })
            })
            .collect();
        assert_eq!(compile_all(&device, jobs), (0..97).map(|i| i * 3).collect::<Vec<_>>());
    }

    #[test]
    fn a_refused_job_reaches_the_callers_gpu_attempt() {
        let device = noop_device();
        let device_ref = &device;
        let jobs: Vec<Job<()>> = (0..24)
            .map(|_| -> Job<()> {
                Box::new(move || {
                    let _ = device_ref
                        .create_shader_module(&mx::ShaderModuleDescriptor {
                            label: Some("broken"),
                            source: "fn broken( {",
                        })
                        .expect("mX: a shader");
                })
            })
            .collect();
        let refused = crate::gpu_attempt(|| compile_all(&device, jobs));
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.starts_with("mX: a shader")),
            "{refused:?}");
    }
}
