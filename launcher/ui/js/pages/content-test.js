// Model test (a tab of Troubleshooting): any content loaded by the game program itself
// (`openomsi --check`) - a bus, a map, an installed mod, or any file or folder: a scenery
// object, a spline, a human, a weather, a mesh, a mod not installed yet (folder or zip) -
// every warning and error of its loaders listed, what it names and lacks looked for. What a
// modder used to read off the terminal while working on their content.

import { html, useState, useEffect, useMemo } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Empty, Input, Search, Select, Spinner, Tabs, Tag, Notice, Toggle } from '../ui.js';

const LEVEL_TAG = { error: 'danger', warn: 'accent', info: '' };

/** What can be checked: the bus list, the map list, the installed mods, or a path. */
const KINDS = [
  ['bus', 'Bus'],
  ['map', 'Map'],
  ['mod', 'Installed mod'],
  ['path', 'File or folder'],
];

/** The running game's log as it grows: its warnings and errors (or everything), read every
 * two seconds - what the terminal showed while content was tried in the game. */
function LiveLog() {
  const s = useStore();
  const games = s.poll.instances || [];
  const game = games[0] || null;
  const [lines, setLines] = useState([]);
  const [all, setAll] = useState(false);
  const [skip, setSkip] = useState(0);
  const pid = game?.pid;
  useEffect(() => {
    if (!pid) {
      setLines([]);
      return;
    }
    const read = () => call('game_log', { pid, lines: 600 }).then(setLines).catch(() => {});
    read();
    const id = setInterval(read, 2000);
    return () => clearInterval(id);
  }, [pid]);
  const shown = lines.filter((l) => all || /WARN|ERROR|panicked/.test(l)).slice(skip);
  return html`<${Card} title="Live, from the running game" actions=${html`<div class="row">
      <${Toggle} checked=${all} onChange=${setAll} label="Everything" />
      <${Button} small label="Clear" onClick=${() => setSkip(lines.filter((l) => all || /WARN|ERROR|panicked/.test(l)).length)} disabled=${!game} />
    </div>`}>
    ${!game
      ? html`<div class="muted">${t('No game runs. Start one on the Drive page with the content you work on; its warnings appear here as they happen.')}</div>`
      : shown.length === 0
        ? html`<div class="muted">${t('Nothing yet from the game ({pid}).', { pid })}</div>`
        : html`<div class="log" style="max-height:320px;overflow:auto">${shown.join('\n')}</div>`}
  </${Card}>`;
}

/** The figures of one checked piece, by its kind. For a folder (a mod) they are filters:
 * clicking a count ("objects: 12") shows only the pieces of that kind (`only`); clicking it
 * again, or "checked", shows all of them again. */
function SummaryTags({ sum, only, setOnly }) {
  if (!sum) return null;
  const tag = (label, v, kind = '') => v != null && html`<${Tag} kind=${kind}>${t(label)}: ${v}</${Tag}>`;
  // (a folder's counts are <button>s, not Tags, so they can be reached and pressed with the keyboard)
  const filter = (label, n, on, onClick, title) => html`<button class=${'tag tag-button' + (on ? ' accent' : '')} title=${title} onClick=${onClick}>${label}: ${n}</button>`;
  const textures = sum.textures != null && html`<${Tag} kind=${sum.textures_missing ? 'danger' : 'ok'}>${t('textures')}: ${sum.textures}${sum.textures_missing ? ` (${t('{n} missing', { n: sum.textures_missing })})` : ''}</${Tag}>`;
  switch (sum.kind) {
    case 'vehicle':
      return html`${sum.parts?.length > 1 && tag('parts', sum.parts.length, 'accent')}${tag('meshes', sum.meshes)}${tag('materials', sum.materials)}${textures}${tag('paint schemes', sum.paint_schemes)}${tag('script variables', sum.variables)}`;
    case 'object':
      return html`${tag('meshes', sum.meshes)}${tag('materials', sum.materials)}${textures}${tag('script variables', sum.variables)}${tag('paths', sum.paths)}`;
    case 'spline':
      return html`${textures}${tag('profiles', sum.profiles)}${tag('paths', sum.paths)}`;
    case 'human':
      return html`${tag('meshes', sum.meshes)}${textures}`;
    case 'mesh':
      return html`${tag('materials', sum.materials)}${tag('vertices', sum.vertices)}`;
    case 'map':
      return html`${tag('tiles', sum.tiles)}${sum.tiles_missing ? tag('tiles missing', sum.tiles_missing, 'danger') : ''}${tag('objects', sum.objects)}${tag('splines', sum.splines)}
        <${Tag} kind=${sum.files_missing ? 'danger' : 'ok'}>${t('not installed')}: ${sum.files_missing}</${Tag}>
        ${sum.files_broken ? tag('unreadable', sum.files_broken, 'danger') : ''}${tag('entry points', sum.entry_points)}${tag('timetable', sum.timetable ? t('yes') : t('no'))}`;
    case 'folder':
      return html`${Object.entries(sum.counts || {}).map(([k, n]) => filter(t(k), n, only === k, () => setOnly(only === k ? null : k), t('Show only these')))}
        ${filter(t('checked'), sum.items?.length, !only, () => setOnly(null))}`;
  }
  return null;
}

/** The pieces a folder (a mod) holds, each with its own state: a click shows its findings. */
function Items({ items, findings, part, setPart, only, setOnly }) {
  const of = (file) => file.split('/').pop();
  const counts = (file) => {
    const name = of(file);
    const mine = findings.filter((f) => f.part === name || (f.part || '').startsWith(name + ' '));
    return [mine.filter((f) => f.level === 'error').length, mine.filter((f) => f.level === 'warn').length];
  };
  return html`<${Card} title="What it holds" actions=${(part || only) && html`<${Button} small label="Show all" onClick=${() => (setPart(null), setOnly(null))} />`}>
    <div class="list" style="margin:0 -8px;max-height:360px;overflow:auto">${items.map((it) => {
      const [e, w] = counts(it.file);
      return html`<div class=${'list-item' + (part === of(it.file) ? ' on' : '')} onClick=${() => setPart(part === of(it.file) ? null : of(it.file))}>
        <${Tag} kind=${!it.ok || e ? 'danger' : w ? 'accent' : 'ok'}>${t(it.kind)}</${Tag}>
        <div class="grow" style="min-width:0"><div class="title" style="word-break:break-word">${it.summary?.name || it.file}</div><div class="sub">${it.file}${e ? ` · ${t('{n} errors', { n: e })}` : ''}${w ? ` · ${t('{n} warnings', { n: w })}` : ''}</div></div>
      </div>`;
    })}</div>
  </${Card}>`;
}

// (the Check button's icon by what is checked)
const KIND_ICONS = { bus: 'bus', map: 'map', mod: 'puzzle' };

export function ContentTest({ initial } = {}) {
  const s = useStore();
  const vehicles = s.vehicles || [];
  const maps = s.maps || [];
  const [kind, setKind] = useState(initial?.kind || 'bus');
  const [bus, setBus] = useState((initial?.kind === 'bus' && initial.target) || s.choice?.bus || vehicles[0]?.file || '');
  const [maker, setMaker] = useState('');
  const makers = useMemo(() => [...new Set(vehicles.map((v) => v.manufacturer).filter(Boolean))].sort(), [vehicles]);
  const sorted = useMemo(() => [...vehicles].sort((a, b) => (a.manufacturer || '').localeCompare(b.manufacturer || '') || (a.name || a.file).localeCompare(b.name || b.file)), [vehicles]);
  const buses = sorted.filter((v) => !maker || v.manufacturer === maker);
  const pickMaker = (m) => {
    setMaker(m);
    const of = sorted.filter((v) => !m || v.manufacturer === m);
    if (!of.some((v) => v.file === bus)) setBus(of[0]?.file || '');
  };
  const [map, setMap] = useState((initial?.kind === 'map' && initial.target) || s.choice?.map || maps[0]?.file || '');
  const [mods, setMods] = useState([]);
  const [mod, setMod] = useState('');
  const [path, setPath] = useState('');
  const [deep, setDeep] = useState(false);
  const [result, setResult] = useState(null);
  const [busy, setBusy] = useState(false);
  const [all, setAll] = useState(false);
  const [part, setPart] = useState(null);
  const [only, setOnly] = useState(null);
  const [modQ, setModQ] = useState('');
  useEffect(() => {
    call('mods_status')
      .then((r) => {
        const list = (r.installed || []).filter((m) => m.enabled && m.paths?.length);
        setMods(list);
        if (!mod && list[0]) setMod(list[0].id);
      })
      .catch(() => {});
  }, []);
  // what the check is given: one path, or the paths of a mod
  const target = () => {
    if (kind === 'bus') return bus ? [bus] : [];
    if (kind === 'map') return map ? [map] : [];
    if (kind === 'mod') return mods.find((m) => m.id === mod)?.paths || [];
    return path.trim() ? [path.trim()] : [];
  };
  const run = async () => {
    const paths = target();
    if (!paths.length || busy) return;
    setBusy(true);
    setPart(null);
    setOnly(null);
    try {
      setResult(await call('content_check', { path: paths.join('\n'), deep }));
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy(false);
  };
  const browse = async (cmd, title) => {
    const p = await call(cmd, cmd === 'pick_mod' ? { archive: true } : { title: t(title) }).catch(() => null);
    if (p) setPath(p);
  };
  const allFindings = result?.findings || [];
  const sum = result?.summary || {};
  const allItems = sum.items || [];
  const items = only ? allItems.filter((it) => it.kind === only) : allItems;
  // (with a kind chosen, the findings of its pieces only)
  const names = only && new Set(items.map((it) => it.file.split('/').pop()));
  const ofPart = (f, name) => f.part === name || (f.part || '').startsWith(name + ' ');
  const findings = allFindings.filter((f) => (all || f.level !== 'info') && (part ? ofPart(f, part) : !names || [...names].some((n) => ofPart(f, n))));
  const shownMods = mods.filter((m) => !modQ || `${m.name} ${m.paths.join(' ')}`.toLowerCase().includes(modQ.toLowerCase()));
  const picker = {
    bus: html`<div class="row"><div style="flex:0 0 200px;min-width:0"><${Select} raw value=${maker} options=${[['', t('All makers')], ...makers.map((m) => [m, m])]} onChange=${pickMaker} /></div>
      <div class="grow" style="min-width:0"><${Select} value=${bus} options=${buses.map((v) => [v.file, v.name ? `${v.name} (${v.file.split('/').pop()})` : v.file])} onChange=${setBus} raw placeholder="Choose a bus" /></div></div>`,
    map: html`<${Select} value=${map} options=${maps.map((m) => [m.file, m.friendly || m.name || m.file])} onChange=${setMap} raw placeholder="Choose a map" />`,
    mod: mods.length
      ? html`<div class="stack" style="gap:8px">
          <${Search} value=${modQ} onInput=${setModQ} placeholder="Search mods..." />
          <div class="list" style="max-height:240px;overflow:auto">
            ${shownMods.length === 0
              ? html`<div class="muted small" style="padding:8px">${t('No mod matches.')}</div>`
              : shownMods.map((m) => html`<div class=${'list-item' + (m.id === mod ? ' on' : '')} onClick=${() => setMod(m.id)}>
                  <div class="grow" style="min-width:0"><div class="title ellipsis">${m.name}</div><div class="sub ellipsis">${m.paths.length > 1 ? t('{n} folders', { n: m.paths.length }) : m.paths[0]}</div></div>
                  ${m.id === mod && html`<${Icon} name="check" />`}
                </div>`)}
          </div>
        </div>`
      : html`<div class="muted">${t('No mod is installed in the content folder.')}</div>`,
    path: html`<div class="row"><div class="grow"><${Input} value=${path} placeholder="A .bus, .ovh, .sco, .sli, .hum, .owt, .o3d, a map's global.cfg, or a folder or .zip" onInput=${setPath} /></div>
      <${Button} icon="file" label="File…" onClick=${() => browse('pick_file', 'Choose the file to check')} />
      <${Button} icon="folder" label="Folder…" onClick=${() => browse('pick_folder', 'Choose the folder to check')} />
      <${Button} icon="puzzle" label="Zip…" onClick=${() => browse('pick_mod')} /></div>`,
  };
  return html`<div class="stack">
    <${Card} title="Model test">
      <p class="muted small" style="margin:0 0 12px">${t('Anything loaded the way the game loads it - a bus with the parts coupled behind it, a map with every tile, object and spline it places, a mod, a scenery object, a spline, a human, a weather or a mesh - and every warning of its files listed, with what it names that is not there. Edit the content, press Check again: the list is fresh each time.')}</p>
      <${Tabs} value=${kind} options=${KINDS} onChange=${(k) => (setKind(k), setResult(null))} />
      <div class="row" style="margin-top:12px;align-items:center">
        <div class="grow" style="min-width:320px">${picker[kind]}</div>
        <${Button} kind="primary" icon=${KIND_ICONS[kind] || 'check'} label=${busy ? 'Checking…' : 'Check'} onClick=${run} disabled=${busy || !target().length} />
        ${busy && html`<${Spinner} />`}
      </div>
      ${kind !== 'bus' && html`<div style="margin-top:8px"><${Toggle} checked=${deep} onChange=${setDeep} label="Load every object and spline of a map whole: models, meshes and textures (slower)" /></div>`}
    </${Card}>
    ${!result
      ? html`<${Empty} icon=${KIND_ICONS[kind] || 'check'} text="Choose what to check and press Check." />`
      : html`<div class="stack">
        ${result.ok
          ? html`<${Notice} kind="ok">${t('Loaded without errors: {name}', { name: sum.name || result.file })}${result.warnings ? ` · ${t('{n} warnings', { n: result.warnings })}` : ''}</${Notice}>`
          : html`<${Notice} kind="danger">${t('It did not load cleanly: {n} errors, {w} warnings', { n: result.errors, w: result.warnings })}</${Notice}>`}
        <${Card} title="Summary">
          <div class="row wrap">
            <${SummaryTags} sum=${sum} only=${only} setOnly=${(value) => { setPart(null); setOnly(value); }} />
            <span class="muted">${result.seconds?.toFixed(1)} s · ${t('{n} log lines', { n: result.log_lines })} · ${result.file}</span>
          </div>
        </${Card}>
        ${allItems.length > 0 && html`<${Items} items=${items} findings=${allFindings} part=${part} setPart=${setPart} only=${only} setOnly=${setOnly} />`}
        <${Card} title=${part ? `${t('Findings')}: ${part}` : 'Findings'} actions=${html`<${Toggle} checked=${all} onChange=${setAll} label="Everything the loaders said" />`}>
          ${findings.length === 0
            ? html`<div class="muted">${t('Nothing to report.')}</div>`
            : html`<div class="list" style="margin:0 -8px">${findings.slice(0, 2000).map((f) => html`<div class="list-item" style="cursor:default;align-items:flex-start">
                <${Tag} kind=${LEVEL_TAG[f.level] || ''}>${t(f.level)}</${Tag}>
                <div class="grow" style="min-width:0">
                  <div class="title selectable" style="white-space:pre-wrap;word-break:break-word;font-weight:500">${f.text}</div>
                  <div class="sub selectable" style="word-break:break-word">${f.part ? f.part + ' · ' : ''}${f.what}${f.hint ? ' · ' + f.hint : ''}</div>
                </div>
              </div>`)}</div>`}
        </${Card}>
      </div>`}
    <${LiveLog} />
  </div>`;
}
