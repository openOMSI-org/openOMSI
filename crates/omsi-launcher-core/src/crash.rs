/// The server's message when a game ended because its server sent it away or turned it away
/// (`lan::LEFT_SERVER` in the log of this run); None for any other end.
pub fn disconnect_of(log: &std::path::Path) -> Option<String> {
    let text = std::fs::read(log).ok()?;
    let text = String::from_utf8_lossy(&text[text.len().saturating_sub(64 * 1024)..]).to_string();
    let all: Vec<&str> = text.lines().collect();
    let run = &all[all.iter().rposition(|l| l.contains("starting the game:")).unwrap_or(0)..];
    let line = run.iter().rev().find(|l| l.contains(omsi_net::LEFT_SERVER))?;
    let why = line.split_once(omsi_net::LEFT_SERVER)?.1.trim();
    Some(if why.is_empty() { "sent away by the host".to_string() } else { why.chars().take(300).collect() })
}

/// What a game's log says when the game ended on an error: the error (a panic, "no graphics
/// adapter", a fatal message) and the last lines of the log. None for a game that ended as
/// it should.
pub fn crash_of(log: &std::path::Path) -> Option<(String, String)> {
    let bytes = std::fs::read(log).ok()?;
    let text = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(64 * 1024)..]).to_string();
    let all: Vec<&str> = text.lines().collect();
    // (the run itself only: an error the launcher logged before the game started - a
    // preview's picture left out - titled the report of a game that died much later, and
    // the phone's "died compiling a shader" hint never showed, #381, #331)
    let lines: Vec<&str> = match all.iter().rposition(|l| l.contains("starting the game:")) {
        Some(k) => all[k..].to_vec(),
        None => all.clone(),
    };
    // (an error the game got over - "the game goes on", a part of the picture left out -
    // is no crash)
    let recovered = |l: &str| l.contains("the game goes on") || l.contains("left out") || l.contains("could not be recorded");
    // (a lost graphics device ends the game in order - it saves the run - but it is a crash
    // for the player all the same: the driver gave up)
    // (one the game got over by starting again with safer graphics is no crash)
    if lines.iter().any(|l| l.contains("starting again with safer graphics")) {
        return None;
    }
    let lost = lines.iter().rposition(|l| l.contains("the graphics device was lost"));
    if lost.is_none() && lines.iter().any(|l| l.contains("game ends")) {
        return None;
    }
    let at = lost.or_else(|| lines.iter().rposition(|l| l.contains("the game stopped on an error") || (l.contains(" ERROR ") && !recovered(l))))?;
    let first = lines[at].split_once("] ").map(|x| x.1).unwrap_or(lines[at]).trim();
    // (a panic's message is on the following lines)
    let mut what = first.to_string();
    // (and stops at the next record: a lost device's line had "game ends" and the tile
    // loading after it in the report's title, #1187)
    for l in lines.iter().skip(at + 1).take(6) {
        if l.trim().is_empty() || l.trim_start().starts_with("0:") || l.starts_with('[') {
            break;
        }
        what.push(' ');
        what.push_str(l.trim());
    }
    // the computer, its graphics card and the command line (the map) from the start of the
    // run, before the end of the log and a line `…`: a report of the end alone never said
    // what it happened on (#1187). Its GitHub link keeps them and shortens only the end.
    let whole = String::from_utf8_lossy(&bytes);
    let machine: Vec<&str> = MACHINE_LINES.iter().filter_map(|k| whole.lines().rfind(|l| l.contains(k))).collect();
    let end = all[all.len().saturating_sub(150)..].join("\n");
    let tail = if machine.is_empty() { end } else { format!("{}\n{CRASH_TAIL_GAP}\n{end}", machine.join("\n")) };
    let what = redact(&what);
    Some((what.chars().take(600).collect(), redact(&tail)))
}

/// A crash report without what names the player: their account's folder (the home folder
/// becomes `~`, any other account's name in a path `<user>`), their driver
/// (`Drivers/<name>.odr`, often the player's own name, on the command line), the other
/// players' names in the multiplayer lines and the chat. The report goes to a public GitHub
/// issue.
pub fn redact(text: &str) -> String {
    let home = ["USERPROFILE", "HOME"].iter().find_map(|k| std::env::var(k).ok());
    redact_with(text, home.as_deref())
}

fn redact_with(text: &str, home: Option<&str>) -> String {
    let mut out = text.to_string();
    // (a home folder that is a drive or the root names nobody)
    if let Some(home) = home.map(|h| h.trim_end_matches(['/', '\\'])).filter(|h| h.len() > 3) {
        for h in [home.to_string(), home.replace('\\', "/"), home.replace('/', "\\")] {
            out = replace_whole(&out, &h, "~");
        }
    }
    let user_end = |rest: &str| {
        let end = rest.find(['\\', '/', '"', '\'', '\n', '\r']).unwrap_or(rest.len());
        // (a name at the end of a path stops at the next word: "C:\\Users\\x exited")
        if rest[end..].starts_with(['\\', '/']) { end } else { rest[..end].find(char::is_whitespace).unwrap_or(end) }
    };
    out = replace_segments(&out, &["\\users\\", "/users/", "/home/"], user_end, "<user>");
    let driver_end = |rest: &str| {
        let line = &rest[..rest.find(['\n', '\r', '"', '\'', '/', '\\']).unwrap_or(rest.len())];
        line.to_ascii_lowercase().find(".odr").unwrap_or(0)
    };
    out = replace_segments(&out, &["drivers/", "drivers\\"], driver_end, "<driver>");
    // the other players: their names in the multiplayer lines, and the chat left out
    let quoted = |rest: &str| rest.find(['\'', '\n']).filter(|&n| rest[n..].starts_with('\'')).unwrap_or(0);
    out = replace_segments(&out, &["lan: '", "connected to '"], quoted, "<player>");
    out = player_names(&out);
    out.lines().map(|l| match l.find("LAN chat") {
        Some(i) => format!("{} (left out of the report)", &l[..i + "LAN chat".len()]),
        None => l.to_string(),
    }).collect::<Vec<_>>().join("\n")
        + if out.ends_with('\n') { "\n" } else { "" }
}

/// `player 3 'Name'` as `player 3 '<player>'`.
fn player_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(i) = text[at..].find("player ").map(|i| at + i + "player ".len()) {
        let digits = text[i..].find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len() - i);
        let open = i + digits;
        let name = (digits > 0 && text[open..].starts_with(" '")).then(|| &text[open + 2..]).and_then(|r| r.find(['\'', '\n']).filter(|&n| r[n..].starts_with('\'')));
        out.push_str(&text[at..i]);
        match name {
            Some(n) => {
                out.push_str(&text[i..open + 2]);
                out.push_str("<player>");
                at = open + 2 + n;
            }
            None => at = i,
        }
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with `what` replaced where it is a whole name (`/root`, not the start of `/rootfs`).
fn replace_whole(text: &str, what: &str, with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(i) = text[at..].find(what).map(|i| at + i) {
        let after = &text[i + what.len()..];
        let whole = !after.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '-');
        out.push_str(&text[at..i]);
        out.push_str(if whole { with } else { what });
        at = i + what.len();
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with the part after each of `markers` (any case) that `end` measures replaced by `with`.
fn replace_segments(text: &str, markers: &[&str], end: impl Fn(&str) -> usize, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some((start, m)) = markers.iter().filter_map(|m| lower[at..].find(m).map(|i| (at + i, m.len()))).min() {
        let from = start + m;
        let n = end(&text[from..]);
        out.push_str(&text[at..from]);
        let seg = &text[from..from + n];
        out.push_str(if n == 0 || seg == with { seg } else { with });
        at = from + n;
    }
    out.push_str(&text[at..]);
    out
}

/// The log lines that tell what a game ran on (see `crash_of`).
const MACHINE_LINES: [&str; 4] = ["] system: ", "] graphics adapter: ", "] opening graphics device: ", "] command line: "];

/// The line between the machine and the end of the log in a crash's tail.
pub const CRASH_TAIL_GAP: &str = "...";

#[cfg(test)]
mod disconnect_tests {
    #[test]
    fn the_servers_word_is_read_back_from_the_log_of_this_run() {
        let dir = std::env::temp_dir().join(format!("openomsi-disc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("game.log");
        let left = omsi_net::LEFT_SERVER;
        std::fs::write(&p, format!("[x INFO a] starting the game: one\n[x WARN openomsi_game::lan] {left}old\n[x INFO a] starting the game: two\n[x WARN openomsi_game::lan] {left}Expulsé : conduite dangereuse\n[x INFO a] game ends\n")).unwrap();
        assert_eq!(super::disconnect_of(&p).as_deref(), Some("Expulsé : conduite dangereuse"));
        // a run that ended in any other way: nothing
        std::fs::write(&p, format!("[x INFO a] starting the game: one\n[x WARN openomsi_game::lan] {left}old\n[x INFO a] starting the game: two\n[x INFO a] game ends\n")).unwrap();
        assert!(super::disconnect_of(&p).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The run went down while a Vulkan driver compiled the shaders: the LAST it said was a stage
/// of that, and it drew with Vulkan (as the phone's shell decides it, `android.rs`). Any
/// compile stage anywhere in the log said so of every silent end - a phone run out of memory
/// 75 % into loading a map on OpenGL was told its Vulkan driver had failed (#848).
pub fn died_compiling_on_vulkan(log: &str) -> bool {
    let last = log.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    let compiling = last.contains("renderer: compiling") || last.contains("cloud noise made") || last.contains("opening graphics device") || last.contains("compiling renderer pipelines");
    let vulkan = log.lines().any(|l| (l.contains("renderer: ") || l.contains("opening graphics device")) && l.contains("(Vulkan")) || log.lines().any(|l| l.contains("graphics: ") && l.to_ascii_uppercase().contains("VULKAN"));
    compiling && vulkan
}

#[cfg(test)]
mod hint_tests {
    #[test]
    fn only_a_run_that_died_compiling_on_vulkan_is_told_so() {
        let compiled = "[t INFO r] opening graphics device: Mali (Vulkan, vendor 0x13b5)\n[t INFO r] renderer: compiling the scene shaders\n";
        assert!(super::died_compiling_on_vulkan(compiled));
        // compiled long ago, then ran out of memory loading
        assert!(!super::died_compiling_on_vulkan(&format!("{compiled}[t INFO g] status: 63 fps, view driver\n[t INFO m] loading tiles 75 %\n")));
        // on OpenGL it is never the Vulkan driver
        assert!(!super::died_compiling_on_vulkan("[t INFO r] opening graphics device: Mali (Gl, vendor 0x13b5)\n[t INFO r] renderer: compiling the scene shaders\n"));
    }
}

#[cfg(test)]
mod redact_tests {
    use super::redact_with;

    #[test]
    fn a_report_names_neither_the_account_nor_the_driver() {
        let log = "[t INFO a] command line: C:\\Users\\Isaac Santana\\openomsi.exe --root D:\\OMSI 2 --driver Drivers/Isaac S.odr\n\
            [t INFO a] working folder: C:/Users/Isaac Santana/AppData\n\
            [t WARN a] /home/rafa/.openomsi/x.cfg and /Users/ana/Library\n\
            [t ERROR a] panicked at C:\\Users\\other\n\
            [t INFO a] DRIVERS\\Joao.ODR read; vehicle Vehicles/MAN/MAN.bus";
        let r = redact_with(log, Some("C:\\Users\\Isaac Santana"));
        for gone in ["Isaac", "rafa", "ana/", "other", "Joao"] {
            assert!(!r.contains(gone), "{gone}: {r}");
        }
        assert!(r.contains("command line: ~\\openomsi.exe --root D:\\OMSI 2 --driver Drivers/<driver>.odr"), "{r}");
        assert!(r.contains("working folder: ~/AppData"), "{r}");
        assert!(r.contains("/home/<user>/.openomsi/x.cfg and /Users/<user>/Library"), "{r}");
        assert!(r.contains("panicked at C:\\Users\\<user>\n"), "{r}");
        assert!(r.contains("DRIVERS\\<driver>.ODR read; vehicle Vehicles/MAN/MAN.bus"), "{r}");
        // what names nobody stays as it is
        let plain = "[t ERROR omsi_render] the graphics device was lost (Unknown)\n[t INFO a] maps/Grundorf/global.cfg";
        assert_eq!(redact_with(plain, Some("/")), plain);
        assert_eq!(redact_with("Drivers are fine", None), "Drivers are fine");
        // the other players: no names, no chat
        let lan = "[t INFO omsi_net] LAN: player 3 'Ana Maria' left\n[t INFO omsi_net] LAN chat <Ana> meu numero 9999\n[t INFO omsi_net] LAN: connected to 'Isaac host' (session 1f), we are player 2; weather 'rain'\n[t WARN omsi_net] LAN: 'Bob' at 203.0.113.x wants to join\n[t INFO a] player 4 skipped\n";
        let r = redact_with(lan, None);
        for gone in ["Ana", "9999", "Isaac", "Bob"] {
            assert!(!r.contains(gone), "{gone}: {r}");
        }
        assert!(r.contains("LAN: player 3 '<player>' left\n") && r.contains("] LAN chat (left out of the report)\n"), "{r}");
        assert!(r.contains("connected to '<player>' (session 1f), we are player 2; weather 'rain'") && r.contains("LAN: '<player>' at"), "{r}");
        assert!(r.ends_with("player 4 skipped\n"), "{r}");
        // (the home folder itself, not a longer name it begins)
        assert_eq!(redact_with("/root/a /rootfs/b", Some("/root")), "~/a /rootfs/b");
    }
}

#[cfg(test)]
mod crash_tests {
    #[test]
    fn a_panic_is_found_and_a_clean_end_is_not() {
        let dir = std::env::temp_dir().join("openomsi-crash-test");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("game.log");
        std::fs::write(&p, "[t INFO x] loading\n[t ERROR openomsi_game] the game stopped on an error (build x): panicked at a.rs:1:1:\n    index out of bounds\n\n   0: std::backtrace\n").unwrap();
        let (what, tail) = super::crash_of(&p).unwrap();
        assert!(what.contains("index out of bounds"), "{what}");
        assert!(tail.contains("loading"));
        std::fs::write(&p, "[t INFO x] loading\n[t INFO openomsi_game::app_events] game ends\n").unwrap();
        assert!(super::crash_of(&p).is_none());
        std::fs::write(&p, "[t ERROR omsi_render] the graphics device was lost (Unknown): Unexpected error variant\n[t INFO openomsi_game::app_events] game ends\n").unwrap();
        assert!(super::crash_of(&p).unwrap().0.contains("device was lost"));
        // an error before the game started, or one it got over, is not the crash
        std::fs::write(&p, "[t ERROR omsi_render] a part of the picture could not be recorded (left out)\n[t INFO x] starting the game: omsi\n[t INFO omsi_render] renderer: compiling the sky and clouds shaders\n").unwrap();
        assert!(super::crash_of(&p).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The report says what the game ran on (the start of the log), and its title is the
    /// error alone, not the records after it (#1187).
    #[test]
    fn the_report_has_the_computer_and_a_clean_title() {
        let dir = std::env::temp_dir().join(format!("openomsi-crash-machine-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("game.log");
        let mut log = String::from("[t INFO openomsi_game::applog] system: windows x86_64 (10.0), 8 threads, 8192 MB memory\n[t INFO openomsi_game::applog] command line: openomsi --map maps/X/global.cfg\n[t INFO omsi_render] graphics adapter: GTX 750 (DiscreteGpu, Dx12, 2048 MB of its own), texture memory taken for it: 716 MB\n");
        // (more than the end that goes with the report)
        for k in 0..400 {
            log.push_str(&format!("[t INFO openomsi_game::scene] tile loading: placed tile {k},0\n"));
        }
        log.push_str("[t ERROR openomsi_game::app_events] ending the session: the graphics device was lost (Unknown: Out of memory)\n[t INFO openomsi_game::app_events] game ends\n[t WARN openomsi_game::scene] tile loading: first-area batch prepared in 352.71 s\n");
        std::fs::write(&p, log).unwrap();
        let (what, tail) = super::crash_of(&p).unwrap();
        assert_eq!(what, "ending the session: the graphics device was lost (Unknown: Out of memory)");
        let (machine, end) = tail.split_once(&format!("\n{}\n", super::CRASH_TAIL_GAP)).unwrap();
        assert!(machine.contains("8192 MB memory") && machine.contains("GTX 750") && machine.contains("maps/X/global.cfg"), "{machine}");
        assert!(end.contains("352.71 s") && !end.contains("placed tile 0,0"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
