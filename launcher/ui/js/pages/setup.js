// Setup: where OMSI 2 and the game program are, where the launcher keeps its files.

import { html, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call, onPhone } from '../api.js';
import { Button, Card, Field, Input, Notice, PageHead } from '../ui.js';
import { loadContent } from '../data.js';

export function SetupPage() {
  const s = useStore();
  const b = s.boot;
  const [root, setRoot] = useState(s.config?.root || b.root || '');
  const [game, setGame] = useState(s.config?.game || '');
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    try {
      const r = await call('config_save', { root, game });
      if (!r.saved) {
        const what = r.missing.slice(0, 6).join(', ') + (r.missing.length > 6 ? ` (+${r.missing.length - 6})` : '');
        toast(t('Not saved: {folder} is not a complete OMSI 2 installation, it lacks {what}', { folder: r.chosen, what }), 'error');
        setBusy(false);
        return;
      }
      set({ config: r.config, boot: { ...S.boot, root: r.root, root_error: r.root_error, game: r.game } });
      if (r.used_other) toast(t('Saved, but the OMSI 2 folder used is {folder}', { folder: r.used_other }), 'error');
      else if (r.root) toast(t('OMSI 2 was found.'), 'ok');
      else toast(r.root_error || t('This is not a complete OMSI 2 installation.'), 'error');
      if (r.root) loadContent();
    } catch (e) {
      toast(e.message, 'error');
    }
    setBusy(false);
  };
  const browse = async (cmd, title, to) => {
    const p = await call(cmd, { title: t(title) }).catch(() => null);
    if (p) to(p);
  };
  return html`
    <${PageHead} title="Setup" text="openOMSI plays the content of an OMSI 2 installation: the original game must be installed (Steam or another copy)." />
    <div class="stack" style="max-width:860px">
      ${!b.root && html`<${Notice} kind="warn">${b.root_error || t('OMSI 2 was not found. Choose its folder below.')}</${Notice}>`}
      <${Card} title="Folders">
        <div class="stack">
          ${onPhone
            ? html`<${Field} label="OMSI 2 folder" help="Copy the whole OMSI 2 folder (with Omsi.exe, maps and Vehicles) from a computer into openOMSI/OMSI 2 in the phone's internal storage; it is found there by itself.">
                <${Input} value=${root} placeholder=${b.root || '/storage/emulated/0/openOMSI/OMSI 2'} onInput=${setRoot} />
              </${Field}>`
            : html`<${Field} label="OMSI 2 folder" help="The folder with Omsi.exe, maps and Vehicles. Left empty, the usual places are searched (Steam libraries, Wine bottles).">
                <div class="row"><div class="grow"><${Input} value=${root} placeholder=${b.root || 'Automatic'} onInput=${setRoot} /></div><${Button} icon="folder" label="Browse…" onClick=${() => browse('pick_folder', 'Choose the OMSI 2 folder', setRoot)} /></div>
              </${Field}>
              <${Field} label="Game program" help="The openomsi program the duties start. Left empty, the one beside the launcher.">
                <div class="row"><div class="grow"><${Input} value=${game} placeholder=${b.game || 'Automatic'} onInput=${setGame} /></div><${Button} icon="file" label="Browse…" onClick=${() => browse('pick_file', 'Choose the game program', setGame)} /></div>
              </${Field}>`}
          <div><${Button} kind="primary" icon="check" label="Save" disabled=${busy} onClick=${save} /></div>
        </div>
      </${Card}>
      <${Card} title="Where things are">
        <dl class="facts" style="grid-template-columns:auto 1fr">
          <dt>${t('OMSI 2')}</dt><dd class="selectable" style="text-align:left">${b.root || '-'}</dd>
          <dt>${t('Content folder (mods)')}</dt><dd class="selectable" style="text-align:left">${b.content_dir || '-'}</dd>
          <dt>${t('Settings and logs')}</dt><dd class="selectable" style="text-align:left">${b.data_dir}</dd>
          <dt>${t('Game program')}</dt><dd class="selectable" style="text-align:left">${b.game || '-'}</dd>
        </dl>
        <div class="row" style="margin-top:14px">
          ${b.content_dir && html`<${Button} small icon="folder" label="Open the content folder" onClick=${() => call('open_path', { path: b.content_dir })} />`}
          <${Button} small icon="folder" label="Open the settings folder" onClick=${() => call('open_path', { path: b.data_dir })} />
        </div>
      </${Card}>
      ${!onPhone && html`<${Card} title="The game's own launcher">
        <div class="row">
          <p class="muted grow">${t('The launcher drawn by the game itself, as before: for a computer where this window has trouble.')}</p>
          <${Button} icon="external" label="Open it" onClick=${() => call('classic_launcher').catch((e) => toast(e.message, 'error'))} />
        </div>
      </${Card}>`}
    </div>
  `;
}
