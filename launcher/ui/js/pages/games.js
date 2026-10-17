// Running games: every game the launcher started, its log, and Stop.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t, ago } from '../i18n.js';
import { call } from '../api.js';
import { Button, Card, Empty, Notice, PageHead, Tag, useConfirm } from '../ui.js';
import { pollNow, busName, mapName } from '../data.js';


export function GamesPage() {
  const s = useStore();
  const list = s.poll.instances || [];
  const [ask, confirm] = useConfirm();
  return html`
    <${PageHead} title="Running games" text="Any number of games can run at once; each writes its own log." />
    ${list.length === 0
      ? html`<${Card}><${Empty} icon="gamepad" title="No game is running" text="Start one on the Drive page." /></${Card}>`
      : html`<div class="stack">${list.map((g) => html`<${Game} key=${g.pid} g=${g} ask=${ask} />`)}</div>`}
    ${confirm}
  `;
}

function Game({ g, ask }) {
  const [log, setLog] = useState(null);
  const [stopping, setStopping] = useState(false);
  useEffect(() => {
    if (log == null) return;
    const read = () => call('game_log', { pid: g.pid, lines: 120 }).then(setLog).catch(() => {});
    const id = setInterval(read, 2000);
    return () => clearInterval(id);
  }, [log != null, g.pid]);
  const stop = async () => {
    if (!(await ask({ title: 'Stop this game?', text: 'The game ends as if Escape and Quit were chosen; an unsaved session is lost.', yes: 'Stop', danger: true }))) return;
    setStopping(true);
    try {
      await call('stop_game', { pid: g.pid });
      toast(t('The game was stopped.'), 'ok');
    } catch (e) {
      toast(e.message, 'error');
    }
    setStopping(false);
    pollNow();
  };
  return html`<${Card} body=${false}>
    <div class="card-body row" style="align-items:flex-start">
      <div class="grow stack" style="gap:6px">
        <div class="row"><span class="dot ok"></span><b style="font-size:15px">${mapName(g.map)}</b>${g.line && html`<span class="line-badge">${g.line}</span>`}${g.tour && html`<${Tag}>${t('Tour')} ${g.tour}</${Tag}>`}</div>
        <div class="muted small">${busName(g.bus)} - ${g.profile || '-'} - ${g.stopping ? t('stopping - saving the run...') : t('started {when}', { when: ago(g.started) })} - PID ${g.pid}</div>
        ${g.last_line && html`<div class="muted small ellipsis" title=${g.last_line}>${g.last_line}</div>`}
      </div>
      <${Button} small icon="file" label=${log == null ? 'Show log' : 'Hide log'} onClick=${() => (log == null ? call('game_log', { pid: g.pid, lines: 120 }).then(setLog).catch((e) => setLog([e.message])) : setLog(null))} />
      <${Button} small icon="folder" title="Show the log file" onClick=${() => call('open_path', { path: g.log.replace(/[^/\\]*$/, '') }).catch((e) => toast(e.message, 'error'))} />
      <${Button} small kind="danger" icon="stop" label="Stop" disabled=${stopping} onClick=${stop} />
    </div>
    ${g.lan_status && html`<${Lobby} lan=${g.lan_status} />`}
    ${log != null && html`<div style="padding:0 20px 18px"><div class="log">${log.map((l) => html`<div class=${/ ERROR /.test(l) ? 'log-error' : / WARN /.test(l) ? 'log-warn' : ''}>${l || ' '}</div>`)}</div></div>`}
  </${Card}>`;
}

function Lobby({ lan }) {
  const players = lan.players || [];
  const chat = (lan.chat || []).slice(-6);
  const copy = () => navigator.clipboard.writeText(lan.code).then(() => toast(t('Session code copied'), 'ok'), (e) => toast(e.message, 'error'));
  return html`<div class="stack lobby" style="padding:0 20px 18px;gap:12px">
    ${lan.role === 'host' && lan.code && html`<div class="session-code">
      <div class="grow">
        <div class="section-title">${t('Session code')}</div>
        <div class="code selectable">${lan.code}</div>
        <div class="muted small">${t('Your friends paste it into Multiplayer → Connect by Code.')}</div>
      </div>
      <${Button} kind="primary" icon="copy" label="Copy code" onClick=${copy} />
    </div>`}
    ${lan.role === 'client' && html`<div class=${'small ' + (lan.connected ? 'ok-text' : 'warn-text')}>${
      lan.rejected ? t('Not connected: {why}', { why: lan.rejected }) : lan.connected ? t('Connected to {host}', { host: lan.host_name || '' }) : t('connecting...')
    }</div>`}
    ${players.length > 0 && html`<div>
      <div class="section-title">${t('PLAYERS')}</div>
      ${players.map((p) => html`<div class="small">${[p.name, busName(p.bus || ''), p.destination ? `${p.line} → ${p.destination}` : '', p.passengers > 0 ? t('{n} passengers', { n: p.passengers }) : '', p.where].filter(Boolean).join(' - ')}</div>`)}
    </div>`}
    ${chat.length > 0 && html`<div>
      <div class="section-title">${t('CHAT  (/ in the game to write)')}</div>
      ${chat.map((c) => html`<div class="small selectable">${c}</div>`)}
    </div>`}
    ${(lan.warnings || []).map((w) => html`<${Notice} kind="warn">${w}</${Notice}>`)}
  </div>`;
}
