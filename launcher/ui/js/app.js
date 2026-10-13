// The launcher's frame: the menu on the left, the page beside it, messages in the corner.

import { html, render, useState, useEffect } from '../vendor/preact-htm.js';
import { useStore, set, S } from './store.js';
import { t, plainText } from './i18n.js';
import { Icon } from './icons.js';
import { Spinner } from './ui.js';
import { boot, installMod, jobRunning, fitWindow } from './data.js';
import { DrivePage } from './pages/drive.js';
import { MultiplayerPage } from './pages/multiplayer.js';
import { ProfilePage } from './pages/profile.js';
import { ModsPage } from './pages/mods.js';
import { SettingsPage } from './pages/settings.js';
import { ControlsPage } from './pages/controls.js';
import { GamesPage } from './pages/games.js';
import { TutorialsPage } from './pages/tutorials.js';
import { TimetablePage } from './pages/timetable.js';
import { SetupPage } from './pages/setup.js';
import { MobileApp } from './mobile/mobile.js';
import { GameEndDialogs, watchGameEnds } from './crash.js';
import { UpdateDialog, startUpdates } from './updates.js';
import { onPhone, onDrop } from './api.js';

// (the phone app, or `?phone` for a look at it in a browser)
const PHONE = onPhone || new URLSearchParams(location.search).has('phone');
if (PHONE) document.documentElement.classList.add('phone');

const PAGES = [
  ['drive', 'Drive', 'bus', DrivePage],
  ['multiplayer', 'Multiplayer', 'users', MultiplayerPage],
  ['profile', 'Profile', 'user', ProfilePage],
  ['mods', 'Mods', 'puzzle', ModsPage],
  ['settings', 'Settings', 'sliders', SettingsPage],
  ['controls', 'Controls', 'keyboard', ControlsPage],
  null,
  ['games', 'Running games', 'gamepad', GamesPage],
  ['tutorials', 'Tutorials', 'book', TutorialsPage],
  ['timetable', 'Timetable', 'clock', TimetablePage],
  ['setup', 'Setup', 'folder', SetupPage],
];

function Sidebar() {
  const s = useStore();
  const running = s.poll.instances?.length || 0;
  const installing = s.poll.jobs?.filter(jobRunning).length || 0;
  const p = s.profile;
  const name = s.config?.profile || '';
  return html`<nav class="sidebar">
    <div class="brand"><img src="logo.svg" alt="" /><div><b>openOMSI</b><small>${t('Launcher')} ${s.boot?.version || ''}</small></div></div>
    ${PAGES.map((x) => {
      if (!x) return html`<div class="nav-sep"></div>`;
      const [id, label, icon] = x;
      const count = id === 'games' ? running : id === 'mods' ? installing : 0;
      return html`<button class=${'nav-item' + (s.page === id ? ' active' : '')} onClick=${() => set({ page: id })} title=${t(label)}>
        <${Icon} name=${icon} /><span class="label">${t(label)}</span>${count > 0 && html`<span class="count">${count}</span>`}
      </button>`;
    })}
    <div class="nav-fill"></div>
    <button class="driver-chip" onClick=${() => set({ page: 'profile' })} title=${t('Profile')}>
      <span class="avatar">${(name[0] || '?').toUpperCase()}</span>
      <span class="who"><b>${name || t('No driver')}</b><span>${p ? t('Level {n}', { n: p.level }) : t('Choose a driver')}</span></span>
    </button>
  </nav>`;
}

function Toasts() {
  const s = useStore();
  return html`<div class="toasts">${s.toasts.map((x) => html`<div class=${'toast ' + x.kind}><${Icon} name=${x.kind === 'error' ? 'alert' : x.kind === 'ok' ? 'check' : 'info'} /><div class="selectable">${x.text}</div></div>`)}</div>`;
}

function useFocused() {
  const [focused, setFocused] = useState(() => document.hasFocus());
  useEffect(() => {
    const on = () => setFocused(true);
    const off = () => setFocused(false);
    addEventListener('focus', on);
    addEventListener('blur', off);
    return () => {
      removeEventListener('focus', on);
      removeEventListener('blur', off);
    };
  }, []);
  return focused;
}

function App() {
  const s = useStore();
  const focused = useFocused();
  if (!s.ready) {
    return html`<div class="splash">${s.errors.boot ? html`<div class="notice danger" style="max-width:520px">${s.errors.boot}</div>` : html`<${Spinner} />`}</div>`;
  }
  if (PHONE) return html`<${MobileApp} /><${GameEndDialogs} /><${UpdateDialog} /><${Toasts} />`;
  const page = PAGES.find((x) => x && x[0] === s.page) || PAGES[0];
  const Page = page[3];
  const resting = !s.boot?.selftest && s.settings?.launcher_rest !== false && s.poll.instances?.length > 0 && !focused;
  return html`<div class="shell">
    <${Sidebar} />
    <main class="main">
      <div class=${'page page-' + page[0]} key=${s.page} hidden=${resting}><${Page} /></div>
      ${resting && html`<div class="splash resting"><${Icon} name="bus" size=${40} /><p>${t('The launcher rests while you drive, so that the game has the graphics card to itself. It is back as soon as the game ends.')}</p></div>`}
    </main>
    <${GameEndDialogs} />
    <${UpdateDialog} />
    <${Toasts} />
  </div>`;
}

plainText(document.body);
fitWindow();
let fitting = 0;
addEventListener('resize', () => {
  clearTimeout(fitting);
  fitting = setTimeout(fitWindow, 100);
});
render(html`<${App} />`, document.getElementById('app'));
watchGameEnds();
onDrop(
  (paths) => {
    if (!S.ready || S.page === 'setup') return;
    set({ page: 'mods' });
    paths.forEach(installMod);
  },
  (over) => over !== S.dropOver && set({ dropOver: over }),
);
boot()
  .then(() => (S.boot?.selftest ? import('./selftest.js').then((m) => m.selftest()) : startUpdates()))
  .catch((e) => set({ errors: { ...S.errors, boot: String(e.message || e) } }));

// (the page follows the system's light or dark look when asked to)
matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => import('./data.js').then((d) => d.applyTheme()));
