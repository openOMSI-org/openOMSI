//! A block without its `{end}` runs on into the next block (synthetic scripts, no OMSI assets).
//!
//! Omsi.exe (sub_5d1db8) links the words of all script files into one chain; a header only
//! marks where a block starts in it and only `{end}` cuts it.

use omsi_script::{CompileInput, NullHost, Program, State, Vm, compile};

fn program(files: &[&str]) -> Program {
    let p = compile_files(files);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    p
}

fn compile_files(files: &[&str]) -> Program {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsi_fallthrough_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let scripts = files
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let path = dir.join(format!("s{i}.osc"));
            std::fs::write(&path, text).unwrap();
            path
        })
        .collect();
    compile(&CompileInput {
        builtin_vars: ["a", "b", "c", "f"].map(String::from).to_vec(),
        scripts,
        ..Default::default()
    })
}

fn count(p: &Program, st: &State) -> [f32; 4] {
    ["a", "b", "c", "f"].map(|n| st.get(p.var(n).unwrap()))
}

#[test]
fn a_block_without_its_end_runs_on_into_the_next() {
    let p = program(&[
        // `m` lacks its `{end}`, and so does `n` at the end of its file; the `{end}` inside
        // `n`'s `{if}` still ends `n` when its condition holds
        "{frame}\n(L.L.f) 1 + (S.L.f) (M.L.m)\n{end}\n{macro:m}\n(L.L.a) 1 + (S.L.a)\n{macro:n}\n(L.L.c) {if} {end} {endif} (L.L.b) 1 + (S.L.b)\n",
        "{trigger:t}\n(L.L.c) 1 + (S.L.c)\n{end}\n",
    ]);
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_frame(&p, &mut st, &mut NullHost);
    // f: the frame, a: m, b: n after m, c: the trigger after n in the next file
    assert_eq!(count(&p, &st), [1.0, 1.0, 1.0, 1.0]);
    // now c is 1: n's `{end}` in its `{if}` ends the run there
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [2.0, 1.0, 1.0, 2.0]);
}

#[test]
fn a_frame_block_run_on_into_from_the_one_above_runs_once() {
    let p = program(&["{frame}\n(L.L.a) 1 + (S.L.a)\n{frame}\n(L.L.b) 1 + (S.L.b)\n{end}\n"]);
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [1.0, 1.0, 0.0, 0.0]);
}

#[test]
fn the_last_block_without_its_end_just_ends() {
    let p = program(&["{init}\n(L.L.a) 1 + (S.L.a)\n"]);
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_init(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_long_chain_of_blocks_without_their_end_runs_at_the_callers_depth() {
    // 60 macros deep (`d0` … `d59`), then 100 blocks without their `{end}` (`c0` … `c99`):
    // in Omsi.exe they are one word chain, not 100 nested calls
    let mut text = String::from("{frame}\n(M.L.d0)\n{end}\n");
    for i in 0..60 {
        let next = if i < 59 {
            format!("d{}", i + 1)
        } else {
            "c0".into()
        };
        text += &format!("{{macro:d{i}}}\n(M.L.{next})\n{{end}}\n");
    }
    for i in 0..100 {
        text += &format!("{{macro:c{i}}}\n(L.L.a) 1 + (S.L.a)\n");
    }
    text += "{end}\n";
    let p = program(&[&text]);
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [100.0, 0.0, 0.0, 0.0]);
}

#[test]
fn the_words_before_a_files_first_header_run_after_a_block_without_its_end() {
    // the words before `{macro:m}` sit between the `{frame}` and `m` in the word chain
    let next = "(L.L.b) 1 + (S.L.b)\n{macro:m}\n(L.L.c) 1 + (S.L.c)\n{end}\n";
    let p = program(&["{frame}\n(L.L.a) 1 + (S.L.a)\n", next]);
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [1.0, 1.0, 1.0, 0.0]);
    assert!(p.macro_block("m").is_some());
    // after an `{end}` nothing links to them
    let p = program(&["{frame}\n(L.L.a) 1 + (S.L.a)\n{end}\n", next]);
    let mut st = State::new(&p);
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn block_words_are_case_sensitive() {
    // Omsi.exe compares them exactly: `{End}` does not end the frame, which runs on into
    // `{trigger:t}`, and `{Frame}` starts no block
    let p = compile_files(&[
        "{frame}\n(L.L.a) 1 + (S.L.a)\n{End}\n{trigger:t}\n(L.L.b) 1 + (S.L.b)\n{end}\n{Frame}\n(L.L.c) 1 + (S.L.c)\n{end}\n",
    ]);
    assert_eq!(p.errors.len(), 2, "{:?}", p.errors);
    assert_eq!(p.frame.len(), 1);
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [1.0, 1.0, 0.0, 0.0]);
}

/// Omsi.exe sub_5d1db8 keeps the first `{init}` / `{frame}` of the whole set (the words of
/// all files in `[script]` order): a second one in a later file, as the SKM6126's hidden AI
/// script adds, never runs, so its timers run once a frame (issue #1101).
#[test]
fn only_the_sets_first_init_and_frame_run() {
    let p = program(&[
        "{init}\n(L.L.f) 1 + (S.L.f)\n{end}\n{frame}\n(M.L.tick)\n{end}\n",
        "{init}\n(L.L.f) 10 + (S.L.f)\n{end}\n{frame}\n(M.L.tick) (L.L.b) 1 + (S.L.b)\n{end}\n{frame}\n(M.L.tick) (L.L.c) 1 + (S.L.c)\n{end}\n",
        "{macro:tick}\n(L.L.a) 1 + (S.L.a)\n{end}\n",
    ]);
    assert_eq!((p.init.len(), p.frame.len()), (1, 1));
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    vm.run_init(&p, &mut st, &mut NullHost);
    vm.run_frame(&p, &mut st, &mut NullHost);
    assert_eq!(count(&p, &st), [1.0, 0.0, 0.0, 1.0]);
}
