import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { t, bytes, language } from '../i18n.js';
import { call } from '../api.js';
import { set } from '../store.js';
import { Button, Dialog, Notice, Spinner, Tag, Empty } from '../ui.js';

const WORKING = ['downloading', 'installing', 'installed'];

function day(iso) {
  const d = new Date(iso + 'T12:00:00');
  return iso && !isNaN(d) ? d.toLocaleDateString(language(), { day: 'numeric', month: 'short', year: 'numeric' }) : '';
}

function Row({ r, on, blocked, onPick }) {
  const off = r.current || !!r.broken || blocked;
  return html`<button type="button" class=${'list-item version' + (on ? ' on' : '')} disabled=${off} aria-pressed=${on} onClick=${() => onPick(r.version)}>
    <span class="grow">
      <span class="title">openOMSI ${r.version}</span>
      <span class="sub">${[day(r.date), r.size ? bytes(r.size) : ''].filter(Boolean).join(', ')}</span>
    </span>
    <span class="tags">
      ${r.latest && html`<${Tag} kind="ok">${t('Latest')}</${Tag}>`}
      ${r.current && html`<${Tag} kind="accent">${t('Installed')}</${Tag}>`}
      ${r.broken === 'here' && html`<${Tag} kind="danger">${t('Did not start on this computer')}</${Tag}>`}
      ${r.broken === 'listed' && html`<${Tag} kind="danger">${t('Known to be broken')}</${Tag}>`}
    </span>
  </button>`;
}

function Progress({ job }) {
  const v = { version: job.version };
  if (job.state === 'downloading') {
    const pct = job.total > 0 ? Math.min(100, (job.done / job.total) * 100) : 0;
    return html`<div class="version-progress">
      <b>${t('Downloading openOMSI {version}', v)}</b>
      <div class="progress"><div style=${`width:${pct.toFixed(1)}%`}></div></div>
      <span class="muted small">${job.total > 0 ? `${bytes(job.done)} / ${bytes(job.total)}` : html`<${Spinner} />`}</span>
    </div>`;
  }
  return html`<div class="version-progress">
    <b>${t('Installing openOMSI {version}', v)}</b>
    <div class="progress"><div style="width:100%"></div></div>
    <span class="muted small">${t('The new version is put in place; the launcher starts again in a moment.')}</span>
  </div>`;
}

export function VersionsDialog({ onClose }) {
  const [data, setData] = useState(null);
  const [error, setError] = useState(null);
  const [pick, setPick] = useState(null);
  const [job, setJob] = useState(null);
  const [starting, setStarting] = useState(false);

  const load = () => {
    setError(null);
    setData(null);
    setPick(null);
    setJob(null);
    call('versions')
      .then(setData)
      .catch((e) => setError(e.message));
  };

  useEffect(() => {
    call('version_status')
      .then((j) => (WORKING.includes(j?.state) ? setJob(j) : load()))
      .catch(load);
  }, []);

  useEffect(() => {
    if (!job || !['downloading', 'installing'].includes(job.state)) return;
    const id = setTimeout(() => {
      call('version_status')
        .then(setJob)
        .catch((e) => setJob({ state: 'failed', version: job.version, error: e.message }));
    }, 400);
    return () => clearTimeout(id);
  }, [job]);

  useEffect(() => {
    if (job?.state !== 'installed' || starting) return;
    setStarting(true);
    call('settings_get')
      .then((settings) => set({ settings }))
      .catch(() => {});
    call('version_restart').catch((e) => setJob({ state: 'stuck', version: job.version, error: e.message }));
  }, [job?.state]);

  const install = () => {
    setError(null);
    call('version_install', { version: pick })
      .then(setJob)
      .catch((e) => setError(t(e.message)));
  };

  const working = WORKING.includes(job?.state);
  const chosen = data?.list.find((r) => r.version === pick);
  let body;
  let footer;
  if (working) {
    body = html`<${Progress} job=${job} />`;
  } else if (job?.state === 'failed' || job?.state === 'stuck') {
    const title = job.state === 'failed' ? 'openOMSI {version} was not installed' : 'openOMSI {version} is installed: start it again yourself.';
    body = html`<${Notice} kind="danger"><b>${t(title, { version: job.version })}</b><div class="dim small selectable">${job.error}</div></${Notice}>`;
    footer = html`<${Button} label="Close" onClick=${onClose} />${job.state === 'failed' && html`<${Button} icon="refresh" label="Try again" onClick=${load} />`}`;
  } else if (!data && !error) {
    body = html`<div class="row muted"><${Spinner} />${t('Asking GitHub for the releases…')}</div>`;
    footer = html`<${Button} label="Cancel" onClick=${onClose} />`;
  } else if (!data) {
    body = html`<${Notice} kind="danger"><b>${t('The list of versions could not be read.')}</b><div class="dim small selectable">${error}</div></${Notice}>`;
    footer = html`<${Button} label="Close" onClick=${onClose} /><${Button} icon="refresh" label="Try again" onClick=${load} />`;
  } else {
    const blocked = !!data.blocked;
    body = html`<div class="versions">
      ${blocked && html`<${Notice} kind="warn"><b>${t('Versions cannot be installed here.')}</b><div class="dim small selectable">${data.blocked}</div></${Notice}>`}
      ${data.list.length
        ? html`<div class="list">${data.list.map((r) => html`<${Row} key=${r.version} r=${r} on=${pick === r.version} blocked=${blocked} onPick=${setPick} />`)}</div>`
        : html`<${Empty} icon="history" title="No release to choose from" />`}
      ${chosen && html`<${Notice}>
        ${t('openOMSI {version} is downloaded from GitHub and takes the place of this version; the launcher then starts again. Your mods, settings and saves stay as they are.', { version: chosen.version })}
        ${!chosen.latest && html`<div class="version-keep">${t('Update checks are turned off so that this version stays. To come back, choose the latest version in this list, or download it from GitHub if openOMSI {version} does not have this list yet.', { version: chosen.version })}</div>`}
      </${Notice}>`}
      ${error && html`<${Notice} kind="danger"><div class="selectable">${error}</div></${Notice}>`}
    </div>`;
    footer = html`<${Button} label="Cancel" onClick=${onClose} />
      <${Button} kind="primary" icon="download" disabled=${!chosen || blocked} onClick=${install}>${chosen ? t('Install openOMSI {version}', { version: chosen.version }) : t('Install')}</${Button}>`;
  }
  return html`<${Dialog} title="Choose a version" onClose=${working ? null : onClose} footer=${footer}>${body}</${Dialog}>`;
}
