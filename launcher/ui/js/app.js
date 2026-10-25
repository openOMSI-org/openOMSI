// The launcher's frame: the menu on the left, the page beside it, messages in the corner.

import { html, render, useState, useEffect } from '../vendor/preact-htm.js';
import { useStore, set, S, remembered, remember } from './store.js';
import { t, plainText } from './i18n.js';
import { Icon } from './icons.js';
import { Spinner } from './ui.js';
import { boot, installMod, jobRunning, fitWindow } from './data.js';
import { DrivePage } from './pages/drive.js';
import { DepotPage } from './pages/depot.js';
import { MultiplayerPage } from './pages/multiplayer.js';
import { ProfilePage } from './pages/profile.js';
import { ModsPage } from './pages/mods.js';
import { TroubleshootPage, problems } from './pages/troubleshoot.js';
import { SettingsPage } from './pages/settings.js';
import { ControlsPage } from './pages/controls.js';
import { GamesPage } from './pages/games.js';
import { TutorialsPage } from './pages/tutorials.js';
import { AddonsPage } from './pages/addons.js';
import { TimetablePage } from './pages/timetable.js';
import { SetupPage } from './pages/setup.js';
import { MobileApp } from './mobile/mobile.js';
import { GameEndDialogs, watchGameEnds } from './crash.js';
import { UpdateDialog, startUpdates } from './updates.js';
import { onPhone, onDrop } from './api.js';

// (the phone app, or `?phone` for a look at it in a browser)
const PHONE = onPhone || new URLSearchParams(location.search).has('phone');
if (PHONE) document.documentElement.classList.add('phone');

const GROUPS = [
  ['Play', [['drive', 'Drive', 'bus', DrivePage], ['depot', 'Depot', 'garage', DepotPage], ['multiplayer', 'Multiplayer', 'users', MultiplayerPage], ['games', 'Running games', 'gamepad', GamesPage]]],
  ['Content', [['mods', 'Mods', 'puzzle', ModsPage], ['timetable', 'Timetable', 'clock', TimetablePage], ['addons', 'Add-ons', 'globe', AddonsPage]]],
  ['Options', [['settings', 'Settings', 'sliders', SettingsPage], ['controls', 'Controls', 'keyboard', ControlsPage], ['setup', 'Setup', 'folder', SetupPage]]],
  ['Help', [['tutorials', 'Tutorials', 'book', TutorialsPage], ['troubleshoot', 'Troubleshooting', 'alert', TroubleshootPage]]],
];

const PAGES = [...GROUPS.flatMap(([, pages]) => pages), ['profile', 'Profile', 'user', ProfilePage]];

function Sidebar() {
  const s = useStore();
  const running = s.poll.instances?.length || 0;
  const installing = s.poll.jobs?.filter(jobRunning).length || 0;
  const p = s.profile;
  const name = s.config?.profile || '';
  const [icons, setIcons] = useState(() => remembered('sidebar', window.innerWidth < 1380 ? 'icons' : 'full') === 'icons');
  const flip = () => {
    setIcons(!icons);
    remember('sidebar', icons ? 'full' : 'icons');
  };
  return html`<nav class=${'sidebar' + (icons ? ' icons' : '')}>
    <div class="brand"><img src="logo.svg" alt="" /><div><b>openOMSI</b><small>${t('Launcher')} ${s.boot?.version || ''}</small></div></div>
    ${GROUPS.map(([title, pages]) => html`<div class="nav-group" role="group" aria-label=${t(title)}>
      <div class="nav-title">${t(title)}</div>
      ${pages.map(([id, label, icon]) => {
        const count = id === 'games' ? running : id === 'mods' ? installing : id === 'troubleshoot' ? problems(s).length : 0;
        return html`<button class=${'nav-item' + (s.page === id ? ' active' : '')} onClick=${() => set({ page: id })} title=${t(label)}>
          <${Icon} name=${icon} /><span class="label">${t(label)}</span>${count > 0 && html`<span class="count">${count}</span>`}
        </button>`;
      })}
    </div>`)}
    <div class="nav-fill"></div>
    <button class="nav-item sidebar-toggle" onClick=${flip} title=${t(icons ? 'Expand the menu' : 'Collapse the menu')} aria-expanded=${!icons}>
      <${Icon} name=${icons ? 'right' : 'left'} /><span class="label">${t('Collapse the menu')}</span>
    </button>
    <button class=${'driver-chip' + (s.page === 'profile' ? ' active' : '')} onClick=${() => set({ page: 'profile' })} title=${t('Profile')}>
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
