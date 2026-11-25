//! Synthetic ibox digit-entry regression tests (original openOMSI fixture — not an add-on extract).
//!
//! Models the stock float/`$IntToStr` behaviours that Aachen-style units share so
//! `compat::aachen_ibox_route_pad` can rewrite them: digit 0 is mouseevent `ibox_taste_D11`
//! (not `.D0`); modes 2 / 7 / 10 need a string buffer for leading zeros; `clickspots_reset`
//! must not wipe that buffer every frame; entry caps at 4 characters (stock `> 9999` → ÷10).
//!
//! No add-on script text and no path into a local OMSI install.

use omsi_script::{CompileInput, NullHost, Program, State, Vm, compile};
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ibox_digit_entry")
}

/// Minimal original script (its `{frame}` first: a macro is called only from above its
/// definition) that triggers the Aachen ibox compat recognition pattern
/// (`GetRouteIndex` packing + stock `$IntToStr` / PIN mask + `clickspots_reset`).
fn synthetic_ibox_script() -> String {
    r#"{frame}
	(M.L.clickspots_reset)
	(L.L.ibox_modus) 2 =
	{if}
		(M.L.ibox_tastatur)
		(L.L.ibox_eingabe) $IntToStr $length s0
		"*" l0 $* (S.$.ibox_Zahleingabe)
	{endif}
	(L.L.ibox_modus) 7 =
	{if}
		(M.L.ibox_tastatur)
		(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)
	{endif}
	(L.L.ibox_modus) 10 =
	{if}
		(M.L.ibox_tastatur)
		(L.L.ibox_eingabe) (S.L.ibox_Route)
		(L.L.ibox_eingabe) $IntToStr (S.$.ibox_Zahleingabe)
	{endif}
	(L.L.ibox_eingabe) 0 =
	{if}
		"" (S.$.ibox_Zahleingabe)
	{endif}
{end}
{trigger:ibox_taste_D1}
	1 (S.L.ibox_taste_D1)
{end}
{trigger:ibox_taste_D1_drag}
	0 (S.L.ibox_taste_D1)
{end}
{trigger:ibox_taste_D2}
	1 (S.L.ibox_taste_D2)
{end}
{trigger:ibox_taste_D2_drag}
	0 (S.L.ibox_taste_D2)
{end}
{trigger:ibox_taste_D3}
	1 (S.L.ibox_taste_D3)
{end}
{trigger:ibox_taste_D3_drag}
	0 (S.L.ibox_taste_D3)
{end}
{trigger:ibox_taste_D11}
	1 (S.L.ibox_taste_D11)
{end}
{trigger:ibox_taste_D11_drag}
	0 (S.L.ibox_taste_D11)
{end}
{macro:clickspots_reset}
	"" (S.$.ibox_StatuszeileMitte) (S.$.ibox_StatuszeileRechts) (S.$.ibox_Zahleingabe) (S.$.ibox_delay) (S.$.ibox_hstverlauf)
{end}
{macro:ibox_tastatur}
(L.L.ibox_taste_D1) 1 =
{if}
	(L.L.ibox_eingabe) 10 * trunc (S.L.ibox_eingabe)
	(L.L.ibox_eingabe) 1 + trunc (S.L.ibox_eingabe)
{endif}
(L.L.ibox_taste_D2) 1 =
{if}
	(L.L.ibox_eingabe) 10 * trunc (S.L.ibox_eingabe)
	(L.L.ibox_eingabe) 2 + trunc (S.L.ibox_eingabe)
{endif}
(L.L.ibox_taste_D3) 1 =
{if}
	(L.L.ibox_eingabe) 10 * trunc (S.L.ibox_eingabe)
	(L.L.ibox_eingabe) 3 + trunc (S.L.ibox_eingabe)
{endif}
(L.L.ibox_taste_D11) 1 =
{if}
	(L.L.ibox_eingabe) 10 * trunc (S.L.ibox_eingabe)
{endif}
(L.L.ibox_taste_D10) 1 =
{if}
	(L.L.ibox_eingabe) 10 / trunc (S.L.ibox_eingabe)
	0 (S.L.ibox_taste_D10)
{endif}
(L.L.ibox_eingabe) 9999 >
{if}
	(L.L.ibox_eingabe) 10 / trunc (S.L.ibox_eingabe)
{endif}
{end}
{macro:ibox_route_lookup}
	(L.L.ibox_eingabe) s4 (S.L.ibox_Route)
	(L.L.IBIS_LinieKurs) 100 * l4 + (M.V.GetRouteIndex) (S.L.ibox_routenindex)
{end}
"#
    .to_string()
}

fn compile_fixture() -> Program {
    // One shared write+compile: parallel tests must not race on the same fixture files
    // (CI saw `variable "ibox_taste_D1" not found` when `v.txt` was truncated mid-read).
    static PROGRAM: std::sync::OnceLock<Program> = std::sync::OnceLock::new();
    PROGRAM
        .get_or_init(|| {
            let dir = fixture_dir();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("ibox.osc"), synthetic_ibox_script()).unwrap();
            std::fs::write(
                dir.join("v.txt"),
                "ibox_taste_D1\nibox_taste_D2\nibox_taste_D3\nibox_taste_D4\nibox_taste_D5\n\
ibox_taste_D6\nibox_taste_D7\nibox_taste_D8\nibox_taste_D9\n\
ibox_taste_D10\nibox_taste_D11\nibox_eingabe\nibox_modus\nibox_Route\nIBIS_LinieKurs\nibox_routenindex\nibox_PIN\n",
            )
            .unwrap();
            std::fs::write(
                dir.join("s.txt"),
                "ibox_Zahleingabe\nibox_StatuszeileMitte\nibox_StatuszeileRechts\nibox_delay\nibox_hstverlauf\n",
            )
            .unwrap();

            let p = compile(&CompileInput {
                varlists: vec![dir.join("v.txt")],
                stringvarlists: vec![dir.join("s.txt")],
                scripts: vec![dir.join("ibox.osc")],
                ..Default::default()
            });
            assert!(p.errors.is_empty(), "{:?}", p.errors);
            p
        })
        .clone()
}

fn press(vm: &mut Vm, p: &Program, st: &mut State, host: &mut NullHost, trig: &str, drag: &str) {
    assert!(vm.run_trigger(p, trig, st, host));
    vm.run_frame(p, st, host);
    assert!(vm.run_trigger(p, drag, st, host));
}

#[test]
fn d11_digit_zero_appends_like_other_digits() {
    let p = compile_fixture();
    let d11 = p.var("ibox_taste_D11").unwrap();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 7.0);
    st.set(e, 1.0);
    st.str_vars[z as usize] = "1".into();
    assert!(vm.run_trigger(&p, "ibox_taste_D11", &mut st, &mut host));
    assert_eq!(st.get(d11), 1.0, "press must set ibox_taste_D11");
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.get(e), 10.0, "digit 0 must turn 1 into 10");
    assert_eq!(
        st.str_vars[z as usize], "10",
        "display must show the middle zero"
    );

    assert!(vm.run_trigger(&p, "ibox_taste_D11_drag", &mut st, &mut host));
    assert_eq!(st.get(d11), 0.0);
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.get(e), 10.0, "cleared flag must not multiply again");
    assert_eq!(st.str_vars[z as usize], "10");
}

#[test]
fn d11_after_d1_shows_ten_like_click_path() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;
    st.set(modus, 7.0);
    st.set(e, 0.0);

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D1",
        "ibox_taste_D1_drag",
    );
    assert_eq!(st.get(e), 1.0);
    assert_eq!(st.str_vars[z as usize], "1");

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    assert_eq!(st.get(e), 10.0);
    assert_eq!(st.str_vars[z as usize], "10");
}

#[test]
fn mode10_leading_zero_then_one_shows_route_01() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let route = p.var("ibox_Route").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 10.0);
    st.set(e, 0.0);

    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "", "no prefilled 00");

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    assert_eq!(st.get(e), 0.0);
    assert_eq!(st.str_vars[z as usize], "0");
    vm.run_frame(&p, &mut st, &mut host);
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(
        st.str_vars[z as usize], "0",
        "leading zero must survive clickspots_reset (not only the press frame)"
    );

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D1",
        "ibox_taste_D1_drag",
    );
    assert_eq!(st.get(e), 1.0);
    assert_eq!(st.str_vars[z as usize], "01");
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(
        st.str_vars[z as usize], "01",
        "01 must persist across frames"
    );
    assert_eq!(
        st.get(route),
        1.0,
        "submitted route number is 1 (shown as 01)"
    );

    st.set(e, 0.0);
    st.str_vars[z as usize].clear();
    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D1",
        "ibox_taste_D1_drag",
    );
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "1");
    assert_ne!(
        st.str_vars[z as usize], "01",
        "must not auto-pad without typing 0"
    );
}

#[test]
fn mode7_leading_zero_then_one_shows_line_01() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 7.0);
    st.set(e, 0.0);
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "", "line entry starts empty");

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    assert_eq!(st.get(e), 0.0);
    assert_eq!(st.str_vars[z as usize], "0");
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(
        st.str_vars[z as usize], "0",
        "line leading zero must survive clickspots_reset"
    );

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D1",
        "ibox_taste_D1_drag",
    );
    assert_eq!(st.get(e), 1.0);
    assert_eq!(st.str_vars[z as usize], "01");
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "01");
}

#[test]
fn mode2_pin_leading_zero_appends_star() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 2.0);
    st.set(e, 0.0);
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "", "PIN entry starts empty");

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    assert_eq!(st.get(e), 0.0);
    assert_eq!(st.str_vars[z as usize], "*");
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(
        st.str_vars[z as usize], "*",
        "PIN star must survive clickspots_reset"
    );

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D1",
        "ibox_taste_D1_drag",
    );
    assert_eq!(st.get(e), 1.0);
    assert_eq!(st.str_vars[z as usize], "**");
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "**");
}

#[test]
fn mode7_line_entry_starts_empty_after_route_display() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 10.0);
    st.set(e, 0.0);
    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    assert_eq!(st.str_vars[z as usize], "0");

    st.set(modus, 7.0);
    st.set(e, 0.0);
    st.str_vars[z as usize].clear();
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.str_vars[z as usize], "");
}

#[test]
fn mode10_route_02_and_mid_zero_ten() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;
    st.set(modus, 10.0);

    st.set(e, 0.0);
    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    assert_eq!(st.str_vars[z as usize], "0");

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D2",
        "ibox_taste_D2_drag",
    );
    assert_eq!(st.get(e), 2.0);
    assert_eq!(st.str_vars[z as usize], "02");

    st.set(e, 1.0);
    st.str_vars[z as usize] = "1".into();
    assert!(vm.run_trigger(&p, "ibox_taste_D11", &mut st, &mut host));
    vm.run_frame(&p, &mut st, &mut host);
    assert_eq!(st.get(e), 10.0);
    assert_eq!(st.str_vars[z as usize], "10");
}

/// Hof-style trip codes are line then route (e.g. line `033` + route `01`), not one five-digit string.
#[test]
fn mode7_line_033_then_mode10_route_01() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 7.0);
    st.set(e, 0.0);
    vm.run_frame(&p, &mut st, &mut host);
    for (trig, drag) in [
        ("ibox_taste_D11", "ibox_taste_D11_drag"),
        ("ibox_taste_D3", "ibox_taste_D3_drag"),
        ("ibox_taste_D3", "ibox_taste_D3_drag"),
    ] {
        press(&mut vm, &p, &mut st, &mut host, trig, drag);
    }
    assert_eq!(st.get(e), 33.0);
    assert_eq!(st.str_vars[z as usize], "033");

    st.set(modus, 10.0);
    st.set(e, 0.0);
    st.str_vars[z as usize].clear();
    vm.run_frame(&p, &mut st, &mut host);
    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D11",
        "ibox_taste_D11_drag",
    );
    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D1",
        "ibox_taste_D1_drag",
    );
    assert_eq!(st.get(e), 1.0);
    assert_eq!(st.str_vars[z as usize], "01");
}

#[test]
fn stock_4_digit_cap_rejects_fifth_char_and_fifth_significant_digit() {
    let p = compile_fixture();
    let e = p.var("ibox_eingabe").unwrap();
    let modus = p.var("ibox_modus").unwrap();
    let z = p.str_var("ibox_Zahleingabe").unwrap();
    let mut vm = Vm::new();
    let mut st = State::new(&p);
    let mut host = NullHost;

    st.set(modus, 7.0);
    st.set(e, 0.0);
    for (trig, drag) in [
        ("ibox_taste_D3", "ibox_taste_D3_drag"),
        ("ibox_taste_D3", "ibox_taste_D3_drag"),
        ("ibox_taste_D11", "ibox_taste_D11_drag"),
        ("ibox_taste_D1", "ibox_taste_D1_drag"),
    ] {
        press(&mut vm, &p, &mut st, &mut host, trig, drag);
    }
    assert_eq!(st.get(e), 3301.0);
    assert_eq!(st.str_vars[z as usize], "3301");

    press(
        &mut vm,
        &p,
        &mut st,
        &mut host,
        "ibox_taste_D2",
        "ibox_taste_D2_drag",
    );
    assert_eq!(st.get(e), 3301.0, "stock 9999 cap must drop the 5th digit");
    assert_eq!(
        st.str_vars[z as usize], "3301",
        "string buffer must not grow past 4 characters"
    );
}
