//! Worker-local adapter capabilities for auxiliary renderers, including compiler jobs.
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Default)]
pub(crate) struct AuxiliaryState {
    flags: [bool; 8],
    arrays: u8,
    ceiling: Option<crate::Ceiling>,
}

pub(crate) type AuxiliaryContext = Option<Arc<Mutex<AuxiliaryState>>>;
thread_local! {
    static AUXILIARY: RefCell<AuxiliaryContext> = const { RefCell::new(None) };
}

pub(crate) fn auxiliary_context() -> AuxiliaryContext {
    AUXILIARY.with(|state| state.borrow().clone())
}

pub(crate) fn with_auxiliary_context<T>(context: AuxiliaryContext, work: impl FnOnce() -> T) -> T {
    struct Restore(AuxiliaryContext);
    impl Drop for Restore {
        fn drop(&mut self) {
            AUXILIARY.with(|state| state.replace(self.0.take()));
        }
    }
    let _restore = Restore(AUXILIARY.with(|state| state.replace(context)));
    work()
}

/// Run an auxiliary renderer on a dedicated worker without publishing game GPU state.
/// Keep creation, rendering, and destruction in this closure; do not move it to another thread.
pub fn with_auxiliary_renderer<T>(work: impl FnOnce() -> T) -> T {
    with_auxiliary_context(Some(Arc::new(Mutex::new(AuxiliaryState::default()))), work)
}

pub(crate) struct AdapterBool {
    global: AtomicBool,
    index: usize,
}

impl AdapterBool {
    pub const fn new(index: usize) -> Self {
        Self { global: AtomicBool::new(false), index }
    }

    pub fn load(&self, order: Ordering) -> bool {
        match auxiliary_context() {
            Some(state) => state.lock().unwrap_or_else(|e| e.into_inner()).flags[self.index],
            None => self.global.load(order),
        }
    }

    pub fn store(&self, value: bool, order: Ordering) {
        self.swap(value, order);
    }

    pub fn swap(&self, value: bool, order: Ordering) -> bool {
        match auxiliary_context() {
            Some(state) => {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                std::mem::replace(&mut state.flags[self.index], value)
            }
            None => self.global.swap(value, order),
        }
    }
}

pub(crate) struct AdapterArrays(AtomicU8);
impl AdapterArrays {
    pub const fn new() -> Self { Self(AtomicU8::new(0)) }
    pub fn load(&self, order: Ordering) -> u8 {
        match auxiliary_context() {
            Some(state) => state.lock().unwrap_or_else(|e| e.into_inner()).arrays,
            None => self.0.load(order),
        }
    }
    pub fn store(&self, value: u8, order: Ordering) {
        match auxiliary_context() {
            Some(state) => state.lock().unwrap_or_else(|e| e.into_inner()).arrays = value,
            None => self.0.store(value, order),
        }
    }
}

pub(crate) fn update_ceiling<T>(update: impl FnOnce(&mut Option<crate::Ceiling>) -> T) -> T {
    match auxiliary_context() {
        Some(state) => update(&mut state.lock().unwrap_or_else(|e| e.into_inner()).ceiling),
        None => update(&mut super::CEILING.lock().unwrap_or_else(|e| e.into_inner())),
    }
}
