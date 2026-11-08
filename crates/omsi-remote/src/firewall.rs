use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FirewallState {
    Unknown,
    Open,
    Blocked,
    Asking,
}

static STATE: AtomicU8 = AtomicU8::new(FirewallState::Unknown as u8);

pub fn state() -> FirewallState {
    match STATE.load(Ordering::Relaxed) {
        1 => FirewallState::Open,
        2 => FirewallState::Blocked,
        3 => FirewallState::Asking,
        _ => FirewallState::Unknown,
    }
}

pub fn check() {
    if STATE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| (s != FirewallState::Asking as u8).then_some(FirewallState::Unknown as u8)).is_ok() {
        spawn(false);
    }
}

pub fn allow() {
    if STATE.swap(FirewallState::Asking as u8, Ordering::Relaxed) != FirewallState::Asking as u8 {
        spawn(true);
    }
}

fn spawn(allow: bool) {
    let exe = match std::env::current_exe() {
        Ok(exe) if cfg!(windows) => exe,
        _ => return STATE.store(FirewallState::Unknown as u8, Ordering::Relaxed),
    };
    let started = std::thread::Builder::new().name("firewall".into()).spawn(move || {
        let exe = exe.to_string_lossy();
        if allow && run(&allow_script(&exe)) != Some(0) {
            log::info!("remote cockpit: the firewall rule was not added");
        }
        let now = match run(&check_script(&exe)) {
            Some(0) => FirewallState::Open,
            Some(3) => FirewallState::Blocked,
            _ => FirewallState::Unknown,
        };
        let _ = STATE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| (allow || s != FirewallState::Asking as u8).then_some(now as u8));
    });
    if started.is_err() {
        STATE.store(FirewallState::Unknown as u8, Ordering::Relaxed);
    }
}

fn path_literal(exe: &str) -> String {
    let hex: String = exe.encode_utf16().map(|u| format!("{u:04x}")).collect();
    format!("$e = -join ('{hex}' -split '(.{{4}})' | Where-Object {{ $_ }} | ForEach-Object {{ [char][Convert]::ToInt32($_, 16) }})")
}

const RULE: &str = "openOMSI, allowed from the game";

pub(crate) fn check_script(exe: &str) -> String {
    format!(
        "{}; $r = @(Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -eq $e }} | Get-NetFirewallRule | Where-Object {{ $_.Direction -eq 'Inbound' -and $_.Enabled -eq 'True' }}); \
         if (@($r | Where-Object {{ $_.Action -eq 'Block' }}).Count -gt 0) {{ exit 3 }}; \
         if (@(Get-NetFirewallProfile | Where-Object {{ $_.Enabled -eq 'True' -and $_.DefaultInboundAction -ne 'Allow' }}).Count -eq 0) {{ exit 0 }}; \
         if (@($r | Where-Object {{ $_.Action -eq 'Allow' -and ([int]$_.Profile -eq 0 -or ([int]$_.Profile -band 6) -eq 6) }}).Count -gt 0) {{ exit 0 }}; exit 3",
        path_literal(exe)
    )
}

pub(crate) fn allow_script(exe: &str) -> String {
    let elevated = format!(
        "{}; Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -eq $e }} | Get-NetFirewallRule | Where-Object {{ $_.Direction -eq 'Inbound' -and ($_.Action -eq 'Block' -or $_.DisplayName -eq '{RULE}') }} | Remove-NetFirewallRule -ErrorAction SilentlyContinue; \
         New-NetFirewallRule -DisplayName '{RULE}' -Direction Inbound -Action Allow -Program $e -Profile Any -ErrorAction Stop | Out-Null",
        path_literal(exe)
    );
    format!(
        "$b = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('{}')); \
         $p = Start-Process (Join-Path $PSHOME 'powershell.exe') -ErrorAction Stop -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand',$b; exit $p.ExitCode",
        elevated.replace('\'', "''")
    )
}

#[cfg(windows)]
fn run(script: &str) -> Option<i32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let root = std::env::var_os("SystemRoot")?;
    std::process::Command::new(std::path::Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .ok()
        .and_then(|s| s.code())
}

#[cfg(not(windows))]
fn run(_: &str) -> Option<i32> {
    None
}
