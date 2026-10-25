use super::Processes;
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const LIFETIME_FIXTURE: &str = "api::process::lifetime_tests::lifetime_fixture";
const ARGUMENT_FIXTURE: &str = "api::process::lifetime_tests::argument_fixture";

#[test]
#[ignore = "subprocess fixture"]
fn lifetime_fixture() {
    let root = std::env::var_os("PLUGIN_LIFETIME_ROOT").unwrap();
    let root = Path::new(&root);
    let role = std::env::var("PLUGIN_LIFETIME_ROLE").unwrap();
    let exe = std::env::current_exe().unwrap();
    let args = ["--exact", LIFETIME_FIXTURE, "--ignored"].map(str::to_owned);
    if role == "parent" {
        let mut processes = Processes::default();
        let id = processes
            .start(
                exe.to_str().unwrap(),
                &args,
                None,
                &[
                    ("PLUGIN_LIFETIME_ROOT".into(), root.to_str().unwrap().into()),
                    ("PLUGIN_LIFETIME_ROLE".into(), "child".into()),
                    ("OMSI_PLUGIN_PROCESS_ALLOW".into(), "must be removed".into()),
                ],
            )
            .unwrap();
        std::fs::write(
            root.join("child"),
            processes.status(id).unwrap().pid.to_string(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(60));
    } else {
        assert!(std::env::var_os("OMSI_PLUGIN_PROCESS_ALLOW").is_none());
        #[cfg(windows)]
        if role == "child" {
            let child = Command::new(exe)
                .args(&args)
                .env("PLUGIN_LIFETIME_ROLE", "grandchild")
                .spawn()
                .unwrap();
            std::fs::write(root.join("grandchild"), child.id().to_string()).unwrap();
        }
        std::fs::write(root.join(format!("{role}-ready")), "").unwrap();
        std::thread::sleep(Duration::from_secs(60));
    }
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let running = unsafe { WaitForSingleObject(handle, 0) } == WAIT_TIMEOUT;
    unsafe {
        CloseHandle(handle);
    }
    running
}
#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
        !s.split_once(") ")
            .is_some_and(|(_, tail)| tail.starts_with('Z'))
    })
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn abrupt_parent_exit_kills_helpers() {
    let root = std::env::temp_dir().join(format!("omsi-lifetime-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", LIFETIME_FIXTURE, "--ignored"])
        .env("PLUGIN_LIFETIME_ROOT", &root)
        .env("PLUGIN_LIFETIME_ROLE", "parent")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let ready = if cfg!(windows) {
        "grandchild-ready"
    } else {
        "child-ready"
    };
    while !root.join(ready).exists() && Instant::now() < deadline {
        if parent.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let was_ready = root.join(ready).exists();
    let _ = parent.kill();
    parent.wait().unwrap();
    assert!(was_ready, "helper fixture did not start");
    let names: &[&str] = if cfg!(windows) {
        &["child", "grandchild"]
    } else {
        &["child"]
    };
    for name in names {
        let pid = std::fs::read_to_string(root.join(name))
            .unwrap()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!alive(pid), "{name} {pid} survived abrupt parent exit");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "argument fixture"]
fn argument_fixture() {
    let expected: Vec<String> =
        serde_json::from_str(&std::env::var("PLUGIN_EXPECTED_ARGS").unwrap()).unwrap();
    let actual: Vec<_> = std::env::args().skip(4).collect();
    assert_eq!(actual, expected);
    assert_eq!(
        std::env::var("PLUGIN_ARGUMENT_VALUE").unwrap(),
        "hello world"
    );
    assert!(std::env::var_os("OMSI_PLUGIN_PROCESS_ALLOW").is_none());
    assert_eq!(
        std::env::current_dir().unwrap().canonicalize().unwrap(),
        Path::new(&std::env::var_os("PLUGIN_EXPECTED_CWD").unwrap())
            .canonicalize()
            .unwrap()
    );
    std::process::exit(59);
}

#[test]
fn literal_arguments_environment_and_working_directory() {
    let mut processes = Processes::default();
    let exe = std::env::current_exe().unwrap();
    // Use a normal absolute cwd. A verbatim \\?\ cwd can hang loader initialization
    // for this unit-test executable on Windows, independently of the process API.
    let cwd = std::env::temp_dir();
    let mut args = vec![
        "--exact".into(),
        ARGUMENT_FIXTURE.into(),
        "--ignored".into(),
    ];
    let mut expected = Vec::new();
    for value in [
        "",
        "spaces and \"quotes\"",
        "trailing\\",
        "$HOME; exit 99",
        "árvíz",
    ] {
        expected.push("--skip".to_owned());
        expected.push(value.to_owned());
    }
    args.extend(expected.clone());
    let id = processes
        .start(
            exe.to_str().unwrap(),
            &args,
            cwd.to_str(),
            &[
                (
                    "PLUGIN_EXPECTED_ARGS".into(),
                    serde_json::to_string(&expected).unwrap(),
                ),
                ("PLUGIN_ARGUMENT_VALUE".into(), "hello world".into()),
                ("PLUGIN_EXPECTED_CWD".into(), cwd.to_str().unwrap().into()),
                ("OMSI_PLUGIN_PROCESS_ALLOW".into(), "must be removed".into()),
            ],
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(exit) = processes.status(id).unwrap().exit {
            assert_eq!(exit.code(), Some(59));
            break;
        }
        assert!(Instant::now() < deadline, "argument fixture timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
