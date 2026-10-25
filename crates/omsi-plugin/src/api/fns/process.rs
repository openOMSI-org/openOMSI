use super::{def, o, p, rec};
use crate::api::{ApiError, ApiFn, Value};
const NEW: &str = "unreleased";
fn reason(e: String) -> Value {
    Value::List(vec![Value::Nil, e.into()])
}
fn id(a: &crate::api::Args) -> Result<u64, ApiError> {
    u64::try_from(a.int(0)?)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| ApiError("invalid process handle".into()))
}
fn strict_string(v: &Value) -> Result<String, ApiError> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        _ => Err(ApiError(
            "process arguments and options must be strings".into(),
        )),
    }
}

pub static FNS: &[ApiFn] = &[
    def!("process.wait", "process", [p("handle", "integer")], "exit code or nil, state or reason", "Lua: suspend this plugin through queued verification/start and until its direct child exits, then return `code, \"exited\"` (nonzero codes retained; nil code on signal exit). The game and other plugins keep running; no game-thread wait occurs. Works in the main chunk, start/frame/event handlers and timers. Queued callbacks resume afterwards (limit 256), and normal frames/timers pause during the wait. Stop/reload cancels the continuation and terminates the child. A nested `emit` callback or manually resumed coroutine cannot suspend. Preparation failure returns `nil, \"failed\", reason`. Other language bindings should poll `process.exit_code` instead.", NEW, Process, true, |c, a| Err::<Value,ApiError>(ApiError("process.wait requires the Lua continuation runtime; poll process.exit_code in other bindings".into()))),
    def!("process.start", "process", [p("file", "string"), o("args", "table"), o("options", "table")], "handle, or nil and reason", "Queue a native helper without waiting; return a reserved handle immediately. The interactive game asks the player with Allow once, Always allow or Deny unless the exact content was already approved. Embedded hosts without a permission broker remain default-deny. Verification and OS process creation run on two shared workers with at most 32 globally queued starts. The 32-handle plugin limit (including queued and failed starts) is checked before admission. A full queue returns nil and a reason. Accepted starts can fail later: inspect status.error or exit_code/wait returning nil, failed, reason. `file` must be a relative path inside the plugin folder. Approval is bound to the entry, folder snapshot and helper SHA256. Allow once covers one launch; Always allow saves consent in the host user settings; Deny blocks all native helpers for this canonical plugin entry for the current session, including renamed helpers or changed content or arguments. Changed content needs new approval. While the dialog is open, preparation workers and child cleanup keep running. The entry, executable-content snapshot (including importable Lua modules and native libraries), and helper hashes are rechecked on the worker immediately before each start; code changes revoke consent, while ordinary save/data writes do not. `args` are separate literal strings without shell expansion. Options: `cwd` and `env` (string overrides, other variables inherited, except OMSI_PLUGIN_PROCESS_ALLOW is always removed). Standard streams are disconnected. Windows helpers must be .exe files. At most 32 handles per plugin; forget completed handles. Stop, disable and reload terminate children; Windows job objects also terminate descendants and survive abrupt game exit, Linux uses a parent-death signal for direct children. No native helper runs before player approval. OMSI_PLUGIN_PROCESS_ALLOW can optionally name an absolute JSON grant manifest for preapproved content. Packages must also declare process. Unload/reload cancels unanswered requests. Approved helpers run with the game account's permissions.", NEW, Process, true, |c, a| {
        if let Err(error) = c.state().processes.check_capacity() { return Ok(reason(error)); }
        let file = strict_string(a.get(0))?;
        let args = match a.get(1) { Value::Nil => Vec::new(), Value::List(l) => l.iter().map(strict_string).collect::<Result<Vec<_>,_>>()?, Value::Map(m) if m.is_empty() => Vec::new(), _ => return Err(ApiError("args must be a list of strings".into())) };
        let mut cwd = None;
        let mut env = Vec::new();
        if let Some(opts) = a.opt_table(2)? {
            let opts = match opts { Value::Map(m) => m.as_slice(), Value::List(l) if l.is_empty() => &[], _ => return Err(ApiError("options must be a map".into())) };
            for (k, v) in opts {
                match k.as_str() {
                    "cwd" => cwd = Some(strict_string(v)?),
                    "env" => {
                        let vars = match v { Value::Map(m) => m.as_slice(), Value::List(l) if l.is_empty() => &[], _ => return Err(ApiError("env must be a string-to-string map".into())) };
                        for (k,v) in vars { env.push((k.clone(), strict_string(v)?)); }
                    }
                    _ => return Err(ApiError(format!("unknown process option {k}"))),
                }
            }
        }
        if args.len() > 256 || env.len() > 128 || file.len() + cwd.as_ref().map_or(0, String::len)
            + args.iter().map(String::len).sum::<usize>() + env.iter().map(|(k,v)| k.len()+v.len()).sum::<usize>() > 65536 {
            return Err(ApiError("process request exceeds 256 arguments, 128 environment overrides or 64 KiB".into()));
        }
        let state = c.state();
        let entry = state.process_entry.clone().ok_or_else(|| ApiError("no plugin entry for process grant".into()))?;
        let request = crate::api::process::StartRequest { permissions: state.hub.process_permissions.clone(), plugin: state.name.clone(), grants: state.hub.process_grants.clone(), entry,
            folder: state.folder.clone(), file, args, cwd, env };
        Ok::<_, ApiError>(match state.processes.start_checked(request) { Ok(id) => Value::List(vec![Value::Int(id as i64)]), Err(e) => reason(e) })
    }),
    def!("process.status", "process", [p("handle", "integer")], "table, or nil and reason", "Nonblocking status: `{pid, starting, awaiting_permission, running, exited, failed, error, code, success, signal}`. While verification/start is pending, starting and running are true and pid is nil. Failed preparation sets failed/error without an exit code. While running `code`, `success` and `signal` are nil. After exit `code` is the exit code (including nonzero); termination by a signal has no code, and `signal` is available on Unix. Results remain readable until forgotten.", NEW, Process, true, |c, a| {
        Ok::<_, ApiError>(match c.state().processes.status(id(&a)?) {
            Err(e) => reason(e),
            Ok(s) => {
                #[cfg(unix)] let signal = { use std::os::unix::process::ExitStatusExt; s.exit.and_then(|s| s.signal()).map(|n| n as i64) };
                #[cfg(not(unix))] let signal: Option<i64> = None;
                Value::List(vec![rec(vec![("pid", if s.pid == 0 { Value::Nil } else { (s.pid as i64).into() }), ("starting", s.starting.into()), ("awaiting_permission", s.awaiting_permission.into()), ("error", s.error.clone().into()), ("failed", s.error.is_some().into()), ("running", (s.exit.is_none() && s.error.is_none()).into()), ("exited", s.exit.is_some().into()), ("code", s.exit.and_then(|s| s.code()).map(|n| n as i64).into()), ("success", s.exit.map(|s| s.success()).into()), ("signal", signal.into())])])
            }
        })
    }),
    def!("process.exit_code", "process", [p("handle", "integer")], "code or nil, state or reason", "Nonblocking exit code lookup (queued verification also counts as running): `nil, \"running\"` while running; `code, \"exited\"` after normal exit. A nonzero code is retained. Signal termination returns `nil, \"exited\"`; inspect `process.status` for details. Preparation failure returns `nil, \"failed\", reason`. An unknown handle returns `nil` and a reason.", NEW, Process, true, |c, a| {
        Ok::<_, ApiError>(match c.state().processes.status(id(&a)?) { Err(e) => reason(e), Ok(s) => Value::List(vec![s.exit.and_then(|s| s.code()).map(|n| n as i64).into(), if s.error.is_some() { "failed" } else if s.exit.is_some() { "exited" } else { "running" }.into(), s.error.into()]) })
    }),
    def!("process.kill", "process", [p("handle", "integer")], "true, or nil and reason", "Cancel a queued start or request termination of a plugin-owned direct child without waiting. Poll `process.status` for completion; Windows also terminates the job descendants, Unix targets only the direct child.", NEW, Process, true, |c, a| Ok::<_,ApiError>(match c.state().processes.kill(id(&a)?) { Ok(()) => Value::List(vec![true.into()]), Err(e) => reason(e) })),
    def!("process.forget", "process", [p("handle", "integer")], "true, or nil and reason", "Release a completed or failed process handle and its retained result. A running process cannot be forgotten. On Windows, closing its job also terminates any remaining descendants.", NEW, Process, true, |c, a| Ok::<_,ApiError>(match c.state().processes.forget(id(&a)?) { Ok(()) => Value::List(vec![true.into()]), Err(e) => reason(e) })),
];
