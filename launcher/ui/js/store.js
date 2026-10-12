// The page's state: one object, changed through `set`, which redraws whoever uses it.

import { useEffect, useState } from '../vendor/preact-htm.js';

export const S = {
  ready: false,
  page: 'drive',
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
