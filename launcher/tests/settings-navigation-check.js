import { html, render } from '../ui/vendor/preact-htm.js';
import { S, set, remember } from '../ui/js/store.js';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const assert = (ok, message) => { if (!ok) throw new Error(message); };

export async function run(root) {
  const originalTauri = window.__TAURI__;
  const originalState = { ...S };
  const storageKeys = ['settings-tab', 'settings-favourites', 'settings-advanced', 'settings-section-graphics'];
  const originalStorage = storageKeys.map((key) => [key, localStorage.getItem(key)]);
  const report = [];
  const commands = [];
  let saved = await (await fetch('./fixtures/settings_get.json')).json();
  saved = { ...saved, graphics: 'enhanced', ray_tracing: false, fullscreen: false, temporal_upscalers: ['dlss'], upscaler: 'default' };
  const profiles = { 'City drive': { shadows: false, fullscreen: true, msaa: 2 } };
  window.__TAURI__ = { core: { invoke: async (command, payload) => {
    const { cmd, args = {} } = payload || {};
    commands.push([cmd || command, args]);
    if (cmd === 'settings_patch') return saved = { ...saved, ...args.patch };
    if (cmd === 'graphics_profiles') return { ...profiles };
    if (cmd === 'graphics_profile_save') { profiles[args.name] = { msaa: saved.msaa }; return args.name; }
    if (cmd === 'graphics_profile_delete') { delete profiles[args.name]; return true; }
    if (cmd === 'graphics_presets') return [];
    if (cmd === 'nvidia_dlss_status') return { installed: false, supported: false };
    if (cmd === 'optimise_status') return {};
    if (cmd === 'radio_stations') return [];
    if (cmd === 'translations') return {};
    return null;
  } }, event: { listen: async () => () => {} } };
  let mounted = false;
  try {
    remember('settings-tab', 'graphics');
    remember('settings-favourites', '[]');
    remember('settings-advanced', '0');
    remember('settings-section-graphics', 'display');
    const { SettingsPage } = await import('../ui/js/pages/settings.js');
    const { SETTINGS_TABS } = await import('../ui/js/pages/settings-schema.js');
    const mount = async () => {
      set({ boot: { platform: 'windows', version: 'test' }, settings: { ...saved }, config: {}, page: 'settings' });
      render(html`<div class="shell settings-test-shell"><main class="main"><div class="page page-settings"><${SettingsPage} /></div></main></div>`, root);
      mounted = true;
      await sleep(100);
    };
    const click = async (selector) => {
      const el = root.querySelector(selector);
      assert(el, 'Missing control: ' + selector);
      el.click();
      await sleep(80);
    };
    await mount();
    assert(root.querySelector('.settings-categories'), 'Settings need persistent category navigation');
    assert(root.querySelector('[data-settings-group="display"]'), 'Display must be a short subpage');
    assert(root.querySelector('[data-settings-group="profiles"]'), 'Graphics profiles must remain directly visible');
    report.push('PASS persistent categories and graphics profile subpage');
    assert(!root.querySelector('.settings-help-hint'), 'Do not repeat the sidebar help instruction above the list');
    assert(root.querySelector('#settings-description').textContent.includes('Select Help beside a setting'), 'Keep the initial help instruction in the sidebar');
    assert(!root.querySelector('.settings-advanced') && !root.querySelector('.settings-reveal'), 'All options must be visible without an advanced switch');
    assert(root.querySelector('[data-setting-key="sim_thread"]'), 'Previously advanced display options must be visible even with the old preference disabled');
    const beforeHelp = commands.filter(([cmd]) => cmd === 'settings_patch').length;
    await click('[data-setting-key="fullscreen"] .settings-help');
    const helpRow = root.querySelector('[data-setting-key="fullscreen"]');
    assert(helpRow.querySelector('.settings-help').getAttribute('aria-label').includes('Fullscreen'), 'Help button must name its setting');
    assert(root.querySelector('#settings-description h3').textContent === 'Fullscreen', 'Help must select the setting in the description panel');
    assert(helpRow.querySelector('.settings-inline-help'), 'Small screens need an explanation beside the setting');
    if (innerWidth <= 1250) assert(getComputedStyle(helpRow.querySelector('.settings-inline-help')).display !== 'none', 'Help must stay visible when the sidebar is hidden');
    assert(commands.filter(([cmd]) => cmd === 'settings_patch').length === beforeHelp, 'Viewing help must not change a setting');
    report.push('PASS discoverable help buttons and responsive explanations without changing settings');

    const { settingsGroups, readFavourites } = await import('../ui/js/pages/settings-layout.js');
    for (const tab of SETTINGS_TABS) {
      const source = tab.cols.flatMap((col) => col.rows);
      const grouped = settingsGroups(tab).flatMap((group) => group.rows);
      assert(grouped.length === source.length && source.every((row) => grouped.filter((item) => item === row).length === 1), 'Lost or duplicated schema rows in ' + tab.id);
    }
    assert(readFavourites('bad JSON').size === 0 && readFavourites('{}').size === 0, 'Broken saved favourites must not crash settings');
    const extra = { type: 'toggle', key: 'future_setting', label: 'A future setting' };
    assert(settingsGroups({ id: 'graphics', cols: [{ title: 'Graphics', rows: [extra] }] }).some((group) => group.rows.includes(extra)), 'New schema entries need a visible fallback');
    report.push('PASS every original row is retained, future rows have a fallback and saved favourites are validated');

    await click('[data-settings-group="profiles"]');
    assert(root.textContent.includes('Save current graphics as profile'), 'Profile saving disappeared');
    assert(root.textContent.includes('City drive'), 'Existing saved profiles must load');
    assert(root.querySelector('.settings-profile-saved') && root.querySelector('.settings-profile-save'), 'Saved profiles and saving current graphics must be separate sections');
    const findButton = (label) => [...root.querySelectorAll('button')].find((button) => button.textContent.trim() === label);
    assert(findButton('Load') && findButton('Delete'), 'Profile load/delete controls disappeared');
    findButton('Load').click();
    await sleep(120);
    const patch = commands.find(([cmd]) => cmd === 'settings_patch')?.[1].patch;
    assert(patch?.msaa === 2 && patch.shadows === false && !('fullscreen' in patch), 'Profiles must preserve their allowlist');
    const nameInput = root.querySelector('input[maxlength="40"]');
    nameInput.value = 'Rain drive';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(40);
    findButton('Save current graphics as profile').click();
    await sleep(120);
    assert(commands.some(([cmd, args]) => cmd === 'graphics_profile_save' && args.name === 'Rain drive'), 'Profile saving must call the original command');
    assert(root.textContent.includes('Rain drive'), 'The new profile must remain selected');
    findButton('Delete').click();
    await sleep(120);
    assert(commands.some(([cmd, args]) => cmd === 'graphics_profile_delete' && args.name === 'Rain drive'), 'Profile deletion must call the original command');
    report.push('PASS real profile save/load/delete and safe profile loading');

    const search = root.querySelector('.settings-search input');
    search.value = 'profiles'; search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(100);
    assert(root.textContent.includes('Save current graphics as profile'), 'Search must include the custom profiles block');
    search.value = 'shadow_casters'; search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(100);
    assert(root.querySelector('[data-setting-key="shadow_casters"]'), 'Search must include all settings');
    search.value = 'upscaler'; search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(80);
    const upscaler = root.querySelector('[data-setting-key="upscaler"]');
    assert(upscaler && !upscaler.querySelector('.name span'), 'Do not repeat help below the setting name');
    await click('[data-setting-key="upscaler"] .settings-help');
    assert(root.querySelector('#settings-description p').textContent.includes('Standard Catmull-Rom'), 'Keep functional help in the description panel');
    search.value = 'traffic_headlights'; search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(80);
    const headlight = root.querySelector('[data-setting-key="traffic_headlights"]');
    assert(headlight && !headlight.querySelector('.slider .help'), 'Slider descriptions must also use the help button');
    report.push('PASS compact controls retain their descriptions through Help');
    search.value = ''; search.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(80);
    await click('[data-settings-group="display"]');
    await click('[data-setting-key="fullscreen"] .settings-pin');
    await click('[data-settings-category="favourites"]');
    assert(root.querySelector('[data-setting-key="fullscreen"]'), 'Favourites must contain the pinned setting');
    render(null, root); mounted = false;
    await mount();
    await click('[data-settings-category="favourites"]');
    assert(root.querySelector('[data-setting-key="fullscreen"]'), 'Favourites must survive reopening settings');
    await click('[data-setting-key="fullscreen"] .toggle');
    await sleep(100);
    assert(commands.some(([cmd, args]) => cmd === 'settings_patch' && args.patch.fullscreen === true), 'Real controls must still save changes');
    report.push('PASS global search, persistent favourites and setting writes');

    await click('[data-settings-category="graphics"]');
    await click('[data-settings-group="quality"]');
    set({ settings: { ...S.settings, upscaler: 'dlss' } });
    await sleep(100);
    assert(!root.querySelector('[data-setting-key="msaa"]'), 'Temporal upscaling must still hide MSAA');
    report.push('PASS original conditional visibility');
    for (const theme of ['dark', 'light']) {
      document.documentElement.dataset.theme = theme;
      assert(root.querySelector('.settings-body').getBoundingClientRect().width > 0, 'Settings content is hidden in ' + theme);
      assert(document.documentElement.scrollWidth <= innerWidth + 1, 'Horizontal page overflow in ' + theme);
    }
    report.push('PASS light/dark layout without horizontal page overflow');
    return report.join('\n');
  } finally {
    if (mounted) render(null, root);
    window.__TAURI__ = originalTauri;
    for (const [key, value] of originalStorage) value == null ? localStorage.removeItem(key) : localStorage.setItem(key, value);
    set(originalState);
  }
}
