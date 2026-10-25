//! Modern JavaScript and the browser objects a page uses.

use super::*;

fn page(body: &str) -> EngineRenderer {
    EngineRenderer::new(40, 40, &format!("<body style='margin:0'><p id=o></p><script>{body}</script></body>"))
}

fn out(r: &EngineRenderer) -> String {
    r.text_of("o").unwrap_or_default()
}

#[test]
fn classes_templates_destructuring_and_spread() {
    let r = page("class A { #n = 2; static k = 5; constructor(x) { this.x = x; } get d() { return this.x * this.#n; } static make(x) { return new A(x); } }\
        class B extends A { get d() { return super.d + 1; } }\
        const { x, ...rest } = { x: 1, y: 2, z: 3 }; const [a, , b = 9, ...tail] = [1, 2, undefined, 4, 5];\
        const f = (p, q = 10, ...more) => p + q + more.length;\
        document.getElementById('o').textContent = `${new B(4).d}|${A.k}|${x}${Object.keys(rest)}|${a}${b}${tail}|${f(1)}${f(1, 2, 3, 4)}|${[...'ab', ...[1]].join('')}|${{...{p: 1}, q: 2}.q}`;");
    assert_eq!(out(&r), "9|5|1y,z|194,5|11 5|ab1|2".replace(' ', ""));
}

#[test]
fn collections_json_and_iteration() {
    let r = page("const m = new Map([['a', 1], ['b', 2]]); const s = new Set([1, 2, 2, 3]); let t = '';\
        for (const [k, v] of m) t += k + v; for (const v of s) t += v; for (const i in {p: 1, q: 2}) t += i;\
        const j = JSON.parse(JSON.stringify({a: [1, {b: null}], c: 'x'}));\
        function* g() { yield 1; yield 2; } t += [...g()].join('');\
        document.getElementById('o').textContent = t + '|' + j.a[1].b + j.c + '|' + [3, 1, 2].sort().reverse().join('') + [1, [2, [3]]].flat(2).length + Object.entries({k: 7})[0];");
    assert_eq!(out(&r), "a1b2123pq12|nullx|3213k,7");
}

#[test]
fn optional_chaining_nullish_regex_and_dates() {
    let r = page("const o = { a: { b: 5 } }; const re = /(\\d+)-(\\d+)/; const m = '10-20'.match(re);\
        document.getElementById('o').textContent = [o?.a?.b, o?.x?.y, null ?? 'd', 0 ?? 'd', m[1] + m[2], 'a1b2'.replace(/\\d/g, '#'), new Date(2020, 0, 15).getMonth(), (1234.5678).toFixed(1), parseInt('42px'), Number.isInteger(3), 'abc'.at(-1), [1,2,3].findLast(x => x < 3)].join(',');");
    assert_eq!(out(&r), "5,,d,0,1020,a#b#,0,1234.6,42,true,c,2");
}

#[test]
fn promises_and_async_functions_run_after_the_script() {
    let r = page("let log = []; const wait = (v) => new Promise((res) => res(v));\
        (async () => { log.push('a'); const v = await wait(2); log.push('b' + v); const w = await Promise.all([wait(3), 4]); log.push('c' + w.join('')); document.getElementById('o').textContent = log.join(''); })();\
        log.push('sync');");
    assert_eq!(out(&r), "asyncb2c34", "a, then the sync line, then what the awaits give");
}

#[test]
fn timers_use_the_page_clock() {
    let r = page("var n = 0; var id = setInterval(function () { n++; document.getElementById('o').textContent = 'n' + n; if (n == 2) clearInterval(id); }, 100); setTimeout(() => omsi.setVar('late', 1), 500);");
    r.set_now(0.05);
    assert!(!r.run_timers());
    r.set_now(0.11);
    assert!(r.run_timers());
    r.set_now(0.25);
    assert!(r.run_timers());
    assert_eq!(out(&r), "n2");
    r.set_now(0.6);
    r.run_timers();
    let mut r = r;
    assert_eq!(r.take_events(), vec![("late".to_string(), 1.0)]);
}

#[test]
fn a_loop_that_never_ends_is_stopped_and_the_page_goes_on() {
    let mut r = EngineRenderer::new(8, 8, "<body style='background:#00ff00'><script>while (true) {}</script><script>document.body.style.background = '#0000ff';</script></body>");
    let f = r.poll_frame().unwrap();
    assert_eq!(&f[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 4], &[0, 0, 255, 255], "the next script still ran");
}

#[test]
fn runaway_recursion_is_an_error_not_a_crash() {
    // (the limit ends the script; a catch cannot swallow it)
    let r = EngineRenderer::new(8, 8, "<body><p id=o></p><script>function f() { return f() + 1; } f();</script><script>document.getElementById('o').textContent = 'after';</script></body>");
    assert_eq!(out(&r), "after");
}

#[test]
fn a_script_error_does_not_stop_the_next_script() {
    let r = EngineRenderer::new(8, 8, "<body><p id=o></p><script>null.x();</script><script>document.getElementById('o').textContent = 'ok';</script></body>");
    assert_eq!(out(&r), "ok");
}

// ─────────────── the DOM ───────────────

#[test]
fn elements_attributes_classes_and_data() {
    let r = page("const d = document.createElement('div'); d.id = 'n'; d.className = 'a b'; d.classList.add('c'); d.classList.toggle('a');\
        d.setAttribute('data-x-y', '7'); d.dataset.k = 'v'; d.title = 'T'; d.hidden = true;\
        document.body.appendChild(d);\
        document.getElementById('o').textContent = [d.className, d.dataset.xY, d.getAttribute('data-k'), d.title, d.hasAttribute('hidden'), document.getElementById('n') === d, d.tagName, d.matches('div.b[data-k=v]'), d.closest('body') === document.body].join('|');");
    assert_eq!(out(&r), "b c|7|v|T|true|true|DIV|true|true");
}

#[test]
fn the_tree_can_be_walked_and_rebuilt() {
    let r = page("const ul = document.createElement('ul'); document.body.append(ul);\
        for (const t of ['a', 'b', 'c']) { const li = document.createElement('li'); li.textContent = t; ul.appendChild(li); }\
        ul.insertBefore(document.createElement('li'), ul.children[1]); ul.children[1].textContent = 'x';\
        const c = ul.cloneNode(true); ul.removeChild(ul.firstElementChild);\
        document.getElementById('o').textContent = [ul.textContent, c.textContent, ul.children.length, ul.lastElementChild.previousElementSibling.textContent, ul.querySelectorAll('li').length, c.firstChild.nextSibling.textContent].join('|');");
    assert_eq!(out(&r), "xbc|axbc|3|b|3|x");
}

#[test]
fn inner_html_reads_and_writes_markup() {
    let r = page("const d = document.createElement('div'); d.innerHTML = '<p class=k>one <b>two</b></p><i>3</i>'; document.body.appendChild(d);\
        document.getElementById('o').textContent = [d.children.length, d.firstChild.className, d.querySelector('b').textContent, d.innerHTML].join('|');");
    assert_eq!(out(&r), "2|k|two|<p class=\"k\">one <b>two</b></p><i>3</i>");
}

#[test]
fn style_reads_and_writes_through_the_proxy() {
    let mut r = EngineRenderer::new(10, 10, "<body style='margin:0'><div id=a style='height:4px;background:#f00'></div><script>const a = document.getElementById('a'); a.style.width = '6px'; a.style.backgroundColor = 'rgb(0, 0, 255)'; window.s = a.style.height + a.style.getPropertyValue('width');</script></body>");
    let f = r.poll_frame().unwrap();
    assert_eq!(&f[(1 * 10 + 3) * 4..(1 * 10 + 3) * 4 + 4], &[0, 0, 255, 255]);
    assert_eq!(&f[(1 * 10 + 7) * 4..(1 * 10 + 7) * 4 + 4], &[0, 0, 0, 0], "6px wide");
    r.run_js("document.body.dataset.s = window.s").unwrap();
}

#[test]
fn geometry_and_computed_style_come_from_the_layout() {
    let r = EngineRenderer::new(100, 100, "<body style='margin:0'><div id=a style='margin:5px;width:30px;height:12px;padding:2px;border:1px solid #000;opacity:0.5'></div><p id=o></p><script>const a = document.getElementById('a'); const r = a.getBoundingClientRect(); const c = getComputedStyle(a);\
        document.getElementById('o').textContent = [r.left, r.top, r.width, r.height, a.offsetWidth, c.width, c.opacity, c.display, c.getPropertyValue('padding-left')].join('|');</script></body>");
    assert_eq!(r.text_of("o").as_deref(), Some("5|5|36|18|36|30px|0.5|block|2px"));
}

// ─────────────── events ───────────────

fn click(r: &mut EngineRenderer, x: f32, y: f32) {
    r.pointer(x, y, PointerKind::Down);
    r.pointer(x, y, PointerKind::Up);
}

#[test]
fn clicks_bubble_and_can_be_stopped() {
    let mut r = EngineRenderer::new(40, 40, "<style>body{margin:0}#a{width:40px;height:40px}#b{width:20px;height:20px}</style><body><div id=a><div id=b></div></div><script>\
        const log = (s) => omsi.setVar(s, 1);\
        document.getElementById('a').addEventListener('click', () => log('a'));\
        document.getElementById('b').addEventListener('click', (e) => { log('b'); if (window.stop) e.stopPropagation(); });\
        document.addEventListener('click', () => log('doc'));</script></body>");
    click(&mut r, 5.0, 5.0);
    let names: Vec<String> = r.take_events().into_iter().map(|e| e.0).collect();
    assert_eq!(names, ["b", "a", "doc"], "inner first, then up through the document");
    r.run_js("window.stop = true").unwrap();
    click(&mut r, 5.0, 5.0);
    let names: Vec<String> = r.take_events().into_iter().map(|e| e.0).collect();
    assert_eq!(names, ["b"]);
    click(&mut r, 30.0, 30.0);
    let names: Vec<String> = r.take_events().into_iter().map(|e| e.0).collect();
    assert_eq!(names, ["a", "doc"]);
}

#[test]
fn event_objects_and_custom_events() {
    let mut r = EngineRenderer::new(20, 20, "<style>body{margin:0}#a{width:20px;height:20px}</style><body><div id=a></div><script>\
        const a = document.getElementById('a');\
        a.onclick = (e) => omsi.setVar('x' + e.type, e.clientX);\
        a.addEventListener('ping', (e) => omsi.setVar('ping', e.detail.n), { once: true });\
        a.dispatchEvent(new CustomEvent('ping', { detail: { n: 7 } })); a.dispatchEvent(new CustomEvent('ping', { detail: { n: 8 } }));</script></body>");
    assert_eq!(r.take_events(), vec![("ping".to_string(), 7.0)], "once: the second dispatch is not heard");
    click(&mut r, 9.0, 3.0);
    assert_eq!(r.take_events(), vec![("xclick".to_string(), 9.0)]);
}

#[test]
fn inline_attribute_handlers_see_the_event_and_this() {
    let mut r = EngineRenderer::new(20, 20, "<style>body{margin:0}#a{width:20px;height:20px}</style><body><div id=a onclick=\"omsi.setVar(this.id + event.type, 1)\"></div></body>");
    click(&mut r, 3.0, 3.0);
    assert_eq!(r.take_events(), vec![("aclick".to_string(), 1.0)]);
}

// ─────────────── the host ───────────────

#[test]
fn fetch_reads_files_of_the_page_only() {
    let dir = std::env::temp_dir().join(format!("omsi-html-fetch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("data.json"), "{\"v\": 41}").unwrap();
    let mut r = EngineRenderer::new(8, 8, "<body><p id=o></p><script>fetch('data.json').then(r => r.json()).then(d => { document.getElementById('o').textContent = d.v + 1; });\
        fetch('../secret.txt').then(r => { omsi.setVar('outside', r.ok ? 1 : 0); }); </script></body>");
    // (the page's scripts wait for its folders)
    r.set_asset_dirs(vec![dir.clone()]);
    assert_eq!(out(&r), "42");
    assert_eq!(r.take_events().last(), Some(&("outside".to_string(), 0.0)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_omsi_object_survives_the_new_engine() {
    let mut r = EngineRenderer::new(8, 8, "<body><script>omsi.update = (d) => { omsi.setVar('seen', d.num.Speed + d.vars.num.speed + omsi.getVar('SPEED')); omsi.trigger('go'); };</script></body>");
    r.set_vars(&[("Speed".into(), 2.0)], &[]);
    assert_eq!(r.take_events(), vec![("seen".to_string(), 6.0)]);
    assert_eq!(r.take_triggers(), vec!["go".to_string()]);
}

#[test]
fn modules_import_from_the_page_folders() {
    let dir = std::env::temp_dir().join(format!("omsi-html-modules-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/math.js"), "import { k } from './k.js'; export const add = (a, b) => a + b + k;").unwrap();
    std::fs::write(dir.join("lib/k.js"), "export const k = 100;").unwrap();
    let mut r = EngineRenderer::new(8, 8, "<body><p id=o></p><script type=\"module\">import { add } from './lib/math.js'; document.getElementById('o').textContent = add(1, 2);</script>\
        <script type=\"module\">import '../outside.js'; document.getElementById('o').textContent = 'escaped';</script></body>");
    r.set_asset_dirs(vec![dir.clone()]);
    assert_eq!(out(&r), "103", "a module imports another, which imports a third, relative to itself; one that climbs out fails");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scripts_of_other_types_do_not_run() {
    let r = EngineRenderer::new(8, 8, "<body><p id=o>x</p><script type=\"application/json\">{\"a\": 1}</script><script type=\"text/template\">document.getElementById('o').textContent = 'ran';</script></body>");
    assert_eq!(out(&r), "x");
}

#[test]
fn the_example_site_loads_and_draws() {
    use omsi_sim::vehicle_api::ApiValue;
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/htmltexture/site");
    // (the way the game reads a page: the stylesheet and the module are put into it)
    let html = omsi_sim::htmltex::load_page(&[dir.as_path()], "index.html");
    assert!(html.contains("data-omsi-src=\"app.js\"") && html.contains("--accent"), "the files were inlined");
    let mut r = EngineRenderer::new(1024, 512, &html);
    r.set_asset_dirs(vec![dir.clone()]);
    let map = |v: Vec<(&str, ApiValue)>| ApiValue::Map(v.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
    r.set_vehicle(&map(vec![
        ("speed", map(vec![("kmh", ApiValue::Num(41.6))])),
        ("route", map(vec![("line", ApiValue::Str("92".into())), ("nextStop", ApiValue::Str("Zitadelle".into()))])),
        ("doors", map(vec![("list", ApiValue::List(vec![map(vec![("isOpen", ApiValue::Bool(true))]), map(vec![("isOpen", ApiValue::Bool(false))])]))])),
    ]));
    r.set_env(&map(vec![("time", map(vec![("hour", ApiValue::Num(7.0)), ("minute", ApiValue::Num(5.0))]))]));
    r.set_vars(&[("velocity".to_string(), 0.0)], &[]);
    assert_eq!(r.text_of("speed").as_deref(), Some("42"));
    assert_eq!(r.text_of("clock").as_deref(), Some("07:05"));
    assert_eq!(r.text_of("line").as_deref(), Some("92"));
    assert_eq!(r.text_of("stop").as_deref(), Some("Zitadelle"));
    let f = r.poll_frame().unwrap();
    // the top bar's green rule (2px accent line under a 52 px bar) and a door that is open (red)
    let any = |pred: &dyn Fn([u8; 4]) -> bool| f.chunks_exact(4).any(|p| pred([p[0], p[1], p[2], p[3]]));
    assert!(any(&|p| p == [0x3d, 0xdc, 0x84, 255]), "the accent colour from the custom property");
    assert!(any(&|p| p == [0xff, 0x52, 0x52, 255]), "the open door is red");
}
