// Multiplayer: hosting the next duty, joining a friend's session by its code, and servers
// that are always on.

import { html, useState, useEffect } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Field, Input, Empty, PageHead, Spinner, Tabs, Toggle, Notice, Tag } from '../ui.js';
import { choose, cached } from '../data.js';

export function MultiplayerPage() {
  const [tab, setTab] = useState('code');
  return html`
    <${PageHead} title="Multiplayer" text="Drive with friends by a code, or on servers that are always on." />
    <${Tabs} value=${tab} options=${[['code', 'Connect by Code'], ['servers', 'Servers']]} onChange=${setTab} />
    ${tab === 'code' ? html`<${ByCode} />` : html`<${Servers} />`}
  `;
}

function ByCode() {
  const s = useStore();
  const c = s.choice;
  const hosting = c.lan_mode === 'host';
  const session = (s.poll.instances || []).filter((i) => i.running !== false).map((i) => i.lan_status).find((x) => x?.role === 'host');
  const [code, setCode] = useState(c.lan_mode === 'join' ? c.lan_addr : '');
  const [check, setCheck] = useState(null);
  const [host, setHost] = useState(null);
  useEffect(() => {
    setHost(null);
    if (!code.trim()) return setCheck(null);
    const asked = code.trim();
    const id = setTimeout(() => {
      call('check_join', { text: code }).then(setCheck).catch(() => setCheck(null));
      if (/^OMSI-/i.test(asked)) call('server_query', { address: asked }).then((h) => setHost({ ...h, code: asked })).catch(() => setHost(null));
    }, 300);
    return () => clearTimeout(id);
  }, [code]);
  const join = () => {
    if (!code.trim()) return toast(t('Paste a session code first'), 'error');
    const known = host?.code === code.trim() ? host : null;
    if (known?.map && S.maps?.some((m) => m.file.toLowerCase() === known.map.replace(/\\/g, '/').toLowerCase())) choose({ map: known.map.replace(/\\/g, '/') });
    choose({ lan_mode: 'join', lan_addr: code.trim(), lan_map: known?.map || '' });
    set({ page: 'drive' });
  };
  return html`<div class="grid-2" style="align-items:start">
    <${Card} title="Host a game">
      <div class="stack">
        <p class="muted">${t('Your next duty opens a session. Give the code to your friends: it works at home and over the internet.')}</p>
        <${Toggle} checked=${hosting} label="Host my next duty" onChange=${(on) => {
          choose({ lan_mode: on ? 'host' : 'off' });
          if (on) call('prepare_tunnel').catch(() => {});
        }} />
        ${session
          ? html`<div class="field"><span class="label">${t('Session code')}</span>
              <div class="row"><code class="input selectable" style="display:flex;align-items:center">${session.code}</code>
              <${Button} icon="copy" title="Copy the code" onClick=${() => navigator.clipboard.writeText(session.code).then(() => toast(t('Session code copied'), 'ok'))} /></div>
              <span class="help">${t(session.tunnel ? 'Friends can join from anywhere' : 'Getting ready for friends on the internet...')}</span></div>`
          : html`<p class="dim small">${t(hosting ? 'Start a duty on the Drive page: the code appears here.' : 'Turn on hosting, then start a duty on the Drive page.')}</p>`}
        <div><${Button} icon="bus" label="Go to Drive" onClick=${() => set({ page: 'drive' })} /></div>
      </div>
    </${Card}>
    <${Card} title="Connect by Code">
      <div class="stack">
        <p class="muted">${t("Paste the code your friend's game shows. The map, time and weather are the host's; you choose your bus.")}</p>
        <${Input} value=${code} placeholder="OMSI-XXXX-XXXX-..." onInput=${setCode} />
        ${check && html`<div class=${'small ' + (check.ok ? 'dim' : '')} style=${check.ok ? '' : 'color:var(--danger)'}>${check.text}</div>`}
        ${host && html`<${Notice} icon="users"><b>${host.name}</b><div class="muted small">${host.map} - ${t('{n} of {max} players', { n: host.players, max: host.max_players })}</div></${Notice}>`}
        <div><${Button} kind="primary" icon="play" label="Choose a bus and join" onClick=${join} /></div>
      </div>
    </${Card}>
  </div>`;
}

function Servers() {
  const s = useStore();
  const [list, setList] = useState(null);
  const [info, setInfo] = useState({});
  const [address, setAddress] = useState('');
  const [name, setName] = useState('');
  const ask = (a) => {
    setInfo((x) => ({ ...x, [a]: { loading: true } }));
    call('server_query', { address: a })
      .then((r) => setInfo((x) => ({ ...x, [a]: { ok: r } })))
      .catch((e) => setInfo((x) => ({ ...x, [a]: { err: e.message } })));
  };
  useEffect(() => {
    cached('servers', 'servers_get').then((l) => {
      if (!Array.isArray(l)) return;
      setList(l);
      l.forEach((x) => ask(x.address));
    });
  }, []);
  const save = (l) => {
    setList(l);
    set({ cache: { ...S.cache, servers: l } });
    call('servers_save', { list: l }).catch((e) => toast(e.message, 'error'));
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
  const join = (x) => {
    const i = info[x.address]?.ok;
    if (!i) return toast(t('The server has not answered yet (is its address right? is it running?)'), 'error');
    const map = (i.map || '').replace(/\\/g, '/');
    if (map && s.maps?.some((m) => m.file.toLowerCase() === map.toLowerCase())) choose({ map });
    else if (map) toast(t('The server plays {}, which is not installed here: it is fetched from the server on joining.', [map]));
    const web = /^(https?|wss?):\/\//.test(x.address) || x.address === 'openomsi';
    choose({ lan_mode: 'join', lan_addr: !web && i.reached_at ? i.reached_at : x.address, lan_map: i.map || '' });
    toast(t('Joined {} - choose your bus and duty, then Start the duty', [i.name || x.name]), 'ok');
    set({ page: 'drive' });
  };
  return html`<div class="stack">
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
            return html`<${Card} body=${false}><div class="card-body row" style="align-items:flex-start;gap:14px">
              ${i?.icon ? html`<img src=${i.icon} width="48" height="48" style="border-radius:8px" />` : html`<div class="avatar" style="width:48px;height:48px;border-radius:8px"><${Icon} name="globe" /></div>`}
              <div class="grow stack" style="gap:4px;min-width:0">
                <div class="row"><b style="font-size:15px">${i?.name || x.name}</b>${i?.password && html`<${Tag}>${t('Password')}</${Tag}>`}</div>
                ${st.loading && html`<div class="row muted small"><${Spinner} /> ${t('Asking the server...')}</div>`}
                ${st.err && html`<div class="small" style="color:var(--danger)">${t("Can't reach the server: {err}", { err: st.err })}</div>`}
                ${i && html`<div class="muted small">${[i.map, t('{n} of {max} players', { n: i.players, max: i.max_players }), i.time, i.weather].filter(Boolean).join(' - ')}</div>`}
                ${i?.motd && html`<div class="dim small">${i.motd}</div>`}
              </div>
              <${Button} small kind="ghost" icon="refresh" title="Refresh" onClick=${() => ask(x.address)} />
              ${x.address !== 'openomsi' && html`<${Button} small kind="ghost" icon="trash" title="Remove from the list" onClick=${() => save(list.filter((y) => y !== x))} />`}
              <${Button} kind="primary" icon="play" label="Join" disabled=${!i} onClick=${() => join(x)} />
            </div></${Card}>`;
          })}
  </div>`;
}
