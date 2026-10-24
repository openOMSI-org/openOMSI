mod grants;
mod content;
#[cfg(test)]
mod lifetime_tests;
mod worker;
mod permissions;
pub use permissions::{ProcessDecision, ProcessPermissions, ProcessPrompt};
pub use grants::{ProcessGrant, ProcessGrants};
use std::collections::BTreeMap;
#[cfg(not(windows))]
use std::process::Child;
use std::process::{Command, ExitStatus, Stdio};
pub(crate) use worker::StartRequest;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::Child;

const MAX_CHILDREN: usize = 32;
#[derive(Default)]
pub struct Processes {
    next: u64,
    children: BTreeMap<u64, Child>,
    pending: BTreeMap<u64, worker::Pending>,
}

#[derive(Clone, Default)]
pub struct Status {
    pub pid: u32,
    pub exit: Option<ExitStatus>,
    pub starting: bool,
    pub awaiting_permission: bool,
    pub error: Option<String>,
}

impl Processes {
    #[cfg(test)]
    pub(crate) fn start(
        &mut self,
        file: &str,
        args: &[String],
        cwd: Option<&str>,
        env: &[(String, String)],
    ) -> Result<u64, String> {
        if self.children.len() + self.pending.len() >= MAX_CHILDREN {
            return Err("too many process handles (limit 32); forget completed processes".into());
        }
        let id = self
            .next
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or("process handle limit reached")?;
        let child = spawn(file, args, cwd, env)?;
        self.next = id;
        self.children.insert(id, child);
        Ok(id)
    }

    pub(crate) fn check_capacity(&self) -> Result<(), String> {
        if self.children.len() + self.pending.len() >= MAX_CHILDREN {
            Err("too many process handles (limit 32); forget completed processes".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn start_checked(&mut self, request: StartRequest) -> Result<u64, String> {
        self.check_capacity()?;
        let id = self
            .next
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or("process handle limit reached")?;
        let pending = worker::submit_request(request)?;
        self.next = id;
        self.pending.insert(id, pending);
        Ok(id)
    }

    pub fn status(&mut self, id: u64) -> Result<Status, String> {
        if let Some(pending) = self.pending.get_mut(&id) {
            return Ok(pending.status());
        }
        let child = self.children.get_mut(&id).ok_or("unknown process handle")?;
        Ok(Status {
            pid: child.id(),
            exit: child.try_wait().map_err(|e| e.to_string())?,
            starting: false,
            awaiting_permission: false,
            error: None,
        })
    }

    pub fn kill(&mut self, id: u64) -> Result<(), String> {
        if let Some(pending) = self.pending.get_mut(&id) {
            pending.kill();
            return Ok(());
        }
        let child = self.children.get_mut(&id).ok_or("unknown process handle")?;
        if child.try_wait().map_err(|e| e.to_string())?.is_none() {
            child.kill().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn forget(&mut self, id: u64) -> Result<(), String> {
        let status = self.status(id)?;
        if status.exit.is_none() && status.error.is_none() {
            return Err("process still running".into());
        }
        self.children.remove(&id);
        self.pending.remove(&id);
        Ok(())
    }

    pub fn poll(&mut self) {
        for child in self.children.values_mut() {
            let _ = child.try_wait();
        }
    }

    pub fn stop_all(&mut self) {
        self.pending.clear();
        let mut pending = Vec::new();
        for (_, mut child) in std::mem::take(&mut self.children) {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
                pending.push(child);
            }
        }
        if !pending.is_empty() {
            std::thread::spawn(move || {
                for mut child in pending {
                    let _ = child.wait();
                }
            });
        }
    }
}

impl Drop for Processes {
    fn drop(&mut self) {
        self.stop_all();
    }
}

fn spawn(
    file: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &[(String, String)],
) -> Result<Child, String> {
    if env
        .iter()
        .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
    {
        return Err("invalid process environment".into());
    }
    let mut cmd = Command::new(file);
    cmd.args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .env_remove("OMSI_PLUGIN_PROCESS_ALLOW")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let parent = std::process::id() as libc::pid_t;
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    let child = windows::spawn(&cmd).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    Ok(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn shell(p: &mut Processes, script: &str) -> u64 {
        #[cfg(unix)]
        let (file, args) = ("/bin/sh", vec!["-c".into(), script.into()]);
        #[cfg(windows)]
        let (file, args) = ("cmd.exe", vec!["/C".into(), script.into()]);
        p.start(file, &args, None, &[]).unwrap()
    }
    fn exited(p: &mut Processes, id: u64) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(exit) = p.status(id).unwrap().exit {
                return exit;
            }
            assert!(Instant::now() < deadline, "child did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn retains_nonzero_exit_code_until_forgotten() {
        let mut p = Processes::default();
        let id = shell(&mut p, "exit 37");
        assert_eq!(exited(&mut p, id).code(), Some(37));
        assert_eq!(p.status(id).unwrap().exit.unwrap().code(), Some(37));
        p.forget(id).unwrap();
        assert!(p.status(id).is_err());
        assert!(p.kill(id).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn exit_259_is_not_confused_with_still_running() {
        let mut p = Processes::default();
        let id = shell(&mut p, "exit 259");
        assert_eq!(exited(&mut p, id).code(), Some(259));
    }
    #[test]
    fn running_poll_and_kill_do_not_wait_for_child() {
        let mut p = Processes::default();
        #[cfg(unix)]
        let id = shell(&mut p, "exec sleep 60");
        #[cfg(windows)]
        let id = shell(&mut p, "for /L %i in (1,1,2147483647) do @rem");
        assert!(p.status(id).unwrap().exit.is_none());
        assert!(p.forget(id).is_err());
        p.kill(id).unwrap();
        assert!(!exited(&mut p, id).success());
    }
    #[test]
    fn failed_start_does_not_allocate_a_handle() {
        let mut p = Processes::default();
        assert!(
            p.start("/nonexistent/openomsi-test-program", &[], None, &[])
                .is_err()
        );
        assert!(p.children.is_empty());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn unload_kills_and_reaps_owned_children() {
        let mut p = Processes::default();
        let id = shell(&mut p, "exec sleep 60");
        let pid = p.status(id).unwrap().pid;
        p.stop_all();
        assert!(p.status(id).is_err());
        let path = std::path::PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while path.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!path.exists(), "child was not killed and reaped");
    }

    #[cfg(unix)]
    #[test]
    fn arguments_are_literal_and_cwd_and_environment_are_passed() {
        let mut p = Processes::default();
        let args = ["-c", "test \"$1\" = '$HOME; exit 99' && test \"$PLUGIN_TEST\" = 'hello world' && test \"$PWD\" = '/tmp'", "test", "$HOME; exit 99"].map(str::to_owned);
        let id = p
            .start(
                "/bin/sh",
                &args,
                Some("/tmp"),
                &[("PLUGIN_TEST".into(), "hello world".into())],
            )
            .unwrap();
        assert!(exited(&mut p, id).success());
    }
}
