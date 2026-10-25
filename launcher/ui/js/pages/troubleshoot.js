// Troubleshooting (a page of the Help group, below Tutorials): what is wrong with the installed content, by the bus, map
// or pack it concerns, with a fresh look on request - and the model test, which loads one
// piece the way the game does (it was a tab of Settings up to here).

import { html, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast, remembered, remember } from '../store.js';
import { t, ago, language } from '../i18n.js';
import { call } from '../api.js';
import { fixChoice, updateHof, pollNow } from '../data.js';
import { Icon } from '../icons.js';
import { Button, Card, Empty, Notice, PageHead, Search, Spinner, Tabs, Tag } from '../ui.js';
import { ContentTest } from './content-test.js';

const KIND_ICON = { bus: 'bus', pack: 'puzzle', mod: 'download', general: 'alert' };
const LEVEL_TAG = { error: 'danger', warn: 'accent', info: '' };

/** The problems known now, by what they concern: [{ kind, id, name, issues: [{ key, level,
 * text, items }] }] (errors first). Read from what the launcher holds already: the mods'
 * status, the bus list and the installs since it started. */
export function problems(s) {
  // (the sidebar and the phone menu ask on every render: the list is kept while what it is
  // read from, and the language of its texts, stay the same)
  const key = [s.cache.mods, s.vehicles, s.poll.jobs, language()];
  if (memo && memo.key.every((k, i) => k === key[i])) return memo.list;
  const list = collect(s);
  memo = { key, list };
  return list;
}

let memo = null;

function collect(s) {
  const status = s.cache.mods;
  const groups = new Map();
  const add = (kind, id, name, issue) => {
    const k = `${kind}:${id}`;
    if (!groups.has(k)) groups.set(k, { kind, id, name, issues: [] });
    groups.get(k).issues.push({ ...issue, key: `${k}:${issue.code}` });
  };
  for (const w of status?.waiting || []) {
    add('pack', w, w, { code: 'waiting', level: 'warn', text: t('Packs installed before the bus they belong to: they go in once it is there.') });
  }
  for (const v of s.vehicles || []) {
    if (v.missing_packs?.length) {
      add('bus', v.file, v.name || v.file, { code: 'packs', level: 'warn', text: t('Parts of this bus come from packs that are not installed: {packs}', { packs: v.missing_packs.join(', ') }) });
    }
  }
  if (status?.hof_warnings?.length) {
    add('general', 'hof', t('Map HOF issue'), { code: 'hof', level: 'warn', text: t('Some map HOF files could not be added. Check launcher.log for details.'), items: status.hof_warnings });
  }
  for (const j of s.poll.jobs || []) {
    if (j.state === 'failed') add('mod', j.id, j.name, { code: 'install', level: 'error', text: j.message || t('failed'), items: j.warnings });
  }
  const worst = (g) => (g.issues.some((i) => i.level === 'error') ? 0 : 1);
  return [...groups.values()].sort((a, b) => worst(a) - worst(b) || a.name.localeCompare(b.name));
}

/** Every issue's key of the last scan, to tell what a new one has resolved. */
let lastKeys = null;

const FILTERS = [['all', 'All'], ['bus', 'Buses'], ['pack', 'Packs'], ['other', 'Other']];
const filterOf = (g) => (g.kind === 'bus' || g.kind === 'pack' ? g.kind : 'other');

function Overview({ onCheck }) {
  const s = useStore();
  const [busy, setBusy] = useState(false);
  const [q, setQ] = useState('');
  const [show, setShow] = useState('all');
  const [open, setOpen] = useState(null);
  const [resolved, setResolved] = useState([]);
  const all = problems(s);
  const counts = { all: all.length, bus: 0, pack: 0, other: 0 };
  for (const g of all) counts[filterOf(g)]++;
  const kind = counts[show] ? show : 'all';
  const shown = all
    .filter((g) => kind === 'all' || filterOf(g) === kind)
    .filter((g) => !q || `${g.name} ${g.id}`.toLowerCase().includes(q.toLowerCase()));
  const scan = async () => {
    if (busy) return;
    setBusy(true);
    const before = lastKeys ?? new Set(problems(S).flatMap((g) => g.issues.map((i) => i.key)));
    try {
      const [mods, vehicles] = await Promise.all([call('mods_status'), call('vehicles'), pollNow()]);
      set({ cache: { ...S.cache, mods }, vehicles, scannedAt: Date.now() / 1000 });
      // (as the content loading does: a bus taken away since must not stay the chosen one)
      fixChoice();
      updateHof();
      const now = problems(S);
      const keys = new Set(now.flatMap((g) => g.issues.map((i) => i.key)));
      const gone = [...before].filter((k) => !keys.has(k));
      lastKeys = keys;
      setResolved(gone);
      toast(t('{n} problems resolved, {m} left', { n: gone.length, m: keys.size }), gone.length ? 'ok' : 'info');
    } catch (e) {
      toast(e.message, 'error');
    }
    setBusy(false);
  };
  // (a failed install stays in the installs' list until it is cleared: the way to resolve it)
  const clearInstalls = () => call('mods_clear_finished').then(pollNow).catch((e) => toast(t(e.message), 'error'));
  return html`<div class="stack">
    <${Card} title="Problems by content" actions=${html`<div class="row" style="gap:8px">
        ${s.scannedAt && html`<span class="muted small">${t('Last scan: {when}', { when: ago(s.scannedAt) })}</span>`}
        <${Button} kind="primary" icon="refresh" label=${busy ? 'Scanning...' : 'Scan again'} disabled=${busy} onClick=${scan} />
      </div>`}>
      ${resolved.length > 0 && html`<div style="margin-bottom:12px"><${Notice} kind="ok">${t('{n} problems resolved since the last scan', { n: resolved.length })}</${Notice}></div>`}
      ${all.length > 0 && html`<div class="row wrap" style="margin-bottom:12px;gap:8px">
        <div class="segmented wrap">
          ${FILTERS.filter(([k]) => k === 'all' || counts[k] > 0).map(([k, l]) => html`<button class=${k === kind ? 'on' : ''} onClick=${() => setShow(k)}>${t(l)} <span class="muted">${counts[k]}</span></button>`)}
        </div>
        <div class="grow"></div>
        <div style="width:220px;max-width:100%"><${Search} value=${q} onInput=${setQ} placeholder="Search..." /></div>
      </div>`}
      ${busy && all.length === 0
        ? html`<div class="row muted"><${Spinner} /> ${t('Scanning...')}</div>`
        : shown.length === 0
          ? html`<${Empty} icon="check" text=${q || kind !== 'all' ? 'Nothing matches.' : 'No problems found.'} />`
          : html`<div class="list" style="margin:0 -8px">${shown.map((g) => {
              const key = `${g.kind}:${g.id}`;
              const errors = g.issues.filter((i) => i.level === 'error').length;
              const warns = g.issues.length - errors;
              return html`<div>
                <div class="list-item" onClick=${() => setOpen(open === key ? null : key)}>
                  <${Icon} name=${KIND_ICON[g.kind] || 'alert'} />
                  <div class="grow" style="min-width:0">
                    <div class="title ellipsis">${g.name}</div>
                    <div class="sub ellipsis">${g.issues[0].text}</div>
                  </div>
                  ${errors > 0 && html`<${Tag} kind="danger">${t('errors')}: ${errors}</${Tag}>`}
                  ${warns > 0 && html`<${Tag} kind="accent">${t('warnings')}: ${warns}</${Tag}>`}
                </div>
                ${open === key && html`<div class="stack" style="padding:4px 12px 12px 44px;gap:8px">
                  ${g.issues.map((i) => html`<div class="row" style="align-items:flex-start">
                    <${Tag} kind=${LEVEL_TAG[i.level] || ''}>${t(i.level)}</${Tag}>
                    <div class="grow small" style="min-width:0">
                      <div class="selectable" style="word-break:break-word">${i.text}</div>
                      ${i.items?.length > 0 && html`<ul class="muted selectable" style="margin:4px 0 0;padding-left:18px">${i.items.slice(0, 50).map((x) => html`<li style="word-break:break-word">${x}</li>`)}${i.items.length > 50 && html`<li>+${i.items.length - 50}</li>`}</ul>`}
                    </div>
                  </div>`)}
                  ${g.kind === 'mod' && html`<div><${Button} small icon="x" label="Clear finished" onClick=${clearInstalls} /></div>`}
                  ${g.kind === 'bus' && onCheck && html`<div><${Button} small icon="bus" label="Check in model test" onClick=${() => onCheck({ kind: 'bus', target: g.id })} /></div>`}
                </div>`}
              </div>`;
            })}</div>`}
    </${Card}>
  </div>`;
}

const SUBTABS = [['overview', 'Overview'], ['content', 'Model test']];

export function TroubleshootPage() {
  const s = useStore();
  // (the model test runs the game program with --check: not on a phone, as in Settings before)
  const phone = ['android', 'ios'].includes(s.boot?.platform || '');
  const [tab, setTab] = useState(() => {
    const id = remembered('troubleshoot-tab', 'overview');
    return SUBTABS.some(([k]) => k === id) ? id : 'overview';
  });
  const [initial, setInitial] = useState(null);
  const pick = (id) => {
    setTab(id);
    remember('troubleshoot-tab', id);
  };
  const check = (what) => {
    setInitial({ ...what, n: (initial?.n || 0) + 1 });
    pick('content');
  };
  const head = html`<${PageHead} title="Troubleshooting" text="Packs waiting for their bus, buses missing parts, map HOF files and failed installs. Install what is missing, then scan again." />`;
  if (phone) return html`${head}<${Overview} />`;
  return html`${head}<div class="stack">
    <${Tabs} value=${tab} options=${SUBTABS} onChange=${pick} />
    ${tab === 'content'
      ? html`<div key=${'content-' + (initial?.n || 0)}><${ContentTest} initial=${initial} /></div>`
      : html`<${Overview} onCheck=${check} />`}
  </div>`;
}
