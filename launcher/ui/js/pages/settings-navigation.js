import { html, useState, useEffect, useRef } from '../../vendor/preact-htm.js';
import { useStore, remembered, remember, set } from '../store.js';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';
import { Search, useConfirm } from '../ui.js';
import { SETTINGS_TABS } from './settings-schema.js';
import { settingsGroups, settingId, settingLabel, isSettingVisible, isSettingControl, readFavourites } from './settings-layout.js';
import { AiListsPage } from './ai-lists.js';

const icons = { graphics: 'sliders', driving: 'gamepad', camera: 'user', sound: 'play', gameplay: 'bus', general: 'sliders', ailists: 'traffic' };
const allGroups = new Map(SETTINGS_TABS.map((tab) => [tab.id, settingsGroups(tab)]));
const entries = SETTINGS_TABS.flatMap((tab) => allGroups.get(tab.id).flatMap((group) => group.rows.map((row, i) => ({ tab, group, row, id: settingId(tab, row), key: tab.id + ':' + group.id + ':' + i }))));
const fold = (text) => String(text || '').normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();

function matches(entry, words, settings) {
  const { row, tab, group } = entry;
  const help = typeof row.help === 'function' ? row.help(settings) : row.help;
  const texts = [settingLabel(row), help, tab.label, group.label];
  const text = fold([...texts, ...texts.filter(Boolean).map((text) => t(text)), typeof row.key === 'string' && row.key].join(' '));
  return words.every((word) => text.includes(word));
}

function settingHelp(entry, settings) {
  const help = typeof entry.row.help === 'function' ? entry.row.help(settings) : entry.row.help;
  if (entry.row.id === 'launcher_size') return 'Grows with the window; this sets how big it is';
  return help || 'No additional description is available for this setting.';
}

export function SettingsNavigation({ Row }) {
  const s = useStore();
  const [category, setCategory] = useState(() => remembered('settings-tab', 'graphics'));
  const [sections, setSections] = useState({});
  const [favourites, setFavourites] = useState(() => readFavourites(remembered('settings-favourites', '["graphics:resolution","graphics:max_fps","sound:volume","driving:wheel_range"]')));
  const [query, setQuery] = useState('');
  const [focused, setFocused] = useState(null);
  const [ask, dialog] = useConfirm();
  const scroll = useRef();
  useEffect(() => {
    const key = (event) => {
      if ((event.ctrlKey || event.metaKey) && ['f', 'k'].includes(event.key.toLowerCase())) {
        event.preventDefault();
        document.querySelector('.settings-search input')?.focus();
      } else if (event.key === 'Escape' && document.activeElement?.closest('.settings-search')) setQuery('');
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, []);
  useEffect(() => { if (scroll.current) scroll.current.scrollTop = 0; }, [category, sections, query]);
  if (!s.settings) return null;
  const platform = s.boot?.platform || '';
  const visible = (row) => isSettingVisible(row, s.settings, platform);
  const tabs = SETTINGS_TABS.filter(visible);
  const favouritesPage = category === 'favourites';
  const current = tabs.find((tab) => tab.id === category) || tabs[0];
  const groups = allGroups.get(current.id).filter((group) => group.rows.some((row) => isSettingControl(row) && visible(row)));
  const requested = sections[current.id] ?? remembered('settings-section-' + current.id, '');
  const group = groups.find((group) => group.id === requested) || groups[0];
  const words = fold(query).split(/\s+/).filter(Boolean);
  const searching = words.length > 0;
  const shown = entries.filter((entry) => tabs.includes(entry.tab) && visible(entry.row) && (searching ? isSettingControl(entry.row) && matches(entry, words, s.settings) : favouritesPage ? isSettingControl(entry.row) && favourites.has(entry.id) : entry.tab === current && entry.group === group));
  const label = searching ? 'Search all settings' : favouritesPage ? 'Favourite settings' : current.label;
  const pick = (id) => { setCategory(id); setQuery(''); setFocused(null); remember('settings-tab', id); };
  const pickGroup = (id) => {
    setSections({ ...sections, [current.id]: id });
    setFocused(null);
    remember('settings-section-' + current.id, id);
  };
  const pin = (id) => {
    const next = new Set(favourites);
    next.has(id) ? next.delete(id) : next.add(id);
    setFavourites(next);
    remember('settings-favourites', JSON.stringify([...next]));
  };
  return html`<div class="settings-frame">
    <aside class="settings-categories" aria-label=${t('Settings')}>
      <h1>${t('Settings')}</h1>
      <nav>
        <button class=${favouritesPage && !searching ? 'on' : ''} data-settings-category="favourites" aria-current=${favouritesPage && !searching ? 'page' : undefined} onClick=${() => pick('favourites')}><${Icon} name="star" /><span>${t('Favourite settings')}</span><small>${favourites.size}</small></button>
        <div class="settings-nav-label">${t('Game preferences')}</div>
        ${tabs.map((tab) => html`<button key=${tab.id} class=${!favouritesPage && !searching && tab === current ? 'on' : ''} data-settings-category=${tab.id} aria-current=${!favouritesPage && !searching && tab === current ? 'page' : undefined} onClick=${() => pick(tab.id)}><${Icon} name=${icons[tab.id]} /><span>${t(tab.label)}</span></button>`)}
      </nav>
      ${!['android', 'ios'].includes(platform) && html`<button class="settings-troubleshoot" onClick=${() => { remember('troubleshoot-tab', 'content'); set({ page: 'troubleshoot' }); }}><${Icon} name="info" />${t('Troubleshooting')}</button>`}
    </aside>
    <section class="settings-main">
      <header class="settings-heading">
        <div class="settings-heading-line"><div><div class="settings-breadcrumb">${t('Settings')} / ${t(label)}</div><h2>${t(label)}</h2><p>${t('Choose a topic to find the settings you need.')}</p></div><div class="settings-search"><${Search} value=${query} onInput=${setQuery} placeholder="Search all settings (Ctrl+F)" /></div></div>
        ${!searching && !favouritesPage && !current.page && html`<nav class="settings-subtabs" aria-label=${t(current.label)}>${groups.map((item) => html`<button key=${item.id} class=${item === group ? 'on' : ''} data-settings-group=${item.id} aria-current=${item === group ? 'page' : undefined} onClick=${() => pickGroup(item.id)}>${t(item.label)}</button>`)}</nav>`}
      </header>
      <div class="settings-body" ref=${scroll}>
        ${!searching && !favouritesPage && current.page === 'ailists' ? html`<${AiListsPage} />` : html`<div class="settings-workspace">
          <section class="settings-options">
            <div class="settings-options-heading"><h3>${t(searching ? 'Search results' : favouritesPage ? 'Favourite settings' : group?.label || current.label)}<small>${t('{n} options', { n: shown.filter((entry) => isSettingControl(entry.row)).length })}</small></h3></div>
            <div class="settings-options-card" key=${searching ? 'search' : favouritesPage ? 'favourites' : current.id + ':' + group?.id}>
              ${shown.length ? shown.map((entry) => {
                const { row, id, key } = entry;
                if (row.type === 'section' && row.title === group?.label && !searching && !favouritesPage) return null;
                const control = isSettingControl(row);
                const selected = focused?.id === id;
                const helpId = "settings-description-" + key;
                return html`<div key=${key} class=${'settings-entry' + (control ? ' control' : ' structural') + (row.type === 'custom' ? ' custom' : '') + (control && selected ? ' help-selected' : '')} data-setting-key=${typeof row.key === 'string' ? row.key : row.id || undefined} onFocusIn=${() => setFocused(entry)} onClick=${() => setFocused(entry)}>
                  <div class="settings-entry-content">${(searching || favouritesPage) && html`<div class="settings-result-path">${t(entry.tab.label)} / ${t(entry.group.label)}</div>`}<${Row} row=${row} ask=${ask} compact />${control && html`<div id=${helpId} class="settings-inline-help" hidden=${!selected}><b>${t('About this setting')}</b><p>${t(settingHelp(entry, s.settings))}</p></div>`}</div>
                  ${control && html`<div class="settings-entry-tools"><button class=${'settings-help' + (selected ? ' on' : '')} aria-label=${t('View help for {name}', { name: t(settingLabel(row)) })} aria-controls=${'settings-description ' + helpId} title=${t('View help for {name}', { name: t(settingLabel(row)) })} onClick=${(event) => { event.stopPropagation(); setFocused(entry); }}><${Icon} name="info" size=${16} /><span>${t('Setting help')}</span></button><button class=${'settings-pin star' + (favourites.has(id) ? ' on' : '')} aria-label=${t(favourites.has(id) ? 'Remove from favourites: {name}' : 'Add to favourites: {name}', { name: t(settingLabel(row)) })} aria-pressed=${favourites.has(id)} title=${t(favourites.has(id) ? 'Remove from favourite settings' : 'Add to favourite settings')} onClick=${(event) => { event.stopPropagation(); pin(id); }}><${Icon} name="star" /></button></div>`}
                </div>`;
              }) : html`<div class="empty"><${Icon} name=${searching ? 'search' : 'star'} /><b>${t(searching ? 'No setting matches this search' : 'No favourite settings yet')}</b><p>${t(searching ? 'Try a setting name or its English key.' : 'Use the star beside a setting to keep it here.')}</p></div>`}
            </div>
          </section>
          <aside class="settings-info" id="settings-description" aria-live="polite"><div class="settings-info-icon"><${Icon} name=${icons[current.id]} size=${44} /></div><div class="settings-info-label"><${Icon} name="info" size=${16} />${t('About this setting')}</div><h3>${t(focused ? settingLabel(focused.row) : 'Setting help')}</h3><p>${t(focused ? settingHelp(focused, s.settings) : 'Select Help beside a setting to see what it does.')}</p></aside>
        </div>`}
      </div>
      <footer class="settings-footer"><${Icon} name="info" size=${16} /><span>${t('Every change is saved at once; the game reads them when it starts.')}</span><span class="settings-footer-key">Ctrl+F · ${t('Search')}</span></footer>
    </section>
    ${dialog}
  </div>`;
}
