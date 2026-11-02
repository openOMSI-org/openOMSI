// The page's state: one object, changed through `set`, which redraws whoever uses it.

import { useEffect, useState } from '../vendor/preact-htm.js';

/** The page the launcher opens on: the one asked for with `--page <id>` (the program hands it
 * over as `window.__openomsiPage`, or `?page=<id>` in a browser), else Drive. An id that is
 * no page shows Drive, as `App` does. */
function startPage() {
  const asked = window.__openomsiPage || new URLSearchParams(location.search).get('page');
  return typeof asked === 'string' && /^[a-z0-9_-]{1,30}$/.test(asked) ? asked : 'drive';
}

export const S = {
  ready: false,
  page: startPage(),
  boot: null,
  settings: null,
  config: null,
  maps: null,
  vehicles: null,
  vehiclesProgress: null,
  weather: null,
  lines: { key: '', list: null, loading: false, error: null },
  choice: null,
  favourites: [],
  depot: null,
  depotSel: null,
  depotGroups: [],
  depotGroup: remembered('depot-group', ''),
  hof: '',
  ibis: null,
  situations: [],
  profiles: null,
  profile: null,
  poll: { stamp: '', jobs: [], instances: [] },
  toasts: [],
  errors: {},
  cache: {},
};

const subs = new Set();
let queued = false;

/** Change the state (an object merged in, or a function of it) and redraw on the next frame. */
export function set(patch) {
  Object.assign(S, typeof patch === 'function' ? patch(S) : patch);
  if (!queued) {
    queued = true;
    // (a hidden window gets no animation frames: the timer draws then)
    const draw = () => {
      if (!queued) return;
      queued = false;
      for (const f of subs) f();
    };
    requestAnimationFrame(draw);
    setTimeout(draw, 100);
  }
}

/** Redraw this component when the state changes. */
export function useStore() {
  const [, tick] = useState(0);
  useEffect(() => {
    const f = () => tick((n) => n + 1);
    subs.add(f);
    return () => subs.delete(f);
  }, []);
  return S;
}

let toastId = 0;
/** A short message in the corner (`kind`: ok, error, info). */
export function toast(text, kind = 'info', ms = 5000) {
  const id = ++toastId;
  set({ toasts: [...S.toasts, { id, text, kind }] });
  setTimeout(() => set({ toasts: S.toasts.filter((x) => x.id !== id) }), kind === 'error' ? Math.max(ms, 8000) : ms);
}

export function remembered(key, fallback) {
  try {
    return localStorage.getItem(key) || fallback;
  } catch {
    return fallback;
  }
}

export function remember(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    return;
  }
}
