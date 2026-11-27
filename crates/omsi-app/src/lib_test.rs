use super::ui_language;

pub(crate) fn with_test_language<T>(code: &str, f: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, OnceLock};
    static LANGUAGE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LANGUAGE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    ui_language(code);
    let result = f();
    ui_language("ENG");
    result
}
