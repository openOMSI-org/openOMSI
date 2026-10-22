// The launcher's commands (src-tauri/src/commands.rs) and events. Outside the Tauri window
// (a browser opened on ui/ for a look at the layout, `launcher/` served) the commands answer
// from launcher/mock/*.json, which `launcher/README.md` says how to make.

const tauri = window.__TAURI__;
export const inTauri = !!tauri;

// The phone app: the same page in the game's web view (`OmsiActivity`), its commands answered
// by the game program itself (`omsi-app`'s `android_launcher`). `OmsiLauncher.call(id, cmd,
// args)` runs one on a worker and answers through `window.__omsiReply`; the pictures come as
// the answers of requests the web view hands to the game (`http://omsi.localhost/bytes/...`).
const android = window.OmsiLauncher
  ? (() => {
      const waiting = new Map();
      let next = 1;
      window.__omsiReply = (id, ok, text) => {
        const w = waiting.get(id);
        waiting.delete(id);
        if (!w) return;
        if (ok) w.resolve(text ? JSON.parse(text) : null);
        else w.reject(new Error(text));
      };
      return {
        call(cmd, args) {
          if (BYTES.has(cmd)) {
            return fetch('http://omsi.localhost/bytes/' + cmd + '?args=' + encodeURIComponent(JSON.stringify(args))).then(async (r) => {
              if (!r.ok) throw new Error(await r.text());
              return r.arrayBuffer();
            });
          }
          const id = next++;
          return new Promise((resolve, reject) => {
            waiting.set(id, { resolve, reject });
            window.OmsiLauncher.call(id, cmd, JSON.stringify(args || {}));
          });
        },
      };
    })()
  : null;
export const onPhone = !!android;

// (a running game's log in the browser: a made-up line each second since the page opened)
const MOCK_OPENED = Date.now();
const MOCK_LIVE = [
  ['INFO ', 'openomsi_game', 'stop Freudstr.: 3 boarding, 1 leaving'],
  ['INFO ', 'omsi_ai', 'tile 8/3 loaded (36 ms)'],
  ['INFO ', 'openomsi_game', 'next stop: U Rathaus Spandau'],
  ['WARN ', 'omsi_render', 'frame took 41 ms (shaders compiling)'],
  ['INFO ', 'omsi_sound', 'announcement: Rathaus Spandau'],
  ['ERROR', 'omsi_content', 'sound file Sound/kneel.wav could not be read: invalid RIFF header'],
];

async function mockLog(args) {
  const r = await fetch(`../mock/log_file/${String(args.name).replace(/\//g, '_')}.json`);
  if (!r.ok) throw new Error(`no log file ${args.name}`);
  const a = await r.json();
  if (!a.file.pid) return a;
  const n = Math.floor((Date.now() - MOCK_OPENED) / 1000);
  for (let i = 0; i < n; i++) {
    const [lv, tgt, msg] = MOCK_LIVE[i % MOCK_LIVE.length];
    a.lines.push(`[${new Date(MOCK_OPENED + i * 1000).toISOString().slice(0, 19)}Z ${lv} ${tgt}] ${msg}`);
  }
  return a;
}

// (log files deleted in the browser: left out of the list until the page is opened again)
const MOCK_DELETED = new Set();

async function mock(cmd, args) {
  if (cmd === 'log_file') return mockLog(args);
  if (cmd === 'log_delete') return (args.names.forEach((n) => MOCK_DELETED.add(n)), args.names.length);
  if (cmd === 'log_files') return (await (await fetch('../mock/log_files.json')).json()).filter((f) => !MOCK_DELETED.has(f.name));
  // (raw answers - the preview's pictures - as .bin)
  if (cmd === 'preview_frame') return (await fetch('../mock/preview_frame.bin')).arrayBuffer();
  if (BYTES.has(cmd)) return (await fetch(`../mock/${cmd}.json`)).arrayBuffer();
  const r = await fetch(`../mock/${cmd}.json`);
  if (!r.ok) throw new Error(`${cmd}: no mock data`);
  return r.json();
}

// (the desktop window's own commands; every other one is the launcher's, `omsi-launcher-api`)
const OWN = new Set(['pick_folder', 'pick_file', 'pick_mod', 'pick_hofs', 'open_path', 'open_url', 'update_relaunch', 'selftest_done', 'fit_zoom']);
// (commands answering bytes rather than JSON)
const BYTES = new Set(['preview_frame', 'route_map', 'map_ground']);

/** Call a command; rejects with the error text the command gave. */
export function call(cmd, args = {}) {
  if (android) return android.call(cmd, args);
  if (!inTauri) return mock(cmd, args);
  const p = OWN.has(cmd) ? tauri.core.invoke(cmd, args) : tauri.core.invoke(BYTES.has(cmd) ? 'call_bytes' : 'call', { cmd, args });
  return p.catch((e) => {
    throw new Error(typeof e === 'string' ? e : e?.message || String(e));
  });
}

/** Listen to an event of the launcher's (returns the function that stops listening). */
export function on(event, fn) {
  if (android) {
    const h = (e) => fn(e.detail);
    window.addEventListener('omsi:' + event, h);
    return () => window.removeEventListener('omsi:' + event, h);
  }
  if (!inTauri) return () => {};
  let stop = null;
  let gone = false;
  tauri.event.listen(event, (e) => fn(e.payload)).then((u) => {
    if (gone) u();
    else stop = u;
  });
  return () => {
    gone = true;
    if (stop) stop();
  };
}

/** Files dropped onto the window (paths), and whether something is being dragged over it. */
export function onDrop(fn, over) {
  if (!inTauri) return () => {};
  const w = tauri.webviewWindow?.getCurrentWebviewWindow?.() || tauri.window?.getCurrentWindow?.();
  if (!w?.onDragDropEvent) return () => {};
  let stop = null;
  w.onDragDropEvent((e) => {
    const p = e.payload;
    if (p.type === 'enter' || p.type === 'over') over?.(true);
    else if (p.type === 'leave') over?.(false);
    else if (p.type === 'drop') {
      over?.(false);
      fn(p.paths || []);
    }
  }).then((u) => (stop = u));
  return () => stop?.();
}

// (custom protocols are `omsi://localhost/` on macOS and Linux, `http://omsi.localhost/` on
// Windows and Android)
const base = /windows|android/i.test(navigator.userAgent) ? 'http://omsi.localhost/' : 'omsi://localhost/';
const enc = (p) => p.split('/').map(encodeURIComponent).join('/');

/** A picture of the content (`maps/Grundorf/picture.jpg`). */
export const contentUrl = (rel) => (inTauri || onPhone ? base + 'content/' + enc(rel.replace(/\\/g, '/')) : '../mock/' + rel.split('/').pop());
