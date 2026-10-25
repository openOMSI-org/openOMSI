// Setup: where OMSI 2 and the game program are, where the launcher keeps its files.

import { html, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call, onPhone } from '../api.js';
import { Button, Card, Dialog, Field, Input, Notice, PageHead } from '../ui.js';
import { loadContent } from '../data.js';

// Ask until the background move of the .openomsi folder has ended.
async function waitForMove() {
  for (;;) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const s = await call('data_move_status');
    if (s.state === 'running') continue;
    if (s.state === 'done') return s.result;
    throw new Error(s.error || 'The move of the .openomsi folder did not finish');
  }
}

export function SetupPage() {
  const s = useStore();
  const b = s.boot;
  const [root, setRoot] = useState(s.config?.root || b.root || '');
  const [game, setGame] = useState(s.config?.game || '');
  const [dataPath, setDataPath] = useState(b.data_dir || '');
  const [relocating, setRelocating] = useState(false);
  const [busy, setBusy] = useState(false);
  const save = async (moveData = null, existingAction = null) => {
    const changed = !onPhone && dataPath.trim() !== b.data_dir;
    setBusy(true);
    try {
      let path = relocating?.path || dataPath.trim();
      if (changed && !relocating && b.platform === 'macos') {
        path = await call('pick_data_folder', { path });
        if (!path) { setBusy(false); return; }
        setDataPath(path);
      }
      if (changed && moveData === null && existingAction === null) {
        const destination = await call('data_folder_check', { path });
        setDataPath(destination.path);
        if (!destination.same) {
          setRelocating(destination);
          setBusy(false);
          return;
        }
        path = b.data_dir;
      }
      setRelocating(false);
      let r = await call('config_save', { root, game, ...(!onPhone ? { data_path: path, move_data: moveData, existing_action: existingAction } : {}) });
      if (r.moving) {
        // (the copy runs in the launcher in the background; leaving this page does not stop it)
        toast(t('Moving the .openomsi folder in the background…'), 'ok', 6000);
        r = await waitForMove();
      }
      if (!r.saved) {
        if (r.existing_folder) {
          setRelocating({ path: r.existing_folder, exists: true });
          setBusy(false);
          return;
        }
        const what = r.missing.slice(0, 6).join(', ') + (r.missing.length > 6 ? ` (+${r.missing.length - 6})` : '');
        toast(t('Not saved: {folder} is not a complete OMSI 2 installation, it lacks {what}', { folder: r.chosen, what }), 'error');
        setBusy(false);
        return;
      }
      set({ config: r.config, boot: { ...S.boot, root: r.root, root_error: r.root_error, game: r.game, data_dir: r.data_dir, content_dir: r.content_dir } });
      setDataPath(r.data_dir || path);
      if (r.warning) toast(r.warning, 'error', 12000);
      // Reset cached pages and pending writes before using a fresh settings directory.
      if (changed) { window.location.reload(); return; }
      if (r.used_other) toast(t('Saved, but the OMSI 2 folder used is {folder}', { folder: r.used_other }), 'error');
      else if (r.root) toast(t('OMSI 2 was found.'), 'ok');
      else toast(r.root_error || t('This is not a complete OMSI 2 installation.'), 'error');
      if (r.root) loadContent();
    } catch (e) {
      toast(t(e.message), 'error');
    }
    setBusy(false);
  };
  const browse = async (cmd, title, to) => {
    const p = await call(cmd, { title: t(title) }).catch(() => null);
    if (p) to(p);
  };
  const browseData = async () => {
    setBusy(true);
    try {
      const path = await call('pick_data_folder', { path: null });
      if (path) {
        const destination = await call('data_folder_check', { path });
        setDataPath(destination.path);
        if (destination.exists && !destination.same) setRelocating(destination);
      }
    } catch (e) { toast(t(e.message), 'error'); }
    setBusy(false);
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
              </${Field}>
              <${Field} label=".openomsi folder" help="Settings, profiles, logs and user data. Choose a parent folder or an existing .openomsi folder; the folder name must be .openomsi.">
                <div class="row"><div class="grow"><${Input} value=${dataPath} onInput=${setDataPath} /></div><${Button} icon="folder" label="Browse…" disabled=${busy} onClick=${browseData} /></div>
                ${b.platform === 'macos' && html`<p class="muted">${t('macOS will ask you to grant access to the selected folder when saving.')}</p>`}
              </${Field}>`}
          <div><${Button} kind="primary" icon="check" label="Save" disabled=${busy} onClick=${() => save()} /></div>
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
    </div>
    ${relocating && html`<${Dialog} title=${relocating.exists ? 'Existing .openomsi folder found' : 'Change the .openomsi folder'} onClose=${() => setRelocating(false)} footer=${html`
      <${Button} label="Cancel" onClick=${() => setRelocating(false)} />
      ${relocating.exists ? html`
        <${Button} label="Move current and overwrite" disabled=${busy} onClick=${() => save(true, 'overwrite')} />
        <${Button} kind="primary" label="Use existing folder" disabled=${busy} onClick=${() => save(null, 'use')} />
      ` : html`
        <${Button} label="Start fresh" disabled=${busy} onClick=${() => save(false)} />
        <${Button} kind="primary" label="Move current folder" disabled=${busy} onClick=${() => save(true)} />
      `}
    `}>
      <p>${t(relocating.exists
        ? 'An existing .openomsi folder was found. Use it to load its settings and profiles, leaving the current folder unchanged. Moving the current folder here will overwrite the destination and replace all of its existing data.'
        : 'Move the current .openomsi folder to the new location, or create a fresh folder there? Starting fresh keeps the old folder unchanged.')}</p>
      <p class="selectable">${relocating.path}</p>
      <p class="muted">${t('Other copies of openOMSI keep using the old folder.')}</p>
    </${Dialog}>`}
  `;
}
