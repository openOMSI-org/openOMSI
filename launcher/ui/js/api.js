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

async function mock(cmd) {
  // (raw answers - the preview's pictures - as .bin)
  if (cmd === 'preview_frame') return (await fetch('../mock/preview_frame.bin')).arrayBuffer();
  if (BYTES.has(cmd)) return (await fetch(`../mock/${cmd}.json`)).arrayBuffer();
  const r = await fetch(`../mock/${cmd}.json`);
  if (!r.ok) throw new Error(`${cmd}: no mock data`);
  return r.json();
}

// (the desktop window's own commands; every other one is the launcher's, `omsi-launcher-api`)
const OWN = new Set(['pick_folder', 'pick_data_folder', 'pick_file', 'pick_mod', 'pick_hofs', 'open_path', 'open_url', 'update_relaunch', 'selftest_done', 'fit_zoom']);
// (commands answering bytes rather than JSON)
const BYTES = new Set(['preview_frame', 'route_map', 'map_ground']);

/** Call a command; rejects with the error text the command gave. */
export function call(cmd, args = {}) {
  if (android) return android.call(cmd, args);
  if (!inTauri) return mock(cmd);
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
