import { html, useState } from '../../vendor/preact-htm.js';
import { useStore, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { choose, mapName } from '../data.js';
import { Button, Notice } from '../ui.js';
import { Icon } from '../icons.js';
import { JoinSessionDialog } from './join-session.js';
import { cancelSession } from './session-selection.js';

export function DriveMode() {
  const s = useStore();
  const [joining, setJoining] = useState(false);
  const mode = s.choice.lan_mode;
  const session = (s.poll.instances || []).filter((i) => i.running !== false)
    .map((i) => i.lan_status).find((x) => x?.role === 'host');
  // (a running game that was to host and could not: its status says why - without it the page
  // only went on waiting for a code that never came)
  const failed = !session?.code && (s.poll.instances || []).filter((i) => i.running !== false)
    .map((i) => i.lan_status).find((x) => x?.role === 'none' && x.error)?.error;
  const remoteMap = s.choice.lan_map;
  const mapMissing = mode === 'join' && remoteMap && Array.isArray(s.maps)
    && !s.maps.some((m) => m.file.replace(/\\/g, '/').toLowerCase() === remoteMap.replace(/\\/g, '/').toLowerCase());
  const select = (value) => {
    if (value === mode) return;
    choose({ lan_mode: value });
    if (value === 'host') call('prepare_tunnel').catch(() => toast(t('Internet preparation could not be started. You can still set up a LAN game.'), 'error'));
  };
  const copy = () => navigator.clipboard.writeText(session.code)
    .then(() => toast(t('Session code copied'), 'ok'), (e) => toast(e.message, 'error'));
  return html`<section class="drive-mode multiplayer-duty-notice" aria-label=${t('Play mode')}>
    ${mode === 'join' ? html`<${Notice} icon="users"><div class="row wrap"><div class="grow">
      <b>${s.choice.lan_server?.name || t('Choose a bus and start')}</b>
      <div class="muted small">${t('The session is selected, not connected yet. Choose your bus and press Join and drive.')}</div>
      ${mapMissing && html`<div class="muted small">${t('The server plays {}, which is not installed here: it is fetched from the server on joining.', [mapName(remoteMap)])}</div>`}
    </div><${Button} small kind="ghost" label="Cancel" onClick=${cancelSession} /></div></${Notice}>` : html`
      <div class="drive-mode-title small muted">${t('Play mode')}</div>
      <div class="drive-mode-toolbar">
        <div class="drive-mode-options" role="radiogroup" aria-label=${t('Play mode')}>
          ${[['off', 'Solo driving', 'Drive on your own without creating a room.', 'user'], ['host', 'Invite friends', 'Host your selected map, bus and duty. Share the code after starting.', 'users']].map(([value, label, hint, icon]) => html`
            <label title=${t(hint)} class=${'drive-mode-option' + ((mode || 'off') === value ? ' selected' : '')}>
              <input type="radio" name="drive-play-mode" value=${value} checked=${(mode || 'off') === value} onChange=${() => select(value)} />
              <${Icon} name=${icon} size=${18} /><strong>${t(label)}</strong>
            </label>`)}
        </div>
        <span class="drive-mode-divider" aria-hidden="true"></span>
        <button type="button" class="btn drive-mode-join" aria-haspopup="dialog" onClick=${() => setJoining(true)}>
          <${Icon} name="users" size=${18} /><strong>${t('Join friends')}</strong><${Icon} name="external" size=${16} />
        </button>
      </div>`}
    ${(mode === 'host' || session) && html`<${Notice} icon="users">${t("As the host, your game receives your players' driver names, network addresses, chat and voice. You are responsible for what you keep of it, such as logs and bans.")}</${Notice}>`}
    ${mode !== 'join' && session?.code && html`<div class="drive-session">
      <div class="drive-session-info"><span class="small">${t('Running hosted session')}</span>
        <span class="drive-session-code selectable" title=${session.code}>${session.code}</span>
        <span class="small muted">${t(session.tunnel ? 'Friends can join from anywhere' : 'Getting ready for friends on the internet...')}</span></div>
      <${Button} small icon="copy" label="Copy code" onClick=${copy} /></div>`}
    ${mode === 'host' && failed && html`<${Notice} kind="danger"><b>${t('The game could not open a session')}</b><div class="muted small">${t('Another program may use the port, or a firewall or antivirus blocks it. Close the game and start the duty again.')}</div><details><summary>${t('Technical details')}</summary><div class="small selectable">${failed}</div></details></${Notice}>`}
    ${mode === 'host' && !session?.code && !failed && html`<p class="drive-mode-hint small muted">${t('Start your hosted duty first. A session code appears only after the game starts.')}</p>`}
    ${joining && html`<${JoinSessionDialog} onClose=${() => setJoining(false)} />`}
  </section>`;
}
