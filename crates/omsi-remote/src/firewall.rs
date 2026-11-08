use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FirewallState {
    Unknown,
    Open,
    Blocked,
    Asking,
    Setting,
}

static STATE: AtomicU8 = AtomicU8::new(FirewallState::Unknown as u8);

pub fn state() -> FirewallState {
    match STATE.load(Ordering::Relaxed) {
        1 => FirewallState::Open,
        2 => FirewallState::Blocked,
        3 => FirewallState::Asking,
        4 => FirewallState::Setting,
        _ => FirewallState::Unknown,
    }
}

pub fn check(ip: &str, port: u16) {
    if STATE.try_update(Ordering::Relaxed, Ordering::Relaxed, |s| (s != FirewallState::Asking as u8).then_some(FirewallState::Unknown as u8)).is_ok() {
        spawn(false, ip, port);
    }
}

pub fn allow(ip: &str, port: u16) {
    if STATE.swap(FirewallState::Asking as u8, Ordering::Relaxed) != FirewallState::Asking as u8 {
        spawn(true, ip, port);
    }
}

fn spawn(allow: bool, ip: &str, port: u16) {
    let exe = match std::env::current_exe() {
        Ok(exe) if cfg!(windows) => exe,
        _ => return STATE.store(FirewallState::Unknown as u8, Ordering::Relaxed),
    };
    let ip = ip.parse::<std::net::IpAddr>().ok().filter(|a| !a.is_loopback()).map(|a| a.to_string()).unwrap_or_default();
    let started = std::thread::Builder::new().name("firewall".into()).spawn(move || {
        let exe = exe.to_string_lossy();
        if allow && run(&allow_script(&exe, port)) != Some(0) {
            log::info!("remote cockpit: the firewall rule was not added");
        }
        let now = match run(&check_script(&exe, &ip, port)) {
            Some(0) => FirewallState::Open,
            Some(3) => FirewallState::Blocked,
            Some(4) => FirewallState::Setting,
            _ => FirewallState::Unknown,
        };
        let _ = STATE.try_update(Ordering::Relaxed, Ordering::Relaxed, |s| (allow || s != FirewallState::Asking as u8).then_some(now as u8));
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

pub(crate) fn check_script(exe: &str, ip: &str, port: u16) -> String {
    format!(
        "{}; $port = {port}; $keys = @{{ Domain = 1; DomainAuthenticated = 1; Private = 2; Public = 4 }}; \
         $c = @(); if ('{ip}') {{ foreach ($a in @(Get-NetIPAddress -IPAddress '{ip}' -ErrorAction SilentlyContinue)) {{ $c += @(Get-NetConnectionProfile -InterfaceIndex $a.InterfaceIndex -ErrorAction SilentlyContinue) }} }}; \
         if ($c.Count -eq 0) {{ $c = @(Get-NetConnectionProfile -ErrorAction SilentlyContinue) }}; \
         $active = 0; foreach ($x in $c) {{ $active = $active -bor $keys[[string]$x.NetworkCategory] }}; if ($active -eq 0) {{ exit 5 }}; \
         $need = 0; $on = 0; foreach ($f in @(Get-NetFirewallProfile -PolicyStore ActiveStore -ErrorAction Stop)) {{ $b = $keys[[string]$f.Name]; if (($active -band $b) -and [string]$f.Enabled -ne 'False') {{ if ([string]$f.AllowInboundRules -eq 'False' -or [string]$f.AllowLocalFirewallRules -eq 'False') {{ exit 4 }}; $on = $on -bor $b; if ([string]$f.DefaultInboundAction -ne 'Allow') {{ $need = $need -bor $b }} }} }}; if ($on -eq 0) {{ exit 0 }}; \
         $app = @{{}}; Get-NetFirewallApplicationFilter -PolicyStore ActiveStore | ForEach-Object {{ if (-not $_.Package) {{ $app[$_.InstanceID] = $_.Program }} }}; \
         $sf = @{{}}; Get-NetFirewallServiceFilter -PolicyStore ActiveStore | ForEach-Object {{ $sf[$_.InstanceID] = [string]$_.Service }}; \
         $pf = @{{}}; Get-NetFirewallPortFilter -PolicyStore ActiveStore | ForEach-Object {{ $pf[$_.InstanceID] = $_ }}; \
         $af = @{{}}; Get-NetFirewallAddressFilter -PolicyStore ActiveStore | ForEach-Object {{ $af[$_.InstanceID] = $_ }}; \
         $r = @(Get-NetFirewallRule -PolicyStore ActiveStore -Direction Inbound -Enabled True | Where-Object {{ $p = $pf[$_.InstanceID]; -not $_.Owner -and $sf[$_.InstanceID] -eq 'Any' -and ($app[$_.InstanceID] -eq $e -or $app[$_.InstanceID] -eq 'Any') -and ([string]$p.Protocol -eq 'Any' -or [string]$p.Protocol -eq 'TCP') -and @($p.LocalPort | Where-Object {{ $_ -eq 'Any' -or $_ -eq [string]$port -or ($_ -match '^(\\d+)-(\\d+)$' -and [int]$Matches[1] -le $port -and $port -le [int]$Matches[2]) }}).Count -gt 0 -and ([string]$_.Action -eq 'Block' -or @($af[$_.InstanceID].RemoteAddress | Where-Object {{ $_ -in 'Any', 'LocalSubnet', 'LocalSubnet4' }}).Count -gt 0) }} | ForEach-Object {{ $m = [int]$_.Profile; if ($m -eq 0) {{ $m = 7 }}; [pscustomobject]@{{ Name = $_.Name; Action = [string]$_.Action; Profile = $m -band $on }} }} | Where-Object {{ $_.Profile -ne 0 }}); \
         $blocks = @($r | Where-Object {{ $_.Action -eq 'Block' }}); \
         if (@($blocks | Where-Object {{ $_.Name -notlike 'TCP Query User{{*' }}).Count -gt 0) {{ exit 4 }}; if ($blocks.Count -gt 0) {{ exit 3 }}; \
         foreach ($x in $r) {{ if ($x.Action -eq 'Allow') {{ $need = $need -band -bnot $x.Profile }} }}; if ($need -eq 0) {{ exit 0 }}; exit 3",
        path_literal(exe)
    )
}

pub(crate) fn allow_script(exe: &str, port: u16) -> String {
    let elevated = format!(
        "{}; Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -eq $e }} | Get-NetFirewallRule | Where-Object {{ [string]$_.Direction -eq 'Inbound' -and (([string]$_.Action -eq 'Block' -and $_.Name -like 'TCP Query User{{*') -or $_.DisplayName -eq '{RULE}') }} | Remove-NetFirewallRule -ErrorAction SilentlyContinue; \
         New-NetFirewallRule -DisplayName '{RULE}' -Direction Inbound -Action Allow -Program $e -Protocol TCP -LocalPort {port} -RemoteAddress LocalSubnet -Profile Any -ErrorAction Stop | Out-Null",
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
