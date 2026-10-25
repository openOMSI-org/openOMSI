use super::{Child, ProcessGrant, ProcessGrants, ProcessPermissions, Status};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc::{self, SyncSender},
};
use std::time::Duration;

const WORKERS: usize = 2;
const QUEUED: usize = 32;
#[cfg(test)]
type Prepare = Box<dyn FnOnce(&AtomicBool) -> Result<Child, String> + Send>;
struct Job {
    work: Work,
    control: Arc<Control>,
}
struct Finished {
    result: Result<Stage, String>,
    control: Arc<Control>,
}
enum Work {
    #[cfg(test)]
    Native(Prepare),
    Request(StartRequest),
    Approved(Awaiting, bool),
}
enum Stage {
    Ready(Child),
    Waiting(Awaiting),
}
struct Awaiting {
    request: StartRequest,
    grant: ProcessGrant,
    decision: Arc<AtomicU8>,
}
#[derive(Default)]
struct Control {
    cancel: AtomicBool,
    release: AtomicBool,
    status: Mutex<Status>,
}

pub(super) struct Pending {
    control: Arc<Control>,
    cached: Status,
}
impl Pending {
    pub fn status(&mut self) -> Status {
        if let Ok(status) = self.control.status.try_lock() {
            self.cached = status.clone();
        }
        self.cached.clone()
    }
    pub fn kill(&mut self) {
        self.control.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.control.cancel.store(true, Ordering::Release);
        self.control.release.store(true, Ordering::Release);
    }
}

pub(crate) struct StartRequest {
    pub permissions: Option<Arc<ProcessPermissions>>,
    pub plugin: String,
    pub grants: Arc<ProcessGrants>,
    pub entry: PathBuf,
    pub folder: Option<PathBuf>,
    pub file: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
}
impl StartRequest {
    fn prepare(self, cancel: &AtomicBool) -> Result<Stage, String> {
        if let Some(permissions) = &self.permissions { permissions.check_denied(&self.entry)?; }
        #[cfg(test)]
        if let Some(before) = &self.grants.before_verify {
            before();
        }
        if cancel.load(Ordering::Acquire) {
            return Err("process start cancelled".into());
        }
        let helper = match self
            .grants
            .helper(&self.entry, self.folder.as_deref(), &self.file)
        {
            Ok(helper) => helper,
            Err(error) => {
                let Some(permissions) = &self.permissions else {
                    return Err(error);
                };
                match permissions.authorize(&self)? {
                    super::permissions::Authorization::Granted(grant) => {
                        return self.approved(grant, false, cancel);
                    }
                    super::permissions::Authorization::Pending(grant, decision) => {
                        return Ok(Stage::Waiting(Awaiting {
                            request: self,
                            grant,
                            decision,
                        }));
                    }
                }
            }
        };
        self.spawn(helper, cancel)
    }
    fn approved(
        self,
        grant: ProcessGrant,
        always: bool,
        cancel: &AtomicBool,
    ) -> Result<Stage, String> {
        if cancel.load(Ordering::Acquire) {
            return Err("process start cancelled".into());
        }
        let helper = ProcessGrants {
            grants: vec![grant.clone()],
            ..Default::default()
        }
        .helper(&self.entry, self.folder.as_deref(), &self.file)?;
        if cancel.load(Ordering::Acquire) {
            return Err("process start cancelled".into());
        }
        if always {
            self.permissions
                .as_ref()
                .ok_or("process permissions unavailable")?
                .remember(grant)?;
        }
        self.spawn(helper, cancel)
    }
    fn spawn(self, helper: PathBuf, cancel: &AtomicBool) -> Result<Stage, String> {
        if let Some(permissions) = &self.permissions { permissions.check_denied(&self.entry)?; }
        if cancel.load(Ordering::Acquire) {
            return Err("process start cancelled".into());
        }
        super::spawn(
            helper.to_str().ok_or("helper path is not UTF-8")?,
            &self.args,
            self.cwd.as_deref(),
            &self.env,
        )
        .map(Stage::Ready)
    }
}

#[cfg(test)]
pub(super) fn submit(prepare: Prepare) -> Result<Pending, String> {
    enqueue(Work::Native(prepare))
}
pub(super) fn submit_request(request: StartRequest) -> Result<Pending, String> {
    enqueue(Work::Request(request))
}
fn enqueue(work: Work) -> Result<Pending, String> {
    static POOL: OnceLock<Result<SyncSender<Job>, String>> = OnceLock::new();
    let pool = POOL.get_or_init(pool).as_ref().map_err(Clone::clone)?;
    let status = Status {
        starting: true,
        ..Default::default()
    };
    let control = Arc::new(Control {
        status: Mutex::new(status.clone()),
        ..Default::default()
    });
    pool.try_send(Job {
        work,
        control: control.clone(),
    })
    .map_err(|_| "process preparation queue is full or unavailable")?;
    Ok(Pending {
        control,
        cached: status,
    })
}

fn pool() -> Result<SyncSender<Job>, String> {
    let (send, receive) = mpsc::sync_channel::<Job>(QUEUED);
    let receive = Arc::new(Mutex::new(receive));
    let (finished, results) = mpsc::sync_channel::<Finished>(WORKERS);
    let owner_jobs = send.clone();
    std::thread::Builder::new()
        .name("plugin-process-owner".into())
        .spawn(move || owner::own(results, owner_jobs))
        .map_err(|e| e.to_string())?;
    for index in 0..WORKERS {
        let receive = receive.clone();
        let finished = finished.clone();
        std::thread::Builder::new()
            .name(format!("plugin-process-prepare-{index}"))
            .spawn(move || {
                loop {
                    let Ok(job) = receive.lock().unwrap().recv() else {
                        break;
                    };
                    let result = if job.control.cancel.load(Ordering::Acquire) {
                        Err("process start cancelled".into())
                    } else {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match job.work {
                            #[cfg(test)]
                            Work::Native(prepare) => prepare(&job.control.cancel).map(Stage::Ready),
                            Work::Request(request) => request.prepare(&job.control.cancel),
                            Work::Approved(wait, always) => {
                                wait.request
                                    .approved(wait.grant, always, &job.control.cancel)
                            }
                        }))
                        .unwrap_or_else(|_| Err("process preparation worker panicked".into()))
                    };
                    if let Err(error) = finished.send(Finished {
                        result,
                        control: job.control,
                    }) {
                        if let Ok(Stage::Ready(mut child)) = error.0.result {
                            let _ = child.kill();
                            let _ = child.wait();
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(send)
}

#[path = "worker/owner.rs"]
mod owner;

#[cfg(test)]
mod tests;
