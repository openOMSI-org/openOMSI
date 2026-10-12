import { html } from '../vendor/preact-htm.js';
import { S, set, toast, useStore } from './store.js';
import { t } from './i18n.js';
import { call, on, onPhone } from './api.js';
import { Button, Dialog, Toggle } from './ui.js';
import { saveSettings } from './data.js';

const BUSY = ['checking', 'downloading', 'installing', 'installer', 'restarting'];
const mb = (n) => `${((n || 0) / 1000000).toFixed(1)} MB`;
const playing = () => !!S.poll?.instances?.length || (onPhone && !!S.gameStarted);
const looking = () => S.settings?.update_check !== false;
const putAside = (u) => u.dismissed || u.release?.version === u.dismissed_version;
const autoStarted = new Set();
let timer = 0;
let relaunching = false;
let relaunchError = '';

async function refresh() {
  const u = await call('update_status').catch(() => null);
  if (!u) return;
  clearTimeout(timer);
  if (relaunchError && u.state === 'restarting') return set({ update: { ...u, state: 'failed', message: relaunchError } });
  set({ update: u });
  if (BUSY.includes(u.state)) timer = setTimeout(refresh, 700);
  if (u.state === 'available' && S.settings?.update_auto && !playing() && !putAside(u) && !autoStarted.has(u.release.version)) {
    autoStarted.add(u.release.version);
    installUpdate();
  }
  if (u.state === 'restarting' && !relaunching && !onPhone) {
    relaunching = true;
    call('update_relaunch').catch((e) => {
      relaunchError = t('openOMSI {version} is installed: start it again yourself ({e}).', { version: u.release.version, e: e.message });
      set({ updateAsked: true, update: { ...u, state: 'failed', message: relaunchError } });
    });
  }
}

export async function checkUpdates(again = false) {
  if (again) set({ updateAsked: true });
  set({ update: await call('update_check', { again }).catch(() => S.update) });
  refresh();
}

export async function installUpdate() {
  if (playing()) return;
  set({ updateAsked: true, update: await call('update_install').catch(() => S.update) });
  refresh();
}

async function dismissUpdate() {
  set({ updateAsked: false, update: await call('update_dismiss').catch(() => S.update) });
}

export function startUpdates() {
  if (S.boot?.updated) toast(t('Updated to openOMSI {version}', { version: S.boot.updated }), 'ok');
  refresh();
  setTimeout(() => looking() && checkUpdates(), 1000);
  setInterval(() => {
    const st = S.update?.state;
    if (looking() && !playing() && (st === 'idle' || st === 'up_to_date' || (st === 'failed' && S.update.dismissed))) checkUpdates();
  }, 30 * 60 * 1000);
  on('games-ended', () => {
    autoStarted.clear();
    setTimeout(() => looking() && !playing() && checkUpdates(true), 5000);
  });
}

export function UpdateDialog() {
  const s = useStore();
  const u = s.update;
  if (!u || playing()) return null;
  const r = u.release;
  const shown = u.state === 'available' ? !putAside(u) : u.state === 'failed' ? s.updateAsked && !u.dismissed : BUSY.includes(u.state) && u.state !== 'checking';
  if (!shown) return null;
  const busy = u.state !== 'available' && u.state !== 'failed';
  const open = (url) => call('open_url', { url }).catch((e) => toast(e.message, 'error'));
  const title = u.state === 'available' ? t('openOMSI {version} is available', { version: r.version })
    : u.state === 'downloading' ? t('Downloading openOMSI {version}', { version: r.version })
    : u.state === 'failed' ? t('Not updated')
    : t('Installing openOMSI {version}', { version: r.version });
  const footer = u.state === 'available'
    ? html`<${Button} kind="ghost" icon="external" label="What's new" onClick=${() => open(r.page)} />
        <${Button} label="Not now" onClick=${dismissUpdate} />
        <${Button} kind="primary" icon="download" label="Update now" onClick=${installUpdate} />`
    : u.state === 'failed'
      ? html`<${Button} kind="ghost" icon="external" label="Open on GitHub" onClick=${() => open(`${u.repo}/releases/latest`)} />
          <${Button} icon="refresh" label="Try again" onClick=${() => checkUpdates(true)} />
          <${Button} label="Close" onClick=${dismissUpdate} />`
      : null;
  return html`<${Dialog} title=${title} onClose=${busy ? null : dismissUpdate} footer=${footer}>
    <div class="stack" style="gap:14px">
      ${u.state === 'available' && html`
        <p class="dim">${t(onPhone
          ? 'You have {current}. The launcher downloads the new version ({size}) from GitHub and Android installs it; openOMSI then starts again. Your mods and settings stay as they are.'
          : 'You have {current}. The launcher downloads the new version ({size}) from GitHub, puts it in place of this one and starts again. Your mods and settings stay as they are.', { current: u.current, size: mb(r.size) })}</p>
        <${Toggle} checked=${!!s.settings?.update_auto} label="Install updates without asking from now on" onChange=${(v) => saveSettings({ update_auto: v })} />`}
      ${u.state === 'downloading' && html`
        <p class="dim">${t('{done} of {total} from GitHub', { done: mb(u.done), total: mb(u.total) })}</p>
        <div class="progress"><div style=${`width:${u.total ? (100 * u.done) / u.total : 0}%`}></div></div>`}
      ${(u.state === 'installing' || u.state === 'restarting') && html`
        <p class="dim">${t('The new version is put in place; the launcher starts again in a moment.')}</p>
        <div class="progress"><div style="width:100%"></div></div>`}
      ${u.state === 'installer' && html`<p class="dim">${t('Android asks whether to update openOMSI: press Update there. The app then starts again by itself.')}</p>`}
      ${u.state === 'failed' && html`<p class="dim selectable">${u.message}</p>`}
    </div>
  </${Dialog}>`;
}

export function UpdateRow() {
  const s = useStore();
  const u = s.update;
  if (!u?.allowed) return null;
  const line = u.state === 'checking' ? t('Looking for updates…')
    : u.state === 'up_to_date' ? t('openOMSI {version} is the latest version.', { version: u.current })
    : u.state === 'available' ? t('openOMSI {version} is available', { version: u.release.version })
    : u.state === 'downloading' ? `${t('Downloading openOMSI {version}', { version: u.release.version })} (${t('{done} of {total} from GitHub', { done: mb(u.done), total: mb(u.total) })})`
    : u.state === 'installing' || u.state === 'installer' || u.state === 'restarting' ? t('Installing openOMSI {version}', { version: u.release.version })
    : u.state === 'failed' ? u.message
    : '';
  return html`<div class="setting-row full">
    <div class="row wrap">
      <span class=${'small grow' + (u.state === 'failed' ? '' : ' muted')}>${line}</span>
      ${u.state === 'available'
        ? html`<${Button} small kind="primary" icon="download" label="Update now" disabled=${playing()} onClick=${installUpdate} />`
        : html`<${Button} small icon="refresh" label="Check now" disabled=${BUSY.includes(u.state)} onClick=${() => checkUpdates(true)} />`}
    </div>
  </div>`;
}
