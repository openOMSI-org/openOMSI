# The openOMSI dedicated server, asked for what it needs (run-server.cmd starts this with a
# double click): the first time the OMSI 2 folder and the main settings of server.cfg, every
# time the map (a numbered list of the maps installed); and, if wanted, a start-<map>.cmd
# beside it that starts that map again without a question. The server's output goes to this
# window and to server.log.
#
#   run-server.cmd              ask for the map (and the rest the first time)
#   run-server.cmd -Setup       ask for the folder and the settings again
#   run-server.cmd -Map maps/Grundorf/global.cfg   no question: this map
#
# Windows PowerShell 5.1 and PowerShell 7 alike. (start.cmd stays as it was.)
param(
    [switch]$Setup,
    [string]$Map = '',
    # (a start-<map>.cmd: the map saved under this name in run-server.conf)
    [string]$Preset = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2

$here = $PSScriptRoot
$exe = Join-Path $here 'openomsi.exe'
if (-not (Test-Path -LiteralPath $exe)) { $exe = Join-Path $here 'openomsi' }
$conf = Join-Path $here 'run-server.conf'
$cfg = Join-Path $here 'server.cfg'
$log = Join-Path $here 'server.log'
$utf8 = New-Object System.Text.UTF8Encoding $false
# (openomsi writes UTF-8: map folders with accents read right)
$oldOut = [Console]::OutputEncoding
[Console]::OutputEncoding = $utf8

$pt = (Get-UICulture).Name -like 'pt*' -or (Get-Culture).Name -like 'pt*'
function T([string]$en, [string]$ptText) { if ($pt) { $ptText } else { $en } }
function Say([string]$en, [string]$ptText) { Write-Host (T $en $ptText) }
function Quit([int]$code, [string]$en, [string]$ptText) {
    if ($en) { Write-Host (T $en $ptText) -ForegroundColor Yellow }
    [Console]::OutputEncoding = $oldOut
    exit $code
}

if (-not (Test-Path -LiteralPath $exe)) {
    Quit 1 "openomsi.exe is not beside this script ($here)" "o openomsi.exe não está ao lado deste script ($here)"
}

# run-server.conf: root=<OMSI 2 folder>, last=<last map>, preset.<name>=<map>
# (the lines one by one into the pipeline: wrap the call in @() for an array)
function Read-Lines([string]$path) {
    if (Test-Path -LiteralPath $path) { [IO.File]::ReadAllLines($path, $utf8) }
}
# openomsi.exe that Windows did not start (an antivirus, Smart App Control, a download still
# blocked): the reason and the way out - no other folder or map would do
function Not-Started([string]$why) {
    Quit 1 "openomsi.exe could not be started: $why`nWindows or an antivirus may block a program just downloaded: right-click the .zip, Properties, tick Unblock and extract it again; or allow openomsi.exe in Windows Security (Protection history) or your antivirus." "O openomsi.exe não pôde ser iniciado: $why`nO Windows ou um antivírus podem bloquear um programa recém-baixado: clique com o botão direito no .zip, Propriedades, marque Desbloquear e extraia de novo; ou permita o openomsi.exe na Segurança do Windows (Histórico de proteção) ou no seu antivírus."
}

# one argument as Windows programs read their command line (spaces, quotes, a backslash
# before a quote)
function Quote-Arg([string]$a) {
    if ($a -and $a -notmatch '[\s"]') { return $a }
    $a = $a -replace '(\\*)"', '$1$1\"'
    $a = $a -replace '(\\+)$', '$1$1'
    return '"' + $a + '"'
}

# openomsi.exe is a windowed program: run with &, PowerShell neither waits for it nor keeps
# its exit code (the map list came after the prompt, $LASTEXITCODE unset). Started here, its
# output comes through pipes, and it is waited for.
function Start-Openomsi([string[]]$argv) {
    $si = New-Object System.Diagnostics.ProcessStartInfo
    $si.FileName = $exe
    $si.Arguments = (@($argv | ForEach-Object { Quote-Arg $_ }) -join ' ')
    $si.UseShellExecute = $false
    $si.CreateNoWindow = $true
    $si.RedirectStandardOutput = $true
    $si.RedirectStandardError = $true
    $si.StandardOutputEncoding = $utf8
    $si.StandardErrorEncoding = $utf8
    try { return [System.Diagnostics.Process]::Start($si) } catch { Not-Started $_.Exception.Message }
}

# every line of the program's output (stdout and stderr, as they come) to $each, until it ends;
# its exit code. (Short waits: Ctrl+C is not held up.)
function Pump-Openomsi($p, [scriptblock]$each) {
    $o = $p.StandardOutput.ReadLineAsync()
    $e = $p.StandardError.ReadLineAsync()
    while ($o -or $e) {
        $open = [System.Threading.Tasks.Task[]]@(@($o, $e) | Where-Object { $_ })
        [void][System.Threading.Tasks.Task]::WaitAny($open, 200)
        if ($o -and $o.IsCompleted) {
            if ($null -eq $o.Result) { $o = $null } else { & $each $o.Result $false; $o = $p.StandardOutput.ReadLineAsync() }
        }
        if ($e -and $e.IsCompleted) {
            if ($null -eq $e.Result) { $e = $null } else { & $each $e.Result $true; $e = $p.StandardError.ReadLineAsync() }
        }
    }
    $p.WaitForExit()
    return $p.ExitCode
}

function Conf-Get([string]$key) {
    $v = ''
    foreach ($l in (Read-Lines $conf)) { if ($l.StartsWith("$key=")) { $v = $l.Substring($key.Length + 1) } }
    return $v
}
function Conf-Set([string]$key, [string]$value) {
    $lines = @(Read-Lines $conf | Where-Object { -not $_.StartsWith("$key=") })
    $lines += "$key=$value"
    [IO.File]::WriteAllLines($conf, [string[]]$lines, $utf8)
}

# server.cfg: the value of a key, and a key set (its line replaced, or added at the end)
function Cfg-Key([string]$line) {
    $t = $line.Trim()
    if ($t.StartsWith('#') -or $t.IndexOf('=') -lt 0) { return $null }
    return $t.Substring(0, $t.IndexOf('=')).Trim().ToLowerInvariant()
}
function Cfg-Get([string]$key) {
    $v = ''
    foreach ($l in (Read-Lines $cfg)) { if ((Cfg-Key $l) -eq $key) { $v = $l.Substring($l.IndexOf('=') + 1).Trim() } }
    return $v
}
function Cfg-Set([string]$key, [string]$value) {
    $out = New-Object System.Collections.Generic.List[string]
    $done = $false
    foreach ($l in (Read-Lines $cfg)) {
        if ((Cfg-Key $l) -eq $key) {
            if (-not $done) { $out.Add("$key = $value") }
            $done = $true
        } else { $out.Add($l) }
    }
    if (-not $done) { $out.Add("$key = $value") }
    [IO.File]::WriteAllLines($cfg, $out.ToArray(), $utf8)
}

# the installed maps of a folder (server.cfg written with its defaults when missing); $null
# with the reason shown when the folder is not a complete OMSI 2
function List-Maps([string]$root) {
    $p = Start-Openomsi @('--root', $root, '--server', $cfg, '--list-maps')
    $maps = New-Object System.Collections.Generic.List[string]
    $said = New-Object System.Collections.Generic.List[string]
    $code = Pump-Openomsi $p { param($line, $err) if (-not $line.Trim()) { return } if ($err) { $said.Add($line) } else { $maps.Add($line) } }
    if ($code -eq 0) { return ,[string[]]$maps.ToArray() }
    foreach ($t in @($said) + @($maps)) {
        if ($t -notmatch ' INFO ') { Write-Host $t -ForegroundColor Yellow }
    }
    return $null
}

function Ask([string]$en, [string]$ptText) { return Read-Host (T $en $ptText) }

# a folder as typed: no quotes, and no backslash at its end (passed on as "D:\Games\OMSI 2\",
# Windows PowerShell 5.1 hands the program a quote that swallows the next arguments) - a
# drive's own D:\ stays
function Clean-Folder([string]$a) {
    $a = $a.Trim().Trim('"').Trim("'").Trim()
    if ($a -match '^(.*[^:\\])\\+$') { $a = $Matches[1] }
    return $a
}

# The server ends with this window. Its output comes through a pipe, so it gets neither the
# window's closing nor Ctrl+C: closed, the window left it running and holding its port (the
# next start then stopped with code 3). This PowerShell joins a job whose processes all end
# when its last handle goes - this PowerShell's own, when it ends; the server, started from
# it, is in the job too. (Nothing on another system, or where Windows refuses: as before.)
function Join-WindowJob {
    if ($env:OS -ne 'Windows_NT') { return }
    try {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OpenOmsiWindowJob {
    [StructLayout(LayoutKind.Sequential)]
    struct Basic { public long PerProcessUserTimeLimit; public long PerJobUserTimeLimit; public uint LimitFlags; public UIntPtr MinimumWorkingSetSize; public UIntPtr MaximumWorkingSetSize; public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass; public uint SchedulingClass; }
    [StructLayout(LayoutKind.Sequential)]
    struct Io { public ulong ReadOps; public ulong WriteOps; public ulong OtherOps; public ulong ReadBytes; public ulong WriteBytes; public ulong OtherBytes; }
    [StructLayout(LayoutKind.Sequential)]
    struct Extended { public Basic BasicLimits; public Io IoInfo; public UIntPtr ProcessMemoryLimit; public UIntPtr JobMemoryLimit; public UIntPtr PeakProcessMemoryUsed; public UIntPtr PeakJobMemoryUsed; }
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int infoClass, ref Extended info, uint length);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    // (kept open for this process's life: its end is the job's)
    static IntPtr job;
    public static bool Join() {
        job = CreateJobObject(IntPtr.Zero, null);
        if (job == IntPtr.Zero) return false;
        Extended info = new Extended();
        info.BasicLimits.LimitFlags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if (!SetInformationJobObject(job, 9, ref info, (uint)Marshal.SizeOf(typeof(Extended)))) return false; // JobObjectExtendedLimitInformation
        return AssignProcessToJobObject(job, GetCurrentProcess());
    }
}
'@
        if (-not [OpenOmsiWindowJob]::Join()) { Say 'Note: closing this window may leave the server running (Task Manager ends it).' 'Aviso: fechar esta janela pode deixar o servidor rodando (o Gerenciador de Tarefas o encerra).' }
    } catch {
        Say 'Note: closing this window may leave the server running (Task Manager ends it).' 'Aviso: fechar esta janela pode deixar o servidor rodando (o Gerenciador de Tarefas o encerra).'
    }
}

$root = Conf-Get 'root'
if ($root) { $root = Clean-Folder $root }
$first = [bool]($root -and -not (Test-Path -LiteralPath $cfg))
$maps = $null
if (-not $root -or $Setup) {
    $first = $true
    while ($true) {
        $a = Ask 'Folder of the original OMSI 2 (the one with Omsi.exe, maps and Vehicles)' 'Pasta do OMSI 2 original (a que tem Omsi.exe, maps e Vehicles)'
        if ($null -eq $a) { Quit 1 '' '' }
        # (a path pasted from Explorer comes in quotes)
        $a = Clean-Folder $a
        if (-not $a) { continue }
        $maps = List-Maps $a
        if ($null -ne $maps) { $root = $a; Conf-Set 'root' $root; break }
        Say 'That folder is not a complete OMSI 2 (see above). Try again, or Ctrl+C.' 'Essa pasta não é um OMSI 2 completo (veja acima). Tente de novo, ou Ctrl+C.'
    }
} else {
    $maps = List-Maps $root
    if ($null -eq $maps) {
        Quit 1 'The OMSI 2 folder saved in run-server.conf no longer works: run-server.cmd -Setup' 'A pasta do OMSI 2 salva em run-server.conf não funciona mais: run-server.cmd -Setup'
    }
}

# the main settings, the first time (Enter keeps what server.cfg has)
function Number([string]$key, [string]$en, [string]$ptText, [int]$lo, [int]$hi) {
    while ($true) {
        $v = Ask "$en [$(Cfg-Get $key)]" "$ptText [$(Cfg-Get $key)]"
        if (-not $v) { return }
        $n = 0
        if ([int]::TryParse($v.Trim(), [ref]$n) -and $n -ge $lo -and $n -le $hi) { Cfg-Set $key "$n"; return }
        Say "A whole number from $lo to $hi." "Um número inteiro de $lo a $hi."
    }
}
if ($first) {
    Write-Host ''
    Say 'Server settings (Enter keeps the value in brackets; server.cfg has the others):' 'Configurações do servidor (Enter mantém o valor entre colchetes; o server.cfg tem as outras):'
    $v = Ask "Server name [$(Cfg-Get 'name')]" "Nome do servidor [$(Cfg-Get 'name')]"
    if ($v -and $v.Trim()) { Cfg-Set 'name' $v.Trim() }
    Number 'max_players' 'Players at most (1-64)' 'Máximo de jogadores (1-64)' 1 64
    Number 'port' 'UDP port of the game' 'Porta UDP do jogo' 1 65535
    Number 'web_port' 'Web port (status page, players by WebSocket)' 'Porta web (página de status, jogadores por WebSocket)' 1 65535
    while ($true) {
        $v = Ask "Public address through a Cloudflare tunnel (no port forwarding needed)? [y/n, now $(Cfg-Get 'tunnel')]" "Endereço público por túnel da Cloudflare (sem abrir porta no roteador)? [s/n, agora $(Cfg-Get 'tunnel')]"
        if (-not $v) { break }
        if ($v -match '^[yYsS]') { Cfg-Set 'tunnel' '1'; break }
        if ($v -match '^[nN]') { Cfg-Set 'tunnel' '0'; break }
    }
    while ($true) {
        if (Cfg-Get 'admin_password') {
            $prompt = T 'Admin password (12+ characters; Enter keeps the one set; - for none)' 'Senha de admin (12+ caracteres; Enter mantém a atual; - para nenhuma)'
        } else {
            $prompt = T 'Admin password (12+ characters; Enter for none)' 'Senha de admin (12+ caracteres; Enter para nenhuma)'
        }
        if ([Console]::IsInputRedirected) {
            # (the answers come from a file or another program: no typing to hide, and a
            # hidden read would end PowerShell there)
            $v = Read-Host $prompt
        } else {
            $secure = Read-Host $prompt -AsSecureString
            $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
            try { $v = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr) }
        }
        # (one kept from before the 12 characters: the server would ignore it)
        $kept = Cfg-Get 'admin_password'
        if (-not $v -and $kept -and $kept.Length -lt 12) { Say 'The password set is shorter than 12 characters, and the server ignores it: type a new one (- for none).' 'A senha atual tem menos de 12 caracteres, e o servidor a ignora: digite uma nova (- para nenhuma).'; if (-not [Console]::IsInputRedirected) { continue } }
        if (-not $v) { break }
        if ($v -eq '-') { Cfg-Set 'admin_password' ''; break }
        if ($v.Length -lt 12) { Say 'At least 12 characters.' 'Pelo menos 12 caracteres.'; continue }
        if ($v -ne $v.Trim()) { Say 'No spaces at the start or the end (server.cfg would cut them).' 'Sem espaço no começo ou no fim (o server.cfg cortaria).'; continue }
        Cfg-Set 'admin_password' $v
        break
    }
    Say "Saved in $cfg." "Salvo em $cfg."
}

# the map: a preset's, given, or picked from the list
if ($maps.Count -eq 0) { Quit 1 "No map is installed in $root\maps." "Nenhum mapa instalado em $root\maps." }
$askShortcut = $false
if ($Preset) {
    $Map = Conf-Get "preset.$Preset"
    if (-not $Map) { Quit 1 "run-server.conf has no map for start-$Preset.cmd: run run-server.cmd once more." "o run-server.conf não tem mapa para start-$Preset.cmd: rode o run-server.cmd de novo." }
}
if ($Map) {
    $Map = $Map.Replace('\', '/')
    if ($maps -notcontains $Map) {
        Say "The map $Map is not installed. Installed:" "O mapa $Map não está instalado. Instalados:"
        foreach ($m in $maps) { Write-Host "  $m" }
        Quit 2 '' ''
    }
} else {
    $last = Conf-Get 'last'
    if (-not $last) { $last = Cfg-Get 'map' }
    Write-Host ''
    Say 'Maps:' 'Mapas:'
    $dflt = 0
    for ($i = 0; $i -lt $maps.Count; $i++) {
        $name = $maps[$i] -replace '^maps/', '' -replace '/global\.cfg$', ''
        $mark = ''
        if ($maps[$i] -eq $last) { $dflt = $i + 1; $mark = ' *' }
        Write-Host ('  {0,3}  {1}{2}' -f ($i + 1), $name, $mark)
    }
    while ($true) {
        if ($dflt) { $v = Ask "Map number [Enter: $dflt]" "Número do mapa [Enter: $dflt]" } else { $v = Ask 'Map number' 'Número do mapa' }
        if (-not $v -and $dflt) { $v = "$dflt" }
        $n = 0
        if ($v -and [int]::TryParse($v.Trim(), [ref]$n) -and $n -ge 1 -and $n -le $maps.Count) { $Map = $maps[$n - 1]; break }
    }
    $askShortcut = $true
}
Conf-Set 'last' $Map

# a shortcut that starts this map again with no question (only when asked); its file holds
# only plain letters, the map is in run-server.conf (UTF-8), never in the .cmd's code page
if ($askShortcut) {
    $name = $Map -replace '^maps/', '' -replace '/global\.cfg$', ''
    $plain = -join ($name.Normalize([Text.NormalizationForm]::FormD).ToCharArray() | Where-Object { [Globalization.CharUnicodeInfo]::GetUnicodeCategory($_) -ne 'NonSpacingMark' })
    $slug = ($plain -replace '[^A-Za-z0-9._-]+', '-').Trim('-')
    if (-not $slug) { $slug = 'map' }
    $shortcut = Join-Path $here "start-$slug.cmd"
    if (-not (Test-Path -LiteralPath $shortcut)) {
        $v = Ask "Create start-$slug.cmd, which starts this map again without asking? [y/N]" "Criar start-$slug.cmd, que inicia este mapa de novo sem perguntar? [s/N]"
        if ($v -match '^[yYsS]') {
            Conf-Set "preset.$slug" $Map
            $text = "@echo off`r`nrem openOMSI server on one map, with run-server.cmd (made by it)`r`ncall `"%~dp0run-server.cmd`" -Preset `"$slug`" %*`r`n"
            [IO.File]::WriteAllText($shortcut, $text, (New-Object System.Text.ASCIIEncoding))
            Say "Made $shortcut." "Criado $shortcut."
        }
    }
}

Write-Host ''
Say "Starting on $Map (the log also goes to server.log; Ctrl+C stops the server)." "Iniciando em $Map (o log também vai para server.log; Ctrl+C para o servidor)."
Join-WindowJob
$writer = New-Object System.IO.StreamWriter($log, $true, $utf8)
$writer.AutoFlush = $true
$code = 0
$p = Start-Openomsi @('--root', $root, '--server', $cfg, '--server-map', $Map)
try {
    $code = Pump-Openomsi $p { param($line, $err) Write-Host $line; $writer.WriteLine($line) }
} finally {
    $writer.Dispose()
    # (Ctrl+C: the server ends with this script)
    if (-not $p.HasExited) { try { $p.Kill() } catch { } }
}
switch ($code) {
    0 { }
    1 { Say 'The server stopped (code 1): the OMSI 2 folder is incomplete or wrong, or another error - see the lines above. run-server.cmd -Setup chooses the folder again.' 'O servidor parou (código 1): a pasta do OMSI 2 está incompleta ou errada, ou outro erro - veja as linhas acima. run-server.cmd -Setup escolhe a pasta de novo.' }
    2 { Say 'The server stopped (code 2): server.cfg cannot be read, or the map is not installed - see the lines above.' 'O servidor parou (código 2): o server.cfg não pôde ser lido, ou o mapa não está instalado - veja as linhas acima.' }
    3 { Say 'The server stopped (code 3): its UDP port is taken (another server?). Stop it, or choose another port: run-server.cmd -Setup.' 'O servidor parou (código 3): a porta UDP está ocupada (outro servidor?). Feche-o, ou escolha outra porta: run-server.cmd -Setup.' }
    default { Say "The server stopped (code $code) - see the lines above and server.log." "O servidor parou (código $code) - veja as linhas acima e o server.log." }
}
Quit $code '' ''
