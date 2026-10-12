import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { toast } from '../store.js';
import { call } from '../api.js';
import { Button, Input } from '../ui.js';

let saving = Promise.resolve();

export function RadioStations() {
  const [list, setList] = useState(null);
  const pending = useRef(null);
  const timer = useRef(0);
  const flush = () => {
    clearTimeout(timer.current);
    const l = pending.current;
    pending.current = null;
    if (!l) return;
    saving = saving.then(() => call('radio_stations_save', { list: l }).catch((e) => toast(e.message, 'error')));
  };
  useEffect(() => {
    let gone = false;
    saving
      .then(() => call('radio_stations'))
      .then((l) => !gone && setList(Array.isArray(l) ? l : []))
      .catch((e) => {
        if (!gone) setList([]);
        toast(e.message, 'error');
      });
    window.addEventListener('pagehide', flush);
    return () => {
      gone = true;
      window.removeEventListener('pagehide', flush);
      flush();
    };
  }, []);
  const change = (l, now) => {
    setList(l);
    pending.current = l;
    clearTimeout(timer.current);
    if (now) flush();
    else timer.current = setTimeout(flush, 400);
  };
  const edit = (i, field, v) => change(list.map((x, k) => (k === i ? { ...x, [field]: v } : x)));
  if (!list) return null;
  let n = 0;
  return html`<div class="radio-list">
    ${list.map((x, i) => {
      const number = x.address.split('|')[0].trim() ? ++n : '';
      return html`<div class="radio-row" key=${i}>
        <span class="radio-number muted small">${number}</span>
        <${Input} value=${x.name} placeholder="Name" onInput=${(v) => edit(i, 'name', v)} onBlur=${flush} />
        <${Input} value=${x.address} placeholder="Address" onInput=${(v) => edit(i, 'address', v)} onBlur=${flush} />
        <${Button} small kind="ghost" icon="trash" title="Remove this station" onClick=${() => change(list.filter((_, k) => k !== i), true)} />
      </div>`;
    })}
    <div><${Button} small icon="plus" label="Add a station" onClick=${() => setList([...list, { name: '', address: '' }])} /></div>
  </div>`;
}
