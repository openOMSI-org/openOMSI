//! Tests of the compiler (`compile.rs`).

use super::*;

fn program_of(script: &str) -> Program {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsi_gearbox_{}_{id}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let osc = dir.join("g.osc");
    std::fs::write(&osc, script).unwrap();
    // (the script's own variables are in a varlist, as a bus has them: a store to an
    // undeclared variable compiles to nothing)
    let mut locals: Vec<&str> = ["(L.L.", "(S.L."]
        .iter()
        .flat_map(|p| script.split(p).skip(1))
        .filter_map(|t| t.split(')').next())
        .filter(|n| *n != "Clutch")
        .collect();
    locals.sort_unstable();
    locals.dedup();
    let vars = dir.join("vars.txt");
    std::fs::write(&vars, locals.join("\n")).unwrap();
    let p = compile(&CompileInput {
        builtin_vars: vec!["Clutch".into()],
        varlists: vec![vars],
        scripts: vec![osc],
        ..Default::default()
    });
    let _ = std::fs::remove_dir_all(&dir);
    p
}

#[test]
fn a_manual_gearbox_is_told_from_an_automatic_with_gate_keys() {
    // the LiAZ KPP: gates, the first one asks for the clutch
    let kpp = program_of(
        "{trigger:kw_s_1} (L.L.Clutch) 1 = {if} 1 (S.L.g) {endif} {end}\n{trigger:kw_s_2} 2 (S.L.g) {end}\n",
    );
    assert!(kpp.manual_gearbox());
    // a manual bus can share cockpit code that also exposes automatic R/N/D. Its gear
    // triggers select the actual gear directly, while the clutch is read later in the
    // gearbox frame instead of inside kw_s_1.
    let shared = program_of(
        "{trigger:automatic_D} 1 (S.L.d) {end}\n{trigger:automatic_N} 0 (S.L.d) {end}\n{trigger:automatic_R} -1 (S.L.d) {end}\n{trigger:kw_s_1} 1 (S.L.antrieb_getr_gang) {end}\n{trigger:kw_s_2} 2 (S.L.antrieb_getr_gang) {end}\n{macro:gearbox_frame} (L.L.Clutch) (S.L.clutch_now) {end}\n",
    );
    assert!(shared.manual_gearbox());
    // an automatic with gear-hold keys and a torque converter's clutch elsewhere
    let auto = program_of(
        "{trigger:automatic_D} 1 (S.L.d) {end}\n{trigger:kw_s_1} 1 (S.L.hold) {end}\n{trigger:kw_s_2} 2 (S.L.hold) {end}\n{macro:conv} (L.L.Clutch) (S.L.c) {end}\n",
    );
    assert!(!auto.manual_gearbox());
    // gates and no automatic at all
    let plain = program_of("{trigger:kw_s_1} 1 (S.L.g) {end}\n{trigger:kw_s_2} 2 (S.L.g) {end}\n");
    assert!(plain.manual_gearbox());
    // a stock automatic: no gates
    assert!(!program_of("{trigger:automatic_D} 1 (S.L.d) {end}\n").manual_gearbox());
}

#[test]
fn an_end_inside_an_if_keeps_the_else_branch() {
    // Valiant Citybus doors: `{if} {end} {else} ... {endif}` - the else code must exist
    let prog = program_of(
        "{trigger:a}\n1 {if}\n{end}\n{else}\n2 (S.L.x)\n{endif}\n{end}\n{trigger:b} 3 (S.L.y) {end}\n",
    );
    let id = *prog.triggers.get("a").unwrap();
    assert!(
        prog.blocks[id as usize].ops.len() >= 6,
        "else branch was cut: {:?}",
        prog.blocks[id as usize].ops
    );
    assert!(prog.triggers.contains_key("b"));
}

#[test]
fn tokens() {
    let t = tokenize(r#"(L.L.a) 0.5 > {if} "Krueger 7x6" (M.V.GetFontIndex) $+ 'comment (x)"#);
    assert_eq!(
        t,
        vec![
            Tok::Paren("L.L.a".into()),
            Tok::Word("0.5".into()),
            Tok::Word(">".into()),
            Tok::Brace("if".into()),
            Tok::Str("Krueger 7x6".into()),
            Tok::Paren("M.V.GetFontIndex".into()),
            Tok::Word("$+".into()),
        ]
    );
    let t = tokenize(
        "\"churafont CE (8-1)x3\" (M.V.GetFontIndex) (S.L.Font_(8-1)x3) (L.L.a)(L.L.b) ( L.L.c )",
    );
    assert_eq!(
        t,
        vec![
            Tok::Str("churafont CE (8-1)x3".into()),
            Tok::Paren("M.V.GetFontIndex".into()),
            Tok::Paren("S.L.Font_(8-1)x3".into()),
            Tok::Paren("L.L.a".into()),
            Tok::Paren("L.L.b".into()),
            Tok::Paren("L.L.c".into()),
        ]
    );
}

#[test]
fn multiline_string_literals_compile_as_one_token() {
    let dir = std::env::temp_dir().join(format!("omsi-script-multiline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let osc = dir.join("display.osc");
    std::fs::write(
        &osc,
        "{init}\n\"Diagnostics 1/5\n\t@Fault Diagnostics\n\t@Cluster Selftest\n\" (S.$.screen)\n{end}\n",
    )
    .unwrap();
    let p = compile(&CompileInput {
        builtin_str_vars: vec!["screen".into()],
        scripts: vec![osc],
        ..Default::default()
    });
    let _ = std::fs::remove_dir_all(dir);

    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert!(
        p.strings
            .iter()
            .any(|s| s == "Diagnostics 1/5@Fault Diagnostics@Cluster Selftest")
    );
}

#[test]
fn unterminated_multiline_string_reports_an_error_and_keeps_later_blocks() {
    let p = program_of("{init}\n\"unterminated\n{frame}\n1 (S.L.x)\n{end}\n");

    assert!(
        p.errors
            .iter()
            .any(|e| e.message.contains("unterminated string"))
    );
    assert_eq!(p.frame.len(), 1);
}

/// Omsi.exe sub_5d1db8 keeps one entry of each kind, the first header's: identical or
/// distinct, a second `{init}` / `{frame}` / `{frame_ai}` is never an entry of its own.
#[test]
fn a_script_keeps_only_its_first_entry_block_of_each_kind() {
    for kind in ["init", "frame", "frame_ai"] {
        let entries = |p: &Program| match kind {
            "init" => p.init.clone(),
            "frame" => p.frame.clone(),
            _ => p.frame_ai.clone(),
        };
        let dup = program_of(&format!(
            "{{{kind}}}\n1 (S.L.a)\n{{end}}\n{{{kind}}}\n1 (S.L.a)\n{{end}}\n"
        ));
        assert_eq!(entries(&dup).len(), 1, "{kind}");
        let distinct = program_of(&format!(
            "{{{kind}}}\n1 (S.L.a)\n{{end}}\n{{{kind}}}\n2 (S.L.b)\n{{end}}\n"
        ));
        let first = entries(&distinct);
        assert_eq!(first.len(), 1, "{kind}");
        assert_eq!(distinct.blocks[first[0] as usize].line, 1, "{kind}");
        // (one with its macro calls, as the SKM6126's: the later ones never run)
        let calls = program_of(&format!(
            "{{{kind}}}\n(M.L.m)\n{{end}}\n{{{kind}}}\n(M.L.m)\n{{end}}\n{{macro:m}}\n1 (S.L.a)\n{{end}}\n"
        ));
        assert_eq!(entries(&calls).len(), 1, "{kind}");
    }
    let stacked = program_of("{frame}\n{frame}\n3 (S.L.c)\n{end}\n");
    assert_eq!(stacked.frame.len(), 1);
    // a frame block without its end runs on into the next one
    let cut_short = program_of("{frame}\n1 (S.L.a)\n{frame}\n2 (S.L.b)\n{end}\n");
    assert_eq!(cut_short.frame.len(), 1);
    let first = &cut_short.blocks[cut_short.frame[0] as usize].ops;
    assert_eq!(first.last(), Some(&Op::FallThrough(cut_short.frame[0] + 1)));
}

#[test]
#[rustfmt::skip]
fn script_vars_track_varlists_distinct_from_builtins() {
    let dir = std::env::temp_dir().join(format!("omsi-script-test-vars-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let vl = dir.join("varlist.txt");
    std::fs::write(&vl, "door_0\nPAX_Entry0_Open\n").unwrap();
    let p = compile(&CompileInput {
        builtin_vars: vec!["PAX_Exit0_Open".into(), "Velocity".into()],
        varlists: vec![vl],
        ..Default::default()
    });
    std::fs::remove_dir_all(&dir).ok();
    assert!(p.has_script_var("door_0"));
    assert!(p.has_script_var("PAX_Entry0_Open"));
    assert!(!p.has_script_var("PAX_Exit0_Open"));
    assert!(!p.has_script_var("Velocity"));
    assert!(p.var("PAX_Exit0_Open").is_some());
}

/// `stores`/`reads` answer from the bits made once as the whole bytecode scan did, and
/// a block added afterwards (programs put together by hand) is still seen.
#[test]
fn stores_and_reads_answer_as_the_bytecode_scan() {
    let mut p = program_of("{frame}\n(L.L.a) (S.L.b)\n(L.L.c) 1 + (S.L.c)\n{end}\n");
    let scan = |p: &Program, store: bool, var: VarId| {
        p.blocks.iter().flat_map(|b| &b.ops).any(|op| matches!(op, Op::Store(v) | Op::Load(v) if *v == var && matches!(op, Op::Store(_)) == store))
    };
    let names = ["a", "b", "c", "Clutch"];
    for n in names {
        let v = p.var(n).unwrap();
        assert_eq!(p.stores(v), scan(&p, true, v), "stores {n}");
        assert_eq!(p.reads(v), scan(&p, false, v), "reads {n}");
    }
    assert!(
        p.stores(p.var("b").unwrap())
            && !p.stores(p.var("a").unwrap())
            && p.reads(p.var("c").unwrap())
    );
    // (a variable past every one the bits hold)
    assert!(!p.stores(4000) && !p.reads(4000));
    let clutch = p.var("Clutch").unwrap();
    p.blocks.push(Block {
        ops: vec![Op::Push(1.0), Op::Store(clutch)],
        ..Default::default()
    });
    assert!(p.stores(clutch));
}

/// `reads_sys` answers from the same bits as the scan, and still sees a block added later.
#[test]
fn reads_sys_answers_as_the_bytecode_scan() {
    let mut p = program_of("{frame}\n(L.S.Timegap) (L.S.AutoClutch) + (S.L.a)\n{end}\n");
    let scan = |p: &Program, sys: SysVar| {
        p.blocks
            .iter()
            .flat_map(|b| &b.ops)
            .any(|op| matches!(op, Op::LoadSys(v) if *v == sys))
    };
    for sys in SysVar::ALL {
        assert_eq!(p.reads_sys(sys), scan(&p, sys), "{sys:?}");
    }
    assert!(p.reads_sys(SysVar::AutoClutch) && !p.reads_sys(SysVar::SunAlt));
    p.blocks.push(Block {
        ops: vec![Op::LoadSys(SysVar::SunAlt)],
        ..Default::default()
    });
    assert!(p.reads_sys(SysVar::SunAlt));
}

/// Omsi.exe sub_5d1db8 finds a `(M.L.name)` only among the macros defined below the call:
/// one defined above it, or the macro itself, is `macroinvalid` and the call does nothing.
#[test]
fn a_macro_call_finds_only_a_macro_defined_below_it() {
    let p = program_of(
        "{macro:above}\n1 (S.L.a)\n{end}\n{frame}\n(M.L.above) (M.L.below)\n{end}\n{macro:below}\n(M.L.below) 2 (S.L.b)\n{end}\n",
    );
    let frame = &p.blocks[p.frame[0] as usize].ops;
    let below = p.macro_block("below").unwrap();
    assert!(!matches!(frame[0], Op::Macro(_)), "{frame:?}");
    assert_eq!(frame[1], Op::Macro(below));
    let body = &p.blocks[below as usize].ops;
    assert!(!matches!(body[0], Op::Macro(_)), "{body:?}");
    let msgs: Vec<&str> = p.errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(msgs.len(), 2, "{msgs:?}");
    assert!(
        msgs[0].contains("macroinvalid: macro \"above\""),
        "{msgs:?}"
    );
    assert!(
        msgs[1].contains("macroinvalid: macro \"below\""),
        "{msgs:?}"
    );
}
