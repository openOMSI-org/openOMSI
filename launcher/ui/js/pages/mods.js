// Mods: installing a bus, a map or a whole folder into the content folder (the original
// OMSI 2 folder is never written to), the installs under way, and the installed mods.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { useStore, set, toast, remembered, remember } from '../store.js';
import { t, bytes, ago } from '../i18n.js';
import { call, onPhone } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Dialog, Empty, PageHead, Search, Select, Spinner, Tag, Toggle, Notice, useConfirm } from '../ui.js';
import { pollNow, loadContent, cached, installMod, jobRunning } from '../data.js';

const KIND = { bus: ['bus', 'Bus'], map: ['map', 'Map'], archive: ['folder', 'Archive'], other: ['puzzle', 'Other'] };

/** What a mod holds, by the game's folder it put things into, in the order it lists them:
 *  ["Vehicles/MAN_SD202", "Fonts/a.oft", "Fonts/b.oft"] → [["Vehicles", ["MAN_SD202"]], ["Fonts", ["a.oft", "b.oft"]]]. */
export function modTree(paths) {
  const out = [];
  for (const p of paths || []) {
    const i = p.indexOf('/');
    const [folder, rest] = i > 0 ? [p.slice(0, i), p.slice(i + 1)] : ['', p];
    const group = out.find(([f]) => f.toLowerCase() === folder.toLowerCase());
    if (group) group[1].push(rest);
    else out.push([folder, [rest]]);
  }
  return out;
}

export function ModsPage() {
  const s = useStore();
  const status = s.cache.mods ?? null;
  const mode = s.modMode || 'auto';
  const [q, setQ] = useState('');
  const [show, setShow] = useState('all');
  const [busy, setBusy] = useState({});
  const [open, setOpen] = useState(null);
  const [ask, confirm] = useConfirm();
  const read = () => cached('mods', 'mods_status');
  useEffect(() => {
    read();
  }, []);
  const [browsing, setBrowsing] = useState(null);
  const pick = async (archive) => {
    // (a phone has no file dialog: the page browses its storage itself, as the phone's
    // launcher did up to 0.2.34 - the buttons did nothing there since, #2513)
    if (onPhone) {
      setBrowsing({ archive });
      return;
    }
    const p = await call('pick_mod', { archive }).catch(() => null);
    if (p) installMod(p);
  };
  const addHofs = async () => {
    try {
      const files = await call('pick_hofs', { title: t('Choose HOF files'), filter: t('OMSI depot files') }).catch(() => null);
      if (!files?.length) return;
      const added = await call('add_hofs', { files });
      toast(added > 0 ? t('HOF files added for every bus.') : t('These HOF files were already there.'), 'ok');
    } catch (e) {
      toast(t(e.message), 'error');
    }
    read();
    loadContent();
  };
  const [declined, setDeclined] = useState(() => remembered('hofDeclined', '').split('\n').filter(Boolean));
  const decline = (map) => {
    const next = [...declined, map];
    remember('hofDeclined', next.join('\n'));
    setDeclined(next);
  };
  const shareHofs = async (map) => {
    try {
      const added = await call('share_map_hofs', { map });
      toast(added > 0 ? t('HOF files added for every bus.') : t('These HOF files were already there.'), 'ok');
    } catch (e) {
      toast(t(e.message), 'error');
    }
    read();
    loadContent();
  };
  const offers = (status?.hof_offers || []).filter((o) => !declined.includes(o.map));
  const jobs = s.poll.jobs || [];
  const all = status?.installed || [];
  const counts = { all: all.length, bus: 0, map: 0, archive: 0, other: 0, off: 0 };
  for (const m of all) {
    counts[m.kind in counts ? m.kind : 'other']++;
    if (!m.enabled) counts.off++;
  }
  const kind = counts[show] ? show : 'all';
  const fits = (m) => (kind === 'all' ? true : kind === 'off' ? !m.enabled : (m.kind in counts ? m.kind : 'other') === kind);
  const mods = all
    .filter(fits)
    .filter((m) => !q || `${m.name} ${m.paths.join(' ')}`.toLowerCase().includes(q.toLowerCase()))
    .sort((a, b) => (b.enabled ? 1 : 0) - (a.enabled ? 1 : 0) || a.name.localeCompare(b.name));
  const busyWhile = async (m, f) => {
    setBusy((b) => ({ ...b, [m.id]: true }));
    await f();
    setBusy((b) => ({ ...b, [m.id]: false }));
  };
  const toggle = (m, on) => busyWhile(m, async () => {
    try {
      await call('mod_set_enabled', { id: m.id, on });
      toast(t(on ? '{name} switched on' : '{name} switched off', { name: m.name }), 'ok');
      read();
      loadContent();
    } catch (e) {
      toast(t(e.message), 'error');
    }
  });
  const remove = async (m) => {
    if (!(await ask({ title: 'Delete this mod?', text: t('{name} and its files are deleted from the content folder. Switch it off instead to keep it.', { name: m.name }), yes: 'Delete', danger: true }))) return;
    await busyWhile(m, async () => {
      try {
        await call('mod_remove', { id: m.id });
        toast(t('{name} deleted', { name: m.name }), 'ok');
        read();
        loadContent();
      } catch (e) {
        toast(t(e.message), 'error');
      }
    });
  };
  const info = s.modInfo;
  return html`
    ${browsing && html`<${StorageBrowser} archive=${browsing.archive} onClose=${() => setBrowsing(null)} onPick=${(p) => {
      setBrowsing(null);
      installMod(p);
    }} />`}
    <${PageHead} title="Mods" text="A bus, a map, scenery, a whole OMSI folder - as a folder or a .zip, .7z or .rar. The original OMSI 2 folder is never written to." />
    <div class="drive" style="grid-template-columns:minmax(0,1fr) 360px">
      <div class="stack">
        ${status?.error && html`<${Notice} kind="danger">${status.error}</${Notice}>`}
        <${Card} title="Installed mods" actions=${html`<div class="row wrap" style="gap:8px;justify-content:flex-end">
            ${status?.content_dir && html`<${Button} small icon="folder" label="Content folder" onClick=${() => call('open_path', { path: status.content_dir }).catch((e) => toast(t(e.message), 'error'))} />`}
            <div style="width:220px;max-width:100%"><${Search} value=${q} onInput=${setQ} placeholder="Search mods..." /></div>
          </div>`}>
          ${all.length > 0 && html`<div class="row wrap" style="margin-bottom:12px;gap:8px">
            <div class="segmented wrap">
              ${[['all', 'All'], ['bus', 'Buses'], ['map', 'Maps'], ['archive', 'Archives'], ['other', 'Other'], ['off', 'Off']]
                .filter(([k]) => k === 'all' || counts[k] > 0)
                .map(([k, l]) => html`<button class=${k === kind ? 'on' : ''} onClick=${() => setShow(k)}>${t(l)} <span class="muted">${counts[k]}</span></button>`)}
            </div>
            <span class="muted small">${t('{n} mods, {on} on', { n: all.length, on: all.length - counts.off })}</span>
          </div>`}
          ${!status
            ? html`<div class="row muted"><${Spinner} /> ${t('Reading the content folder...')}</div>`
            : mods.length === 0
              ? html`<${Empty} icon="puzzle" text=${q || kind !== 'all' ? 'No mod matches.' : 'No mods yet. Choose a folder or an archive on the right, or drop one onto the window.'} />`
              : html`<div class="list" style="margin:0 -8px">
                  ${mods.map((m) => {
                    const tree = modTree(m.paths);
                    // (the game's folders it holds and how many in each, as the game's own launcher said:
                    // "Vehicles 1, Fonts 2"; an archive is the one file)
                    const holds = m.kind === 'archive' ? '' : tree.filter(([f]) => f).map(([f, items]) => `${f} ${items.length}`).join(', ');
                    return html`<div>
                    <div class="list-item" onClick=${() => setOpen(open === m.id ? null : m.id)}>
                      <${Icon} name=${KIND[m.kind]?.[0] || 'puzzle'} />
                      <div class="grow" style="min-width:0">
                        <div class="title ellipsis" style=${m.enabled ? '' : 'opacity:.55'}>${m.name}</div>
                        <div class="sub">${[t(KIND[m.kind]?.[1] || 'Other'), holds, bytes(m.bytes), m.installed ? ago(m.installed) : !m.noted ? t('found in the content folder') : ''].filter(Boolean).join(' - ')}</div>
                      </div>
                      ${busy[m.id] && html`<${Spinner} />`}
                      <div onClick=${(e) => e.stopPropagation()}><${Toggle} checked=${m.enabled} disabled=${!!busy[m.id]} onChange=${(on) => toggle(m, on)} /></div>
                      <${Button} small kind="ghost" icon="trash" disabled=${!!busy[m.id]} title="Delete this mod (asks first; switch it off instead to keep it)" onClick=${(e) => (e.stopPropagation(), remove(m))} />
                    </div>
                    ${open === m.id && html`<div style="padding:4px 12px 12px 44px" class="small selectable stack">${tree.map(([folder, items]) => html`<div class="row" style="align-items:baseline;gap:12px">
                      <b class="muted" style="flex:none;min-width:96px">${folder}</b><span class="dim" style="min-width:0;overflow-wrap:anywhere">${items.join(', ')}</span>
                    </div>`)}</div>`}
                  </div>`;
                  })}
                </div>`}
        </${Card}>
        ${status?.waiting?.length > 0 && html`<${Card} title="Waiting for their bus" hint="Packs installed before the bus they belong to: they go in once it is there." actions=${html`<${Button} small kind="ghost" icon="alert" label="Details in Troubleshooting" onClick=${() => set({ page: 'troubleshoot' })} />`}>
          <div class="row wrap">${status.waiting.map((w) => html`<${Tag}>${w}</${Tag}>`)}</div>
        </${Card}>`}
        ${offers.map((o) => html`<${Notice} key=${o.map}><b>${t('Share the HOF files of {map} with every bus?', { map: o.map })}</b><div class="small selectable">${o.files.join(', ')}</div><div>${t("Every bus can then show this map's lines and destinations. Some maps have one HOF file per company or bus: sharing them can give buses with their own IBIS wrong destinations.")}</div><div class="row wrap" style="gap:8px;margin-top:6px"><${Button} small kind="primary" label="Share with every bus" onClick=${() => shareHofs(o.map)} /><${Button} small label="Not for this map" onClick=${() => decline(o.map)} /></div></${Notice}>`)}
        ${status?.hof_warnings?.length > 0 && html`<${Notice} kind="warn"><b>${t('Map HOF issue')}</b><div>${t('Some map HOF files could not be added. Check launcher.log for details.')}</div><ul class="small selectable" style="margin:4px 0 0;padding-left:18px">${status.hof_warnings.slice(0, 10).map((warning) => html`<li>${warning}</li>`)}</ul></${Notice}>`}
        ${status?.archives?.length > 0 && html`<${Card} title="Archives" hint="Read in place by the game, without unpacking.">
          <table class="table"><tbody>${status.archives.map(([n, b]) => html`<tr><td>${n}</td><td class="num muted" style="text-align:right">${bytes(b)}</td></tr>`)}</tbody></table>
        </${Card}>`}
      </div>
      <div class="side">
        <${Card} title="Install a mod">
          <div class="stack">
            <div class="grid-2" style="gap:8px">
              <${Button} icon="folder" label="Choose a folder" onClick=${() => pick(false)} />
              <${Button} icon="download" label="Choose archive" onClick=${() => pick(true)} />
            </div>
            ${!onPhone && html`<${Button} icon="file" label="Add HOF files to buses" onClick=${addHofs} />`}
            <div class="field"><span class="label">${t('Archive install mode')}</span>
              <${Select} value=${mode} options=${[['auto', 'Auto'], ['extract', 'Unpacked'], ['inplace', 'Used in place']]} onChange=${(v) => set({ modMode: v })} />
            </div>
            ${s.modPath && html`<div class="small stack" style="gap:4px">
              <code class="selectable ellipsis" title=${s.modPath}>${s.modPath}</code>
              ${info?.error ? html`<span style="color:var(--danger)">${info.error}</span>`
                : info?.is_archive && html`<span style=${info.fits ? '' : 'color:var(--warn)'} class=${info.fits ? 'muted' : ''}>${t('{size} archive, {files} files, {unpacked} unpacked', { size: bytes(info.archive_bytes), files: info.files, unpacked: bytes(info.unpacked_bytes) })} - ${info.fits ? t('fits ({free} free)', { free: bytes(info.free_bytes) }) : t('does not fit: needs {needed}, {free} free', { needed: bytes(info.needed_bytes), free: bytes(info.free_bytes) })}${info.in_place_ok ? ` - ${t('can be used in place')}` : info.in_place ? ` - ${info.in_place}` : ''}</span>`}
            </div>`}
            <div class=${'drop-zone' + (s.dropOver ? ' over' : '')}><${Icon} name="download" size=${26} /><div style="margin-top:6px">${t('Drop a folder or an archive here')}</div></div>
            ${status && html`<div class="muted small">${t('Free space: {n}', { n: bytes(status.free_bytes) })}</div>`}
          </div>
        </${Card}>
        <${Card} title="Installs" actions=${jobs.some((j) => !jobRunning(j)) && html`<${Button} small kind="ghost" label="Clear finished" onClick=${() => call('mods_clear_finished').then(pollNow).catch((e) => toast(t(e.message), 'error'))} />`}>
          ${jobs.length === 0
            ? html`<p class="muted small">${t('Nothing installed since the launcher started. Big archives are checked against the free disk space before anything is unpacked; a cancelled or failed install leaves nothing behind.')}</p>`
            : html`<div class="stack">${jobs.map((j) => html`<${Job} j=${j} />`)}</div>`}
        </${Card}>
        ${status && html`<${Card} title="The Mods folder">
          <div class="stack" style="gap:10px">
            <p class="muted small">${t('Anything put into {} is installed by itself once it has finished copying.', [t('this folder')])}</p>
            <div class="row"><code class="small selectable grow ellipsis">${status.inbox}</code><${Button} small icon="folder" title="Open" onClick=${() => call('open_path', { path: status.inbox })} /></div>
            ${status.inbox_items?.length > 0 && html`<div class="muted small">${t('In it now: {}', [status.inbox_items.join(', ')])}</div>`}
          </div>
        </${Card}>`}
      </div>
    </div>
    ${confirm}
  `;
}

function Job({ j }) {
  const pct = j.bytes_total ? (j.bytes_done / j.bytes_total) * 100 : j.files_total ? (j.files_done / j.files_total) * 100 : 0;
  const active = jobRunning(j);
  return html`<div class="stack" style="gap:6px">
    <div class="row">
      ${active ? html`<${Spinner} />` : html`<span class=${'dot ' + (j.state === 'done' ? 'ok' : j.state === 'cancelled' ? '' : 'danger')}></span>`}
      <b class="grow ellipsis">${j.name}</b>
      <span class="muted small">${t(j.state)}</span>
      ${active && html`<${Button} small kind="ghost" icon="x" title="Cancel" onClick=${() => call('mod_cancel', { id: j.id }).then(pollNow).catch((e) => toast(t(e.message), 'error'))} />`}
    </div>
    ${active && html`<div class="progress"><div style=${`width:${pct.toFixed(1)}%`}></div></div>`}
    ${active && (j.files_total > 0 || j.bytes_total > 0) && html`<div class="muted small">${[j.files_total > 0 && t('{done} / {total} files', { done: j.files_done, total: j.files_total }), j.bytes_total > 0 && `${bytes(j.bytes_done)} / ${bytes(j.bytes_total)}`].filter(Boolean).join(' - ')}</div>`}
    ${j.message && html`<div class=${'small ' + (j.state === 'failed' ? '' : 'muted')} style=${j.state === 'failed' ? 'color:var(--danger)' : ''}>${j.message}</div>`}
    ${j.warnings?.length > 0 && html`<div class="small" style="color:var(--warn)">${j.warnings.join(' - ')}</div>`}
  </div>`;
}

// A phone's storage, a folder at a time: the folder to install (a mod unpacked there) or one
// of its archives.
function StorageBrowser({ archive, onClose, onPick }) {
  const [dir, setDir] = useState(null);
  const [error, setError] = useState(null);
  const open = async (path) => {
    try {
      setDir(await call('list_dir', { path, archives: archive }));
      setError(null);
    } catch (e) {
      setError(e.message);
    }
  };
  useEffect(() => {
    open(null);
  }, []);
  const join = (name) => `${dir.path.replace(/\/$/, '')}/${name}`;
  const footer = html`
    ${!archive && dir && html`<${Button} kind="primary" icon="download" label="Install this folder" onClick=${() => onPick(dir.path)} />`}
    <${Button} label="Close" onClick=${onClose} />`;
  return html`<${Dialog} title=${archive ? 'Choose an archive' : 'Choose a folder'} onClose=${onClose} footer=${footer}>
    <div class="stack" style="gap:8px">
      ${error && html`<${Notice} kind="danger">${error}</${Notice}>`}
      ${!dir && !error && html`<${Spinner} />`}
      ${dir && html`
        <p class="dim selectable" style="overflow-wrap:anywhere">${dir.path}</p>
        <div class="stack" style="gap:4px;max-height:50vh;overflow:auto">
          ${dir.parent && html`<${Button} kind="ghost" icon="left" label=".." onClick=${() => open(dir.parent)} />`}
          ${dir.dirs.map((d) => html`<${Button} kind="ghost" icon="folder" label=${d} onClick=${() => open(join(d))} />`)}
          ${dir.files.map((f) => html`<${Button} kind="ghost" icon="file" label=${f} onClick=${() => onPick(join(f))} />`)}
          ${!dir.dirs.length && !dir.files.length && html`<p class="dim">${t('Nothing here')}</p>`}
        </div>`}
    </div>
  </${Dialog}>`;
}
