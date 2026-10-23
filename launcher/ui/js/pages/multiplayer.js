// Multiplayer: hosting the next duty, joining a friend's session by its code, and servers
// that are always on.

import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { serverJoinProtocol, serverJoinSettings } from '../server-join.js';
import { Button, Card, Field, Input, Empty, PageHead, Spinner, Tabs, Segmented, Select, Notice, Tag } from '../ui.js';
import { choose, cached, serverJoined, allowedBus, mapName } from '../data.js';

export function MultiplayerPage() {
  const [tab, setTab] = useState('code');
  return html`
    <${PageHead} title="Multiplayer" text="Drive with friends by a code, or on servers that are always on." />
    <${Tabs} value=${tab} options=${[['code', 'Connect by Code'], ['servers', 'Servers']]} onChange=${setTab} />
    ${tab === 'code' ? html`<${ByCode} />` : html`<${Servers} />`}
  `;
}

function FlowSteps({ labels, current }) {
  return html`<ol class="multiplayer-steps" aria-label=${t('Connection progress')}>
    ${labels.map((label, i) => html`<li class=${i <= current ? 'on' : ''} aria-current=${i === current ? 'step' : undefined}>
      <span class="n" aria-hidden="true">${i < current ? '✓' : i + 1}</span><span>${t(label)}</span>
    </li>`)}
  </ol>`;
}

function runningHost(s) {
  return (s.poll.instances || []).filter((i) => i.running !== false).map((i) => i.lan_status).find((x) => x?.role === 'host');
}

function ByCode() {
  const s = useStore();
  const [mode, setMode] = useState(() => runningHost(s) || s.choice.lan_mode === 'host' ? 'host' : 'join');
  return html`<div class="stack multiplayer-by-code">
    <${Segmented} value=${mode} options=${[['join', 'Join friends'], ['host', 'Invite friends']]} onChange=${setMode} />
    ${mode === 'join' ? html`<${JoinByCode} />` : html`<${HostByCode} />`}
  </div>`;
}

function FirstTimeHelp({ children }) {
  return html`<details class="card multiplayer-help-panel"><summary>${t('First time here?')}</summary>
    <div class="card-body">${children}</div>
  </details>`;
}

function ErrorDetails({ text }) {
  return html`<details class="multiplayer-error-details"><summary>${t('Technical details')}</summary><div class="small selectable">${text}</div></details>`;
}

function JoinByCode() {
  const s = useStore();
  const [code, setCode] = useState(s.choice.lan_mode === 'join' ? s.choice.lan_addr : '');
  const [result, setResult] = useState(null);
  const request = useRef(0);
  useEffect(() => () => { request.current++; }, []);
  const edit = (text) => {
    request.current++;
    setCode(text);
    setResult(null);
  };
  const confirm = async () => {
    const asked = code.trim();
    if (!asked || result?.busy) return;
    const id = ++request.current;
    setResult({ code: asked, busy: true });
    try {
      const check = await call('check_join', { text: asked });
      if (id !== request.current) return;
      if (!check.ok) return setResult({ code: asked, error: check.text });
      if (!/^OMSI-/i.test(asked)) return setResult({ code: asked, validated: true });
      setResult({ code: asked, busy: true, validated: true });
      try {
        const host = await call('server_query', { address: asked });
        if (id === request.current) setResult({ code: asked, host });
      } catch (e) {
        // A valid UDP session can be reachable even when its status gateway is not.
        if (id === request.current) setResult({ code: asked, unavailable: e.message });
      }
    } catch (e) {
      if (id === request.current) setResult({ code: asked, error: e.message });
    }
  };
  const current = result?.code === code.trim() ? result : null;
  const host = current?.host;
  const full = host && host.max_players > 0 && host.players >= host.max_players;
  const ready = host || (current?.validated && !current.busy);
  const join = () => {
    if (!current || full || (!host && !current.unavailable && !current.validated)) return;
    request.current++;
    setResult(null);
    const map = (host?.map || '').replace(/\\/g, '/');
    const own = map && S.maps?.some((m) => m.file.toLowerCase() === map.toLowerCase());
    choose({ lan_mode: 'join', lan_addr: current.code, lan_ws_fallback: null, lan_map: map, ...serverJoined(host), ...(own ? { map } : {}) });
    choose(allowedBus());
    set({ page: 'drive' });
  };
  return html`<div class="multiplayer-flow">
    <${Card} body=${false}>
      <${FlowSteps} labels=${['Enter session code', 'Confirm session', 'Choose a bus and start']} current=${ready ? 2 : 0} />
      <div class="card-body stack">
        <div><h2>${t('Join friends')}</h2><p class="muted small">${t('Confirm the session first, then choose your bus on the Drive page. The game has not connected yet.')}</p></div>
        <div class="field">
          <label for="multiplayer-code">${t('Session code')}</label>
          <div class="row multiplayer-code-row">
            <${Input} id="multiplayer-code" value=${code} placeholder="OMSI-XXXX-XXXX-..." onInput=${edit}
              aria-describedby="multiplayer-code-help multiplayer-code-status" aria-invalid=${!!current?.error}
              onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); confirm(); } }} />
            <${Button} kind=${host || current?.unavailable ? '' : 'primary'} label=${host ? 'Check again' : 'Confirm session'} icon="search" disabled=${!code.trim() || current?.busy} onClick=${confirm} />
          </div>
          <span class="help" id="multiplayer-code-help">${t('Paste the complete code from your friend. Leading and trailing spaces are removed.')}</span>
        </div>
        <div id="multiplayer-code-status" role="status" aria-live="polite">
          ${current?.busy && html`<div class="row muted small"><${Spinner} />${t('Checking the session...')}</div><div class="muted small">${t('You can choose a bus without waiting for session details. The game connects when you start driving.')}</div>`}
          ${current?.error && html`<${Notice} kind="danger"><b>${t('Could not validate the code')}</b><div class="muted small selectable">${current.error}</div></${Notice}>`}
          ${current?.unavailable && html`<${Notice} kind="warn"><b>${t('Could not read the session details')}</b><div class="muted small">${t('Ask your friend to keep the game running and send the latest code. Retry, or continue without session details.')}</div><${ErrorDetails} text=${current.unavailable} /></${Notice}>`}
        </div>
        ${host && html`<section class="multiplayer-session-summary" aria-label=${t('Session details')}>
          <div class="row wrap"><${Icon} name="users" /><b>${host.name}</b><${Tag}>${t('{n} of {max} players', { n: host.players, max: host.max_players })}</${Tag}></div>
          <dl><div><dt>${t('Map')}</dt><dd>${s.maps?.find((m) => m.file.toLowerCase() === (host.map || '').replace(/\\/g, '/').toLowerCase())?.name || host.map || t('Unknown')}</dd></div>
          ${host.time && html`<div><dt>${t('Time')}</dt><dd>${host.time}</dd></div>`}
          ${host.weather && html`<div><dt>${t('Weather')}</dt><dd>${host.weather}</dd></div>`}</dl>
          <p class="muted small">${t('Choose your bus next. The game connects only when you start driving.')}</p>
        </section>`}
        ${full && html`<${Notice} kind="warn">${t('The session is full. Ask your friend to make room, then confirm again.')}</${Notice}>`}
        ${host?.map && !s.maps?.some((m) => m.file.toLowerCase() === host.map.replace(/\\/g, '/').toLowerCase()) && html`<${Notice} kind="warn">${t('The server plays {}, which is not installed here: it is fetched from the server on joining.', [host.map])}</${Notice}>`}
        ${!full && (host || current?.unavailable || current?.validated) && html`<div><${Button} kind=${ready ? 'primary' : ''} icon="bus" label=${ready ? 'Choose a bus' : 'Choose a bus without session details'} onClick=${join} /></div>`}
      </div>
    </${Card}>
    <${FirstTimeHelp}><ol class="multiplayer-help muted small">
      <li>${t('Ask your friend to start a hosted duty and copy the session code.')}</li>
      <li>${t('Paste the code here and confirm the session.')}</li>
      <li>${t('Choose your bus on the Drive page, then press Join and drive.')}</li>
    </ol></${FirstTimeHelp}>
  </div>`;
}

function HostByCode() {
  const s = useStore();
  const session = runningHost(s);
  const hosting = s.choice.lan_mode === 'host';
  const prepare = () => {
    choose({ lan_mode: 'host' });
    set({ page: 'drive' });
    // The command starts preparation in the background; it does not report tunnel readiness.
    call('prepare_tunnel').catch(() => toast(t('Internet preparation could not be started. You can still set up a LAN game.'), 'error'));
  };
  const copy = () => navigator.clipboard.writeText(session.code)
    .then(() => toast(t('Session code copied'), 'ok'), (e) => toast(t(e.message), 'error'));
  return html`<div class="multiplayer-flow">
    <${Card} body=${false}>
      <${FlowSteps} labels=${['Set up a hosted duty', 'Start the duty', 'Copy code']} current=${session?.code ? 2 : hosting ? 1 : 0} />
      <div class="card-body stack">
        <div><h2>${t('Invite friends')}</h2><p class="muted small">${t('Start your hosted duty first. A session code appears only after the game starts.')}</p></div>
        ${session?.code ? html`
          <div class="session-code"><div class="grow"><div class="section-title">${t('Session code')}</div><div class="code selectable">${session.code}</div></div>
            <${Button} kind="primary" icon="copy" label="Copy code" onClick=${copy} />
          </div>
          <${Notice} kind=${session.tunnel ? '' : 'warn'}>${t(session.tunnel ? 'Friends can join from anywhere' : 'Getting ready for friends on the internet...')}</${Notice}>
          <p class="muted small">${t('Your friends paste it into Multiplayer → Connect by Code.')}</p>
          <div><${Button} icon="gamepad" label="Running games" onClick=${() => set({ page: 'games' })} /></div>
        ` : html`
          <p class="muted small">${t('Choose your map, bus and duty on the Drive page. No game starts until you press Start the duty.')}</p>
          <div class="row"><${Button} kind="primary" icon="bus" label="Set up a hosted duty" onClick=${prepare} />
            ${hosting && html`<${Button} kind="ghost" label="Cancel" onClick=${() => choose({ lan_mode: 'off' })} />`}</div>
        `}
      </div>
    </${Card}>
    <${FirstTimeHelp}><ol class="multiplayer-help muted small">
      <li>${t('Choose your map, bus and duty, then start the game as host.')}</li>
      <li>${t('Return to Multiplayer to copy your running session code.')}</li>
      <li>${t('Send the complete code to your friends; they choose Join friends.')}</li>
    </ol></${FirstTimeHelp}>
  </div>`;
}

/** Choosing a session is not connecting: the actual game still has to start. */
export function MultiplayerDutyNotice() {
  const s = useStore();
  const c = s.choice;
  if (!c || !['host', 'join'].includes(c.lan_mode)) return null;
  const host = c.lan_mode === 'host';
  return html`<div class="multiplayer-duty-notice"><${Notice} icon="users">
    <div class="row wrap"><div class="grow"><b>${t(host ? 'Host my next duty' : 'Choose a bus and start')}</b>
      <div class="muted small">${t(host ? 'Start the duty to create your session, then return to Multiplayer to copy the code.' : 'The session is selected, not connected yet. Choose your bus and press Join and drive.')}</div>
    </div><${Button} small label="Multiplayer" onClick=${() => set({ page: 'multiplayer' })} />
    <${Button} small kind="ghost" label="Cancel" onClick=${() => choose({ lan_mode: 'off', lan_addr: '', lan_map: '', lan_server: null })} /></div>
  </${Notice}></div>`;
}

function Servers() {
  const s = useStore();
  const [list, setList] = useState(null);
  const [info, setInfo] = useState({});
  const [address, setAddress] = useState('');
  const [name, setName] = useState('');
  const asking = useRef(new Set());
  const shown = useRef(null);
  shown.current = list;
  // `quiet`: the answer replaces the last one without "Asking the server..." in between
  const ask = (a, quiet) => {
    if (asking.current.has(a)) return;
    asking.current.add(a);
    if (!quiet) setInfo((x) => ({ ...x, [a]: { loading: true } }));
    call('server_query', { address: a })
      .then((r) => setInfo((x) => ({ ...x, [a]: { ok: r } })))
      .catch((e) => setInfo((x) => ({ ...x, [a]: { err: e.message } })))
      .finally(() => asking.current.delete(a));
  };
  useEffect(() => {
    cached('servers', 'servers_get').then((l) => {
      if (!Array.isArray(l)) return;
      setList(l);
      l.forEach((x) => ask(x.address));
    });
    // asked again every 15 s while the list is open, as the game's own launcher did: the
    // player counts, the time and the weather stay current
    const id = setInterval(() => !document.hidden && (shown.current || []).forEach((x) => ask(x.address, true)), 15000);
    return () => clearInterval(id);
  }, []);
  const save = (l) => {
    setList(l);
    set({ cache: { ...S.cache, servers: l } });
    call('servers_save', { list: l }).catch((e) => toast(t(e.message), 'error'));
  };
  const add = () => {
    const a = address.trim();
    if (!a) return toast(t("Type the server's address (an IP, a name or a link)"), 'error');
    if (list.some((x) => x.address === a)) return toast(t('That server is in the list already'), 'error');
    save([...list, { name: name.trim() || a, address: a }]);
    setAddress('');
    setName('');
    ask(a);
  };
  const join = (x, fresh) => {
    const i = fresh || info[x.address]?.ok;
    let connection;
    try {
      connection = serverJoinSettings(x.address, x.protocol || 'auto', i);
    } catch (error) {
      return toast(t(error.message), 'error');
    }
    const map = (i?.map || '').replace(/\\/g, '/');
    if (map && s.maps?.some((m) => m.file.toLowerCase() === map.toLowerCase())) choose({ map });
    else if (map) toast(t('The server plays {}, which is not installed here: it is fetched from the server on joining.', [map]));
    choose({ lan_mode: 'join', ...connection, lan_map: i?.map || '', ...serverJoined(i ? { ...i, name: x.name || i.name } : null) });
    choose(allowedBus());
    toast(t('Joined {} - choose your bus and duty, then Start the duty', [x.name || i?.name || x.address]), 'ok');
    set({ page: 'drive' });
  };
  // a server of the public list: asked first (the list says nothing a launcher joins on), then
  // kept in the list and joined like any other
  const joinPublic = async (e) => {
    try {
      const i = await call('server_query', { address: e.address });
      const x = list.find((y) => y.address === e.address) || { name: e.covered ? '' : e.name, address: e.address };
      if (!list.some((y) => y.address === e.address)) save([...list, x]);
      join(x, i);
    } catch (err) {
      toast(t("Can't reach the server: {err}", { err: err.message }), 'error');
    }
  };
  return html`<div class="stack">
    <${PublicServers} onJoin=${joinPublic} onAdd=${(e) => { if (!list.some((y) => y.address === e.address)) { save([...list, { name: e.covered ? '' : e.name, address: e.address }]); ask(e.address); } }} />
    <${Card} title="Add server">
      <div class="row">
        <div class="grow"><${Field} label="Server address"><${Input} value=${address} placeholder="play.example.org" onInput=${setAddress} onKeyDown=${(e) => e.key === 'Enter' && add()} /></${Field}></div>
        <div style="width:240px"><${Field} label="Name (optional)"><${Input} value=${name} onInput=${setName} /></${Field}></div>
        <div style="align-self:flex-end"><${Button} icon="plus" label="Add server" onClick=${add} /></div>
      </div>
    </${Card}>
    ${list == null
      ? html`<div class="row muted"><${Spinner} /></div>`
      : list.length === 0
        ? html`<${Card}><${Empty} icon="globe" text="No servers yet. Add a server by the address its owner gives you." /></${Card}>`
        : list.map((x) => {
            const st = info[x.address] || {};
            const i = st.ok;
            const autoLabel = i ? `${t('Auto')} (${serverJoinProtocol(x.address, 'auto', i) === 'udp' ? 'UDP' : 'WS'})` : t('Auto');
            // the name the player gave it first (one added without a name keeps its address there)
            const official = x.address === 'openomsi';
            const own = !official && x.name && x.name !== x.address ? x.name : '';
            return html`<${Card} body=${false}><div class="card-body row" style="align-items:flex-start;gap:14px">
              ${i?.icon ? html`<img src=${i.icon} width="48" height="48" style="border-radius:8px" />` : html`<div class="avatar" style="width:48px;height:48px;border-radius:8px"><${Icon} name="globe" /></div>`}
              <div class="grow stack" style="gap:4px;min-width:0">
                <div class="row"><b style="font-size:15px">${own || i?.name || x.name || x.address}</b>${i?.password && html`<${Tag}>${t('Password')}</${Tag}>`}</div>
                ${st.loading && html`<div class="row muted small"><${Spinner} /> ${t('Asking the server...')}</div>`}
                ${st.err && html`<div class="small" style="color:var(--danger)">${t("Can't reach the server: {err}", { err: st.err })}</div>`}
                ${st.err && !official && html`<div class="muted small selectable">${x.address}</div>`}
                ${i && html`<div class="muted small">${[i.map && mapName(i.map), t('{n} of {max} players', { n: i.players, max: i.max_players }), i.time, i.weather].filter(Boolean).join(' - ')}</div>`}
                ${i?.motd && html`<div class="dim small">${i.motd}</div>`}
              </div>
              <div style="width:130px"><${Select} raw value=${x.protocol || 'auto'} options=${[['auto', autoLabel], ['udp', t('UDP')], ['websocket', t('WebSocket')]]} onChange=${(protocol) => save(list.map((y) => y === x ? { ...y, protocol } : y))} /></div>
              <${Button} small kind="ghost" icon="refresh" title="Refresh" onClick=${() => ask(x.address)} />
              ${x.address !== 'openomsi' && html`<${Button} small kind="ghost" icon="trash" title="Remove from the list" onClick=${() => save(list.filter((y) => y !== x))} />`}
              <${Button} kind="primary" icon="play" label="Join" disabled=${!i} onClick=${() => join(x)} />
            </div></${Card}>`;
          })}
  </div>`;
}

/** The public server list: names and messages are checked by the launcher before they are shown (a flagged one is covered); a custom icon only when the launcher's own check cleared it. */
function PublicServers({ onJoin, onAdd }) {
  const [state, setState] = useState({ list: null, error: '', loading: false });
  const load = () => {
    setState((x) => ({ ...x, loading: true, error: '' }));
    call('public_servers')
      .then((list) => setState({ list, error: '', loading: false }))
      .catch((e) => setState({ list: null, error: e.message, loading: false }));
  };
  const { list, error, loading } = state;
  return html`<${Card} title="Public servers">
    <div class="stack">
      <div class="row">
        <div class="grow muted small">${t('Servers whose owners chose to list them. Names, messages and pictures are checked here before they are shown.')}</div>
        <${Button} icon="refresh" label=${list ? 'Refresh' : 'Show the public list'} disabled=${loading} onClick=${load} />
      </div>
      ${loading && html`<div class="row muted"><${Spinner} /></div>`}
      ${error && html`<div class="small" style="color:var(--danger)">${t('The public list could not be loaded: {err}', { err: error })}</div>`}
      ${list && list.length === 0 && html`<${Empty} icon="globe" text="No public servers right now." />`}
      ${(list || []).map((e) => html`<div class="row" style="align-items:flex-start;gap:14px" key=${e.id}>
        ${e.icon ? html`<img src=${e.icon} width="40" height="40" style="border-radius:8px" />` : html`<div class="avatar" style="width:40px;height:40px;border-radius:8px"><${Icon} name="globe" /></div>`}
        <div class="grow stack" style="gap:2px;min-width:0">
          <b>${e.name}</b>
          <div class="muted small">${[e.map && mapName(e.map), t('{n} of {max} players', { n: e.players, max: e.max_players }), e.version].filter(Boolean).join(' - ')}</div>
          ${e.motd && html`<div class="dim small">${e.motd}</div>`}
        </div>
        <${Button} small kind="ghost" icon="plus" title="Add to my servers" onClick=${() => onAdd(e)} />
        <${Button} kind="primary" icon="play" label="Join" onClick=${() => onJoin(e)} />
      </div>`)}
    </div>
  </${Card}>`;
}
