// The bus preview: drawn by the game's own renderer (the launcher's helper, `openomsi
// --launcher-service`), exactly as the game's launcher window drew it - the bus with its
// textures and every part, under the chosen time and weather.
//
// One request a picture, and the drag and the wheel ride along with it, so a turn costs one
// round trip. Every picture is full size, turning or not. Nothing is asked while nothing moves.

import { html, useEffect, useRef, useState } from '../vendor/preact-htm.js';
import { call, on } from './api.js';
import { t } from './i18n.js';
import { Icon } from './icons.js';
import { Spinner } from './ui.js';

async function frame(args) {
  const buf = new Uint8Array(await call('preview_frame', args));
  const n = new DataView(buf.buffer, buf.byteOffset).getUint32(0, true);
  const head = JSON.parse(new TextDecoder().decode(buf.subarray(4, 4 + n)));
  const jpeg = buf.length > 4 + n ? new Blob([buf.subarray(4 + n)], { type: 'image/jpeg' }) : null;
  return { head, jpeg };
}

export function BusPreview({ look, class: cls = '' }) {
  const host = useRef();
  const canvas = useRef();
  const [state, setState] = useState('loading');
  // what the loop sends next: the drag and zoom gathered since the last picture
  const r = useRef({ alive: true, force: true, dx: 0, dy: 0, zoom: 1, dragging: false, wake: null });

  const wake = (force) => {
    const st = r.current;
    if (force) st.force = true;
    const w = st.wake;
    st.wake = null;
    w?.();
  };

  useEffect(() => {
    const st = r.current;
    st.alive = true;
    (async () => {
      while (st.alive) {
        const el = host.current;
        // (full size while it turns too: a smaller picture stretched over the view is blurred)
        const k = Math.min(window.devicePixelRatio || 1, 2);
        const w = Math.round((el?.clientWidth || 640) * k);
        const h = Math.round((el?.clientHeight || 360) * k);
        const args = { w, h, force: st.force, dx: st.dx, dy: st.dy, zoom: st.zoom, quality: 90 };
        st.dx = st.dy = 0;
        st.zoom = 1;
        st.force = false;
        let more = false;
        try {
          const { head, jpeg } = await frame(args);
          if (jpeg && canvas.current) {
            const bmp = await createImageBitmap(jpeg);
            const c = canvas.current;
            if (c.width !== bmp.width || c.height !== bmp.height) Object.assign(c, { width: bmp.width, height: bmp.height });
            c.getContext('2d').drawImage(bmp, 0, 0);
            bmp.close();
          }
          setState(head.failed ? 'failed' : head.picture ? (head.busy ? 'switching' : 'ok') : 'loading');
          more = head.busy || head.moving || !head.picture;
        } catch {
          setState('failed');
        }
        if (!st.alive) break;
        if (more || st.dx !== 0 || st.dy !== 0 || st.zoom !== 1) await new Promise((res) => requestAnimationFrame(res));
        else await new Promise((res) => (st.wake = res));
      }
    })();
    const ro = new ResizeObserver(() => wake(true));
    ro.observe(host.current);
    const off = on('games-ended', () => wake(true));
    return () => {
      st.alive = false;
      wake();
      ro.disconnect();
      off();
    };
  }, []);

  // another bus, paint, time or weather
  const key = JSON.stringify(look);
  useEffect(() => {
    if (!look?.bus) return;
    call('preview_look', look).then(() => wake(true));
  }, [key]);

  const last = useRef(null);
  return html`<div class=${'viewer ' + cls} ref=${host}
      onPointerDown=${(e) => {
        last.current = { x: e.clientX, y: e.clientY };
        r.current.dragging = true;
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove=${(e) => {
        const p = last.current;
        if (!p) return;
        r.current.dx += e.clientX - p.x;
        r.current.dy += e.clientY - p.y;
        last.current = { x: e.clientX, y: e.clientY };
        wake();
      }}
      onPointerUp=${() => {
        last.current = null;
        r.current.dragging = false;
        wake();
      }}
      onWheel=${(e) => {
        e.preventDefault();
        r.current.zoom *= e.deltaY > 0 ? 1.08 : 1 / 1.08;
        wake();
      }}>
    <canvas ref=${canvas}></canvas>
    ${state === 'loading' && html`<div class="state"><${Spinner} /> ${t('Preparing the preview…')}</div>`}
    ${state === 'switching' && html`<div class="state corner"><${Spinner} /></div>`}
    ${state === 'failed' && html`<div class="state"><${Icon} name="bus" size=${28} /> ${t('No preview of this bus')}</div>`}
    ${state === 'ok' && html`<div class="hint">${t('Drag to turn, scroll to zoom')}</div>`}
  </div>`;
}
