use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub(crate) type Job<'a, T> = Box<dyn FnOnce() -> T + Send + 'a>;

const STACK: usize = 32 << 20;

thread_local! {
    static CAUGHT: RefCell<Option<Vec<wgpu::Error>>> = const { RefCell::new(None) };
}

struct Restore(Option<Option<Vec<wgpu::Error>>>);

impl Drop for Restore {
    fn drop(&mut self) {
        if let Some(outer) = self.0.take() {
            CAUGHT.with(|c| c.replace(outer));
        }
    }
}

pub(crate) fn catching<T>(f: impl FnOnce() -> T) -> (T, Vec<wgpu::Error>) {
    let mut restore = Restore(Some(CAUGHT.with(|c| c.replace(Some(Vec::new())))));
    let made = f();
    let outer = restore.0.take().unwrap_or_default();
    let caught = CAUGHT.with(|c| c.replace(outer)).unwrap_or_default();
    (made, caught)
}

fn threads() -> usize {
    if crate::gl_backend() || omsi_cfg::flags::OMSI_SERIAL_PIPELINES.is_set() {
        return 1;
    }
    let most = if cfg!(any(target_os = "android", target_os = "ios")) { 4 } else { 8 };
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, most)
}

pub(crate) fn compile_all<'a, T: Send>(device: &wgpu::Device, jobs: Vec<Job<'a, T>>) -> Vec<T> {
    let n = jobs.len();
    let threads = threads().min(n);
    if threads <= 1 {
        return jobs.into_iter().map(|job| job()).collect();
    }
    let scoped = CAUGHT.with(|c| c.borrow().is_some());
    let jobs: Vec<Mutex<Option<Job<'a, T>>>> = jobs.into_iter().map(|j| Mutex::new(Some(j))).collect();
    let made: Vec<Mutex<Option<T>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let work = || loop {
        let k = next.fetch_add(1, Ordering::Relaxed);
        let Some(slot) = jobs.get(k) else { break };
        if let Some(job) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let out = job();
            *made[k].lock().unwrap_or_else(|e| e.into_inner()) = Some(out);
        }
    };
    let caught: Vec<wgpu::Error> = std::thread::scope(|s| {
        let context = crate::auxiliary_context();
        let workers: Vec<_> = (1..threads)
            .filter_map(|i| {
                let context = context.clone();
                let work = &work;
                std::thread::Builder::new()
                    .name(format!("renderer pipelines {i}"))
                    .stack_size(STACK)
                    .spawn_scoped(s, move || crate::with_auxiliary_context(context, || {
                        let scopes = scoped.then(|| [wgpu::ErrorFilter::Internal, wgpu::ErrorFilter::OutOfMemory, wgpu::ErrorFilter::Validation].map(|f| device.push_error_scope(f)));
                        work();
                        scopes.map_or_else(Vec::new, |[internal, memory, validation]| {
                            pollster::block_on(async { [validation.pop().await, memory.pop().await, internal.pop().await] }).into_iter().flatten().collect()
                        })
                    }))
                    .ok()
            })
            .collect();
        work();
        let mut caught = Vec::new();
        for w in workers {
            match w.join() {
                Ok(errors) => caught.extend(errors),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        caught
    });
    if scoped && !caught.is_empty() {
        CAUGHT.with(|c| c.borrow_mut().get_or_insert_with(Vec::new).extend(caught));
    }
    made.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner()).expect("every pipeline job ran")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_device() -> wgpu::Device {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::NOOP;
        descriptor.backend_options.noop = wgpu::NoopBackendOptions { enable: true };
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap().0
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
    fn every_failed_job_reaches_the_callers_scopes() {
        let device = noop_device();
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let device_ref = &device;
        let jobs: Vec<Job<()>> = (0..24)
            .map(|_| -> Job<()> {
                Box::new(move || {
                    let _ = device_ref.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("broken"), source: wgpu::ShaderSource::Wgsl("fn broken( {".into()) });
                })
            })
            .collect();
        let escaped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = escaped.clone();
        device.on_uncaptured_error(std::sync::Arc::new(move |_| flag.store(true, Ordering::Relaxed)));
        let (_, caught) = catching(|| compile_all(&device, jobs));
        let own = pollster::block_on(validation.pop());
        assert!(own.is_some() || !caught.is_empty());
        assert!(caught.iter().all(|e| matches!(e, wgpu::Error::Validation { .. })));
        assert!(!escaped.load(Ordering::Relaxed), "an error went past the scopes");
    }
}
