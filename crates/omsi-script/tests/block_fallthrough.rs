//! A block without its `{end}` runs on into the next block (synthetic scripts, no OMSI assets).
//!
//! Omsi.exe (sub_5d1db8) links the words of all script files into one chain; a header only
//! marks where a block starts in it and only `{end}` cuts it.

use omsi_script::{CompileInput, NullHost, Program, State, Vm, compile};

fn program(files: &[&str]) -> Program {
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
    let p = compile(&CompileInput {
        builtin_vars: ["a", "b", "c", "f"].map(String::from).to_vec(),
        scripts,
        ..Default::default()
    });
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    p
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
