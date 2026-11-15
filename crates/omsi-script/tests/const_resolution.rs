//! Synthetic constfile resolution tests; no original OMSI assets required.

use omsi_script::{compile, CompileInput, NullHost, State, Vm};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "omsi_const_resolution_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn definition(value: u32, name: &str) -> String {
    // A sloped curve also checks that later duplicate points do not join the first curve.
    format!(
        "[const]\n{name}\n{value}\n[newcurve]\n{name}\n[pnt]\n0\n{value}\n[pnt]\n1\n{}\n",
        value + 2,
    )
}

fn check_resolution(across_files: bool) {
    for (first, later) in [(1, 4), (4, 1)] {
        for second_name in ["Delay", "dELAY"] {
            let f = Fixture::new();
            let a = definition(first, "Delay");
            let b = definition(later, second_name);
            let constfiles = if across_files {
                vec![f.write("first.txt", &a), f.write("later.txt", &b)]
            } else {
                vec![f.write("both.txt", &(a + &b))]
            };
            let script = f.write(
                "probe.osc",
                "{frame}\n(C.L.delay) (S.L.constant)\n0.5 (F.L.delay) (S.L.curve)\n{end}\n",
            );
            let p = compile(&CompileInput {
                builtin_vars: vec!["constant".into(), "curve".into()],
                constfiles,
                scripts: vec![script],
                ..Default::default()
            });
            assert!(p.errors.is_empty(), "{:?}", p.errors);
            let mut state = State::new(&p);
            Vm::new().run_frame(&p, &mut state, &mut NullHost);
            assert_eq!(state.get(p.var("constant").unwrap()), first as f32);
            assert_eq!(state.get(p.var("curve").unwrap()), first as f32 + 1.0);
        }
    }
}

#[test]
fn first_constant_and_curve_win_within_a_file() {
    check_resolution(false);
}

#[test]
fn first_constant_and_curve_win_across_files() {
    check_resolution(true);
}
