import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { useStore } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Button, Dialog, Input, Notice, Spinner } from '../ui.js';
import { selectSession } from './session-selection.js';

export function JoinSessionDialog({ onClose }) {
  const s = useStore();
  const [code, setCode] = useState(s.choice.lan_mode === 'join' && /^OMSI-/i.test(s.choice.lan_addr) ? s.choice.lan_addr : '');
  const [result, setResult] = useState(null);
  const request = useRef(0);
  useEffect(() => () => { request.current++; }, []);
  const edit = (text) => { request.current++; setCode(text); setResult(null); };
  const current = result?.code === code.trim() ? result : null;
  const select = (address, host) => {
    request.current++;
    selectSession({ lan_addr: address, lan_ws_fallback: null }, host);
    onClose();
  };
  const confirm = async () => {
    const asked = code.trim();
    if (!asked || current?.busy) return;
    const id = ++request.current;
    if (!/^OMSI-/i.test(asked)) return setResult({ code: asked, error: t('Paste the complete session code from your friend.') });
    setResult({ code: asked, busy: true });
    try {
      const check = await call('check_join', { text: asked });
      if (id !== request.current) return;
      if (!check.ok) return setResult({ code: asked, error: check.text });
      setResult({ code: asked, busy: true, validated: true });
      try {
        const host = await call('server_query', { address: asked });
        if (id !== request.current) return;
        if (host.max_players > 0 && host.players >= host.max_players) return setResult({ code: asked, full: true });
        select(asked, host);
      } catch (e) {
        // A valid UDP session can work even if its status gateway is unavailable.
        if (id === request.current) setResult({ code: asked, validated: true, unavailable: e.message });
      }
    } catch (e) {
      if (id === request.current) setResult({ code: asked, error: e.message });
    }
  };
  return html`<${Dialog} modal title="Join friends" onClose=${onClose} class="connection-dialog">
    <div class="stack">
      <p class="muted small">${t('Confirm the session first, then choose your bus on the Drive page. The game has not connected yet.')}</p>
      <div class="field"><label for="multiplayer-code">${t('Session code')}</label>
        <div class="row multiplayer-code-row">
          <${Input} id="multiplayer-code" value=${code} placeholder="OMSI-XXXX-XXXX-..." onInput=${edit} aria-invalid=${!!current?.error}
            aria-describedby="multiplayer-code-help multiplayer-code-status" onKeyDown=${(e) => { if (e.key === 'Enter') { e.preventDefault(); confirm(); } }} />
          <${Button} kind="primary" label="Confirm session" icon="search" disabled=${!code.trim() || current?.busy} onClick=${confirm} />
        </div>
        <span class="help" id="multiplayer-code-help">${t('Paste the complete code from your friend. Leading and trailing spaces are removed.')}</span>
      </div>
      <div id="multiplayer-code-status" role="status" aria-live="polite">
        ${current?.busy && html`<div class="row muted small"><${Spinner} />${t('Checking the session...')}</div>`}
        ${current?.error && html`<${Notice} kind="danger"><b>${t('Could not validate the code')}</b><div class="muted small selectable">${current.error}</div></${Notice}>`}
        ${current?.full && html`<${Notice} kind="warn">${t('The session is full. Ask your friend to make room, then confirm again.')}</${Notice}>`}
        ${current?.unavailable && html`<${Notice} kind="warn"><b>${t('Could not read the session details')}</b><div class="muted small">${t('Ask your friend to keep the game running and send the latest code. Retry, or continue without session details.')}</div><details class="multiplayer-error-details"><summary>${t('Technical details')}</summary><div class="small selectable">${current.unavailable}</div></details></${Notice}>`}
      </div>
      ${current?.validated && !current.full && html`<div><${Button} label="Choose a bus without session details" onClick=${() => select(current.code, null)} /></div>`}
      <${Notice} icon="users"><p class="muted small">${t("Sessions and servers are run by players, not by openOMSI. The host receives your driver name, network address, chat and voice, and may keep logs.")}</p></${Notice}>
      <details class="multiplayer-help-panel"><summary>${t('First time here?')}</summary><p class="muted small">${t('Ask your friend to start a hosted duty and copy the session code.')}</p></details>
    </div>
  </${Dialog}>`;
}
