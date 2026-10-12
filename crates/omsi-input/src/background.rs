pub fn window() -> Option<isize> {
    #[cfg(windows)]
    return crate::dinput::background_window();
    #[cfg(not(windows))]
    None
}

pub fn pump() {
    #[cfg(windows)]
    crate::dinput::pump_messages();
}
