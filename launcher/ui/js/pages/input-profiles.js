// Controls profiles (#3921): named sets of every key and every controller setting. One is in
// use, one is the default, and a bus can have its own: before each drive the launcher switches
// to the bus's profile, or to the default one (`omsi_launcher_lib::input_profiles`).

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { busName } from '../data.js';
import { Button, Card, Select, Tag, Toggle, useConfirm } from '../ui.js';
import { NameDialog } from './depot.js';

const norm = (bus) => (bus || '').trim().replace(/\\/g, '/').toLowerCase();

export function InputProfiles() {
  const s = useStore();
  const [p, setP] = useState(null);
  const [naming, setNaming] = useState(null);
  const [ask, confirm] = useConfirm();
  const read = () => call('input_profiles').then(setP).catch(() => setP({ profiles: [], buses: {} }));
  useEffect(() => {
    read();
  }, []);
  if (!p) return null;
  // (the key and controller pages read their files again)
  const reload = () => set({ cache: { ...S.cache, keys: undefined }, controlsProfileStamp: (S.controlsProfileStamp || 0) + 1 });
  const run = async (cmd, args, done) => {
    try {
      await call(cmd, args);
      if (done) toast(done, 'ok');
      await read();
      if (cmd === 'input_profile_switch' || cmd === 'input_profile_create') reload();
    } catch (e) {
      toast(t(e.message), 'error');
    }
  };
  const active = p.active || '';
  const bus = s.choice?.bus || '';
  const own = p.buses?.[norm(bus)];
  const assigned = Object.entries(p.buses || {});
  // (the profile in use cannot be deleted: the default one, or another, takes its place first)
  const remove = async () => {
    if (!(await ask({ title: 'Delete this profile?', text: [active], yes: 'Delete', danger: true }))) return;
    const next = p.default && p.default !== active ? p.default : p.profiles.find((n) => n !== active);
    try {
      await call('input_profile_switch', { name: next });
      await call('input_profile_delete', { name: active });
      toast(t('Profile {name} in use', { name: next }), 'ok');
    } catch (e) {
      toast(t(e.message), 'error');
    }
    reload();
    await read();
  };
  return html`
    ${naming && html`<${NameDialog} title=${naming.title} label="Profile name" placeholder="" value=${naming.value} onClose=${() => setNaming(null)} onSave=${(name) => {
      setNaming(null);
      if (naming.rename) run('input_profile_rename', { old: active, new: name });
      else run('input_profile_create', { name }, t('Profile {name} in use', { name }));
    }} />`}
    <${Card} title="Profile" hint="A profile keeps every key and every controller setting. Before each drive the launcher switches to the bus's own profile, or to the default one.">
      <div class="row wrap" style="gap:8px;align-items:center">
        ${p.profiles.length > 0 && html`<div style="min-width:200px"><${Select} raw value=${active} options=${p.profiles.map((n) => [n, n])} onChange=${(n) => run('input_profile_switch', { name: n }, t('Profile {name} in use', { name: n }))} /></div>`}
        <${Button} small icon="copy" label="Duplicate" onClick=${() => setNaming({ title: 'Duplicate', value: active ? `${active} 2` : '' })} />
        ${active && html`<${Button} small kind="ghost" icon="edit" title="Rename the profile" onClick=${() => setNaming({ title: 'Rename the profile', value: active, rename: true })} />`}
        ${active && p.profiles.length > 1 && html`<${Button} small kind="ghost" icon="trash" title="Delete" onClick=${remove} />`}
      </div>
      ${active && html`<div class="row wrap" style="gap:16px;margin-top:12px;align-items:center">
        <${Toggle} label="Default for every bus" checked=${p.default === active} onChange=${(on) => run('input_profile_default', { name: on ? active : null })} />
        ${bus && html`<${Toggle} label=${t('Use for {bus}', { bus: busName(bus) })} checked=${own === active} onChange=${(on) => run('input_profile_assign', { bus, name: on ? active : null })} />`}
      </div>`}
      ${assigned.length > 0 && html`<div class="row wrap" style="gap:6px;margin-top:10px">${assigned.map(([b, n]) => html`<${Tag} key=${b}>${busName(b)}: ${n}</${Tag}>`)}</div>`}
    </${Card}>
    ${confirm}
  `;
}
