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
    Some((what.chars().take(600).collect(), tail))
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
