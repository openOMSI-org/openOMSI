import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { S, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { serverJoinProtocol, serverJoinSettings } from '../server-join.js';
import { Button, Card, Dialog, Field, Input, Empty, PageHead, Spinner, Tabs, Select, Tag } from '../ui.js';
import { cached, mapName } from '../data.js';

import { selectSession } from './session-selection.js';

export function MultiplayerPage() {
  return html`<${Servers} />`;
}

function Servers() {
  const [list, setList] = useState(null);
  const [info, setInfo] = useState({});
  const [address, setAddress] = useState('');
  const [name, setName] = useState('');
  const [adding, setAdding] = useState(false);
  const [tab, setTab] = useState('saved');
  const [protocol, setProtocol] = useState('auto');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const request = useRef(0);
  useEffect(() => () => { request.current++; }, []);
  const close = () => { request.current++; setAdding(false); setBusy(false); setError(''); };
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
    if (!a) return setError(t("Type the server's address (an IP, a name or a link)"));
    if ((list || []).some((x) => x.address === a)) return setError(t('That server is in the list already'));
    save([...(list || []), { name: name.trim() || a, address: a, protocol }]);
    close();
    setAddress('');
    setName('');
    ask(a);
  };
  const join = (x, fresh) => {
    const i = fresh || info[x.address]?.ok;
    if (i?.max_players > 0 && i.players >= i.max_players) return toast(t('the session is full ({} players)', [i.players]), 'error');
    let connection;
    try {
      connection = serverJoinSettings(x.address, x.protocol || 'auto', i);
    } catch (error) {
      return toast(t(error.message), 'error');
    }
    selectSession(connection, i, x.name);
    close();
  };
  const joinPublic = async (e) => {
    const id = ++request.current;
    try {
      const i = await call('server_query', { address: e.address });
      if (id !== request.current) return;
      const x = (list || []).find((y) => y.address === e.address) || { name: e.covered ? '' : e.name, address: e.address };
      join(x, i);
    } catch (err) {
      if (id === request.current) toast(t("Can't reach the server: {err}", { err: err.message }), 'error');
    }
  };
  const selectAddress = async () => {
    const a = address.trim();
    if (!a || busy) return setError(t("Type the server's address (an IP, a name or a link)"));
    const id = ++request.current;
    setBusy(true); setError('');
    try {
      const answer = await call('server_query', { address: a });
      if (id === request.current) join({ address: a, name: name.trim(), protocol }, answer);
    } catch (e) {
      if (id === request.current) setError(e.message);
    } finally { if (id === request.current) setBusy(false); }
  };
  return html`<div class="stack servers-page">
    <${PageHead} title="Servers" text="Manage server addresses and lists. Select a server, then choose your bus on Drive." actions=${html`
      <button type="button" class="btn" aria-haspopup="dialog" onClick=${() => { setError(''); setAdding(true); }}><${Icon} name="globe" />${t('Use a server address')}</button>`} />
    <p class="muted small" style="margin:0">${t("Sessions and servers are run by players, not by openOMSI. The host receives your driver name, network address, chat and voice, and may keep logs.")}</p>
    <${Tabs} value=${tab} options=${[['saved', 'Saved servers'], ['public', 'Public servers']]} onChange=${setTab} />
    <div hidden=${tab !== 'public'}><${PublicServers} onJoin=${joinPublic} onAdd=${(e) => { if (list && !list.some((y) => y.address === e.address)) { save([...list, { name: e.covered ? '' : e.name, address: e.address }]); ask(e.address); } }} /></div>
    ${adding && html`<${Dialog} modal title="Use a server address" onClose=${close} class="connection-dialog" footer=${html`
      <${Button} label="Cancel" onClick=${close} />
      <${Button} label="Save to list" disabled=${busy || list == null} onClick=${add} />
      <${Button} kind="primary" label="Select server" disabled=${busy || !address.trim()} onClick=${selectAddress} />`}>
      <div class="stack">
        <div class="field"><label for="server-address">${t('Server address')}</label><${Input} id="server-address" value=${address} placeholder="play.example.org" onInput=${(value) => { request.current++; setBusy(false); setAddress(value); setError(''); }} onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); selectAddress(); } }} /></div>
        <div class="field"><label for="server-name">${t('Name (optional)')}</label><${Input} id="server-name" value=${name} onInput=${setName} /></div>
        <${Field} label="Connection protocol"><${Select} value=${protocol} options=${[['auto', 'Auto'], ['udp', 'UDP'], ['websocket', 'WebSocket']]} onChange=${setProtocol} /></${Field}>
        <p class="muted small">${t('Select a server to prepare your bus, or save its address for later. No game starts yet.')}</p>
        ${busy && html`<div role="status" class="row muted small"><${Spinner} />${t('Asking the server...')}</div>`}
        ${error && html`<p class="connection-error small selectable" role="alert">${error}</p>`}
      </div>
    </${Dialog}>`}
    <div class="stack" hidden=${tab !== 'saved'}>
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
                <div class="row"><b style="font-size:15px">${own || i?.name || x.name || x.address}</b><${Tag}>${t(official ? 'Run by openOMSI' : 'Community server')}</${Tag}>${i?.password && html`<${Tag}>${t('Password')}</${Tag}>`}</div>
                ${st.loading && html`<div class="row muted small"><${Spinner} /> ${t('Asking the server...')}</div>`}
                ${st.err && html`<div class="small" style="color:var(--danger)">${t("Can't reach the server: {err}", { err: st.err })}</div>`}
                ${st.err && !official && html`<div class="muted small selectable">${x.address}</div>`}
                ${i && html`<div class="muted small">${[i.map && mapName(i.map), t('{n} of {max} players', { n: i.players, max: i.max_players }), i.date, i.time, i.weather].filter(Boolean).join(' - ')}</div>`}
                ${i?.motd && html`<div class="dim small">${i.motd}</div>`}
              </div>
              <div style="width:130px"><${Select} raw value=${x.protocol || 'auto'} options=${[['auto', autoLabel], ['udp', t('UDP')], ['websocket', t('WebSocket')]]} onChange=${(protocol) => save(list.map((y) => y === x ? { ...y, protocol } : y))} /></div>
              <${Button} small kind="ghost" icon="refresh" title="Refresh" onClick=${() => ask(x.address)} />
              ${x.address !== 'openomsi' && html`<${Button} small kind="ghost" icon="trash" title="Remove from the list" onClick=${() => save(list.filter((y) => y !== x))} />`}
              <${Button} kind="primary" icon="play" label="Select server" disabled=${!i} onClick=${() => join(x)} />
            </div></${Card}>`;
          })}
    </div>
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
          <div class="row"><b>${e.name}</b><${Tag}>${t('Community server')}</${Tag}></div>
          <div class="muted small">${[e.map && mapName(e.map), t('{n} of {max} players', { n: e.players, max: e.max_players }), e.version].filter(Boolean).join(' - ')}</div>
          ${e.motd && html`<div class="dim small">${e.motd}</div>`}
        </div>
        <${Button} small kind="ghost" icon="plus" title="Add to my servers" onClick=${() => onAdd(e)} />
        <${Button} kind="primary" icon="play" label="Select server" onClick=${() => onJoin(e)} />
      </div>`)}
    </div>
  </${Card}>`;
}
