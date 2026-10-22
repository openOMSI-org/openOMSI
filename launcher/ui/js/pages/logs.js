// Logs (a tab of Troubleshooting): the log of a running game as it is written, the last log
// of each game slot (game.log, game-n.log: a new game overwrites it) and the launcher's own,
// to read and to delete (each one beside its name in the list).

import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t, ago, bytes, language } from '../i18n.js';
import { call } from '../api.js';
import { Button, Card, Empty, Search, Segmented, Spinner, Tag, Toggle, useConfirm } from '../ui.js';
import { busName, mapName } from '../data.js';

const LEVELS = [['all', 'All'], ['warn', 'Warnings'], ['error', 'Errors']];
/** When an earlier log was last written: "Sun 19 Oct, 14:03". */
const when = (secs) => new Date(secs * 1000).toLocaleString(language(), { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });

const levelOf = (l) => (/ ERROR /.test(l) ? 'error' : / WARN /.test(l) ? 'warn' : '');

/** The lines a level and a search leave. */
function visibleLines(lines, level, q) {
  const needle = q.trim().toLowerCase();
  return lines.filter((l) => {
    const lv = levelOf(l);
    if (level === 'error' && lv !== 'error') return false;
    if (level === 'warn' && !lv) return false;
    return !needle || l.toLowerCase().includes(needle);
  });
}

export function LogsPage() {
  const s = useStore();
  const [files, setFiles] = useState(null);
  const [name, setName] = useState(null);
  const [ask, confirm] = useConfirm();
  const read = () =>
    call('log_files')
      .then((list) => {
        setFiles(list);
        // (a running game's log first, else the newest)
        setName((cur) => (cur && list.some((f) => f.name === cur) ? cur : list[0]?.name || null));
      })
      .catch((e) => (setFiles([]), toast(e.message, 'error')));
  // (a game started or ended: the list changes)
  const running = (s.poll.instances || []).filter((i) => i.running).map((i) => i.pid).join(',');
  useEffect(() => {
    read();
  }, [running]);
  // (a running game's log is still written: it is never deleted)
  const remove = async (f) => {
    if (!(await ask({ title: 'Delete this log file?', text: t('{name} is deleted for good.', { name: f.name }), yes: 'Delete', danger: true }))) return;
    try {
      await call('log_delete', { names: [f.name] });
      toast(t('The log file was deleted.'), 'ok');
    } catch (e) {
      toast(e.message, 'error');
    }
    read();
  };
  if (!files) return html`<${Card}><${Spinner} /></${Card}>`;
  if (!files.length) return html`<${Card}><${Empty} icon="file" title="No logs yet" text="Each game writes a log while it runs; it is shown here." /></${Card}>`;
  const live = files.filter((f) => f.pid);
  const saved = files.filter((f) => !f.pid);
  const current = files.find((f) => f.name === name) || files[0];
  const instances = s.poll.instances || [];
  const item = (f) => {
    const g = f.pid && instances.find((i) => i.pid === f.pid);
    const title = g ? `${mapName(g.map)} - ${busName(g.bus)}` : f.kind === 'launcher' ? t('Launcher') : t('Game') + ' - ' + when(f.modified);
    return html`<div class=${'list-item' + (f.name === current.name ? ' on' : '')} onClick=${() => setName(f.name)} title=${f.path}>
      ${f.pid ? html`<span class="dot ok"></span>` : null}
      <div class="grow"><div class="title ellipsis">${title}</div><div class="sub">${f.pid ? t('live') + ' - PID ' + f.pid : f.kind === 'launcher' ? ago(f.modified) : f.name} - ${bytes(f.size)}</div></div>
      ${!f.pid && html`<span class="logs-delete" onClick=${(e) => (e.stopPropagation(), remove(f))}><${Button} small kind="ghost" icon="trash" title="Delete this log file" /></span>`}
    </div>`;
  };
  return html`<div class="logs-grid">
    <${Card} title="Log files" body=${false} actions=${html`<${Button} small kind="ghost" icon="refresh" title="Look again" onClick=${read} />`}>
      <div class="list logs-list">
        ${live.length > 0 && html`<div class="nav-title">${t('Running games')}</div>${live.map(item)}`}
        ${saved.length > 0 && html`<div class="nav-title">${t('Earlier')}</div>${saved.map(item)}`}
      </div>
    </${Card}>
    <${LogView} key=${current.name} file=${current} />
    ${confirm}
  </div>`;
}

function LogView({ file }) {
  const [lines, setLines] = useState(null);
  const [level, setLevel] = useState('all');
  const [q, setQ] = useState('');
  const [follow, setFollow] = useState(true);
  const box = useRef();
  const live = !!file.pid;
  useEffect(() => {
    let gone = false;
    const read = () =>
      call('log_file', { name: file.name, lines: 2000 })
        .then((r) => !gone && setLines(r.lines))
        .catch((e) => !gone && setLines([e.message]));
    read();
    // (the running game's log, as it is written)
    const id = live ? setInterval(read, 1000) : null;
    return () => ((gone = true), id && clearInterval(id));
  }, [file.name, live]);
  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [lines, follow, level, q]);
  const shown = lines ? visibleLines(lines, level, q) : [];
  const counts = lines ? { error: lines.filter((l) => levelOf(l) === 'error').length, warn: lines.filter((l) => levelOf(l) === 'warn').length } : null;
  const copy = () =>
    navigator.clipboard
      ?.writeText(shown.join('\n'))
      .then(() => toast(t('Copied'), 'ok'))
      .catch((e) => toast(e.message, 'error'));
  const actions = html`
    <${Button} small kind="ghost" icon="copy" title="Copy the lines shown" onClick=${copy} />
    <${Button} small kind="ghost" icon="folder" title="Show the log file" onClick=${() => call('open_path', { path: file.path.replace(/[^/\\]*$/, '') }).catch((e) => toast(e.message, 'error'))} />`;
  return html`<${Card} title=${live ? 'Live log' : 'Log'} hint=${file.path} actions=${actions}>
    <div class="stack" style="gap:12px">
      <div class="row" style="flex-wrap:wrap">
        <${Segmented} value=${level} options=${LEVELS} onChange=${setLevel} />
        ${counts && counts.error > 0 && html`<${Tag} kind="danger">${t('Errors')}: ${counts.error}</${Tag}>`}
        ${counts && counts.warn > 0 && html`<${Tag} kind="accent">${t('Warnings')}: ${counts.warn}</${Tag}>`}
        <div class="grow" style="min-width:160px"><${Search} value=${q} onInput=${setQ} placeholder="Search the log" /></div>
        ${live && html`<${Toggle} checked=${follow} onChange=${setFollow} label="Follow" />`}
      </div>
      ${lines == null
        ? html`<${Spinner} />`
        : html`<div class="log logs-view" ref=${box} onScroll=${(e) => {
            const el = e.currentTarget;
            if (live) setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 24);
          }}>
            ${shown.length ? shown.map((l) => html`<div class=${levelOf(l) ? 'log-' + levelOf(l) : ''}>${l || ' '}</div>`) : html`<div class="muted">${t(lines.length ? 'No line matches.' : 'The log is empty.')}</div>`}
          </div>`}
    </div>
  </${Card}>`;
}
