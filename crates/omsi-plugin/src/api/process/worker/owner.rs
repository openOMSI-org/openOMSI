use super::*;

pub(super) fn own(results: mpsc::Receiver<Finished>, jobs: SyncSender<Job>) {
    let mut children: Vec<(Child, Arc<Control>)> = Vec::new();
    let mut waiting: Vec<(Awaiting, Arc<Control>)> = Vec::new();
    loop {
        match results.recv_timeout(Duration::from_millis(5)) {
            Ok(done) => match done.result {
                Ok(Stage::Ready(mut child)) => {
                    if done.control.cancel.load(Ordering::Acquire) {
                        let _ = child.kill();
                    }
                    *done.control.status.lock().unwrap() = Status {
                        pid: child.id(),
                        ..Default::default()
                    };
                    children.push((child, done.control));
                }
                Ok(Stage::Waiting(wait)) => {
                    done.control.status.lock().unwrap().awaiting_permission = true;
                    waiting.push((wait, done.control));
                }
                Err(error) => fail(&done.control, error),
            },
            Err(mpsc::RecvTimeoutError::Disconnected)
                if children.is_empty() && waiting.is_empty() =>
            {
                break;
            }
            Err(_) => {}
        }
        let mut k = 0;
        while k < waiting.len() {
            let (wait, control) = &waiting[k];
            if control.cancel.load(Ordering::Acquire) || control.release.load(Ordering::Acquire) {
                fail(control, "process start cancelled".into());
                waiting.swap_remove(k);
                continue;
            }
            let choice = wait.decision.load(Ordering::Acquire);
            if choice == 0 {
                k += 1;
                continue;
            }
            let (wait, control) = waiting.swap_remove(k);
            if choice == super::super::ProcessDecision::Deny as u8 {
                if let Some(permissions) = &wait.request.permissions {
                    permissions.deny(wait.grant);
                }
                fail(
                    &control,
                    "native process request denied by the player".into(),
                );
                continue;
            }
            // Never wait for a free preparation slot: child cleanup must keep running.
            control.status.lock().unwrap().awaiting_permission = false;
            let job = Job {
                work: Work::Approved(
                    wait,
                    choice == super::super::ProcessDecision::AlwaysAllow as u8,
                ),
                control,
            };
            match jobs.try_send(job) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(job)) => {
                    let Work::Approved(wait, _) = job.work else {
                        unreachable!()
                    };
                    waiting.push((wait, job.control));
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(job)) => {
                    fail(&job.control, "process preparation unavailable".into())
                }
            }
        }
        children.retain_mut(|(child, control)| {
            if control.cancel.swap(false, Ordering::AcqRel)
                || control.release.load(Ordering::Acquire)
            {
                let _ = child.kill();
            }
            match child.try_wait() {
                Ok(exit) => {
                    control.status.lock().unwrap().exit = exit;
                    !(exit.is_some() && control.release.load(Ordering::Acquire))
                }
                Err(error) => {
                    control.status.lock().unwrap().error = Some(error.to_string());
                    let _ = child.kill();
                    true
                }
            }
        });
    }
}
fn fail(control: &Control, error: String) {
    *control.status.lock().unwrap() = Status {
        error: Some(error),
        ..Default::default()
    };
}
