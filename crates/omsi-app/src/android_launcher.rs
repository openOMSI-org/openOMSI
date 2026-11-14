//! The launcher page on a phone: the desktop launcher's page (`launcher/ui`, in the APK's
//! assets) in a web view over the game's window (`LauncherBridge.java`), its commands
//! answered here by the same code the desktop launcher runs (`omsi-launcher-api`). A duty it
//! starts is handed to this process as before (`omsi_launcher_lib::launch` keeps the command
//! line, `android::Shell::switch` takes it): the web view steps aside while the session runs
//! and comes back when it ends. The bus preview and the city map are drawn by the launcher
//! service on a thread of this process (`launcher::InProcess`).

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::{jbyteArray, jstring};
use jni::JNIEnv;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

/// `LauncherBridge`, held from its first call (a thread of this program cannot find an app
/// class by name: the system's class loader does not know them).
static BRIDGE: OnceLock<GlobalRef> = OnceLock::new();

static SERVICE: OnceLock<super::launcher::InProcess> = OnceLock::new();

fn service() -> &'static super::launcher::InProcess {
    SERVICE.get_or_init(|| {
        super::launcher::install();
        super::launcher::InProcess::start()
    })
}

/// Wakes before the page is told the session ended, so its next `frame` finds the preview awake.
pub(crate) fn preview_rests(rests: bool) {
    let Some(service) = SERVICE.get() else { return };
    let cmd = if rests { "rest" } else { "wake" };
    if let Err(e) = service.ask(serde_json::json!({ "cmd": cmd }), Duration::from_secs(10)) {
        log::warn!("bus preview: {cmd}: {e}");
    }
}

/// Call a static `void name(String...)` of `LauncherBridge` from any thread.
fn bridge(name: &str, args: &[&str]) {
    let Some(class) = BRIDGE.get() else { return };
    let ctx = ndk_context::android_context();
    // SAFETY: the VM ndk_context was given lives as long as the program
    let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else { return };
    let Ok(mut env) = vm.attach_current_thread() else { return };
    let strings: Vec<JObject> = args.iter().filter_map(|a| env.new_string(a).ok().map(JObject::from)).collect();
    let values: Vec<JValue> = strings.iter().map(JValue::Object).collect();
    let sig = format!("({})V", "Ljava/lang/String;".repeat(values.len()));
    // SAFETY: the global reference is the class `LauncherBridge`
    let class = unsafe { JClass::from_raw(class.as_obj().as_raw()) };
    if env.call_static_method(&class, name, sig, &values).is_err() {
        let _ = env.exception_clear();
    }
    std::mem::forget(class);
}

/// The session ended: the launcher page comes back (and hears that no game runs any more).
pub(crate) fn session_ended() {
    bridge("sessionEnded", &[]);
}

struct Phone;

impl omsi_launcher_api::Platform for Phone {
    fn emit(&self, event: &str, payload: Value) {
        bridge("emit", &[event, &payload.to_string()]);
    }
    fn launched(&self) {
        bridge("launched", &[]);
    }
    fn renderer(&self, req: Value, wait: Duration) -> Result<(Value, Vec<u8>), String> {
        service().ask(req, wait)
    }
    // (no game program to start with --export-diagnostics: the game is this app's library)
    fn support_package(&self, out: &std::path::Path) -> Option<Result<std::path::PathBuf, String>> {
        Some((|| {
            // Android sets OMSI_CONTENT to shared storage or the app's external data folder.
            let folder = std::env::var_os("OMSI_CONTENT").ok_or("the Android content folder is not available")?;
            let out = std::path::PathBuf::from(folder).join(out.file_name().ok_or("invalid support package filename")?);
            let value = crate::support_bundle::snapshot(None, &crate::settings::Settings::load(), None, None, "launcher");
            crate::support_bundle::export(&out, &value).map_err(|e| format!("{e:#}"))?;
            Ok(out)
        })())
    }
    // (the game runs in this app: a crash took the launcher down with it)
    fn previous_run_crash(&self) -> Option<(String, String)> {
        crate::android::previous_run_crash()
    }
}

fn take(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

/// `LauncherBridge.call(cmd, args)`: a command of the page (on the bridge's worker thread);
/// "1" and the JSON answer, or "0" and the error.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_openomsi_game_LauncherBridge_call<'a>(mut env: JNIEnv<'a>, class: JClass<'a>, cmd: JString<'a>, args: JString<'a>) -> jstring {
    if BRIDGE.get().is_none() {
        if let Ok(g) = env.new_global_ref(&class) {
            let _ = BRIDGE.set(g);
        }
    }
    let cmd = take(&mut env, &cmd);
    let args: Value = serde_json::from_str(&take(&mut env, &args)).unwrap_or(Value::Null);
    let out = match omsi_launcher_api::call(&cmd, &args, &Phone) {
        Ok(v) => format!("1{v}"),
        Err(e) => format!("0{e}"),
    };
    // Setup saved a new installation: out of the gallery now, not at the next start (#1632)
    if cmd == "config_save" && out.starts_with('1') {
        std::thread::spawn(super::android::hide_content_from_gallery);
    }
    env.new_string(out).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// `LauncherBridge.bytes(cmd, args)`: a command answering bytes (the preview's pictures);
/// the bytes after a 1, or a 0 and the error's text.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_openomsi_game_LauncherBridge_bytes<'a>(mut env: JNIEnv<'a>, _class: JClass<'a>, cmd: JString<'a>, args: JString<'a>) -> jbyteArray {
    let cmd = take(&mut env, &cmd);
    let args: Value = serde_json::from_str(&take(&mut env, &args)).unwrap_or(Value::Null);
    let out = match omsi_launcher_api::call_bytes(&cmd, &args, &Phone) {
        Ok(b) => [&[1u8][..], &b].concat(),
        Err(e) => [&[0u8][..], e.as_bytes()].concat(),
    };
    env.byte_array_from_slice(&out).map(JByteArray::into_raw).unwrap_or(std::ptr::null_mut())
}
