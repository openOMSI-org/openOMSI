import { html, useEffect, useState } from '../vendor/preact-htm.js';
import { t, language } from './i18n.js';
import { call } from './api.js';
import { S, set, toast, useStore, remember, remembered } from './store.js';
import { Button, Dialog, Notice, Spinner, Tag, Empty } from './ui.js';
import { blocks, inline, compareVersions, freshVersions } from './markdown.js';

const SEEN = 'changelog-seen';

const openUrl = (url) => call('open_url', { url }).catch((e) => toast(t(e.message), 'error'));

function Inline({ text }) {
  return inline(text).map((p) =>
    p.kind === 'bold' ? html`<b>${p.text}</b>`
    : p.kind === 'italic' ? html`<i>${p.text}</i>`
    : p.kind === 'code' ? html`<code>${p.text}</code>`
    : p.kind === 'link' ? html`<a href=${p.url} onClick=${(e) => { e.preventDefault(); openUrl(p.url); }}>${p.text}</a>`
    : p.text);
}

/** Release notes (Markdown) drawn as elements; the first line naming the version is left out
 * where the version is already shown above them. */
export function Notes({ text, version }) {
  let list = blocks(text);
  if (version && list[0]?.kind === 'paragraph' && list[0].text.replace(/\*/g, '').trim() === `openOMSI ${version}`) list = list.slice(1);
  if (!list.length) return null;
  return html`<div class="notes selectable">${list.map((b) =>
    b.kind === 'heading' ? html`<div class=${'notes-h' + Math.min(b.level, 3)}><${Inline} text=${b.text} /></div>`
    : b.kind === 'list' ? html`<ul>${b.items.map((i) => html`<li style=${i.depth ? `margin-left:${i.depth * 16}px` : ''}><${Inline} text=${i.text} /></li>`)}</ul>`
    : b.kind === 'quote' ? html`<blockquote><${Inline} text=${b.text} /></blockquote>`
    : html`<p><${Inline} text=${b.text} /></p>`)}</div>`;
}

function day(iso) {
  const d = new Date(iso + 'T12:00:00');
  return iso && !isNaN(d) ? d.toLocaleDateString(language(), { day: 'numeric', month: 'short', year: 'numeric' }) : '';
}

export function openChangelog() {
  set({ changelog: true });
}

/** "What's new": the notes of every release, newest first. */
export function ChangelogDialog() {
  const s = useStore();
  const [data, setData] = useState(null);
  const [error, setError] = useState(null);
  const load = () => {
    setError(null);
    setData(null);
    call('versions').then((d) => setData({ ...d, seen: remembered(SEEN, '') })).catch((e) => setError(e.message));
  };
  useEffect(() => { if (s.changelog) load(); }, [s.changelog]);
  if (!s.changelog) return null;
  const close = () => {
    if (data?.current) remember(SEEN, data.current);
    set({ changelog: false });
  };
  let body;
  if (!data && !error) body = html`<div class="row muted"><${Spinner} />${t('Asking GitHub for the releases…')}</div>`;
  else if (!data) body = html`<${Notice} kind="danger"><b>${t('The list of versions could not be read.')}</b><div class="dim small selectable">${error}</div></${Notice}>`;
  else if (!data.list.length) body = html`<${Empty} icon="history" title="No release to choose from" />`;
  else {
    const { seen, current } = data;
    const fresh = freshVersions(data.list, current, seen);
    const isNew = (v) => seen && compareVersions(v, seen) > 0 && compareVersions(v, current) <= 0;
    body = html`<div class="changelog">${data.list.map((r) => html`<details key=${r.version} open=${fresh.includes(r.version)}>
      <summary>
        <span class="grow"><b>openOMSI ${r.version}</b> <span class="muted small">${day(r.date)}</span></span>
        <span class="tags">
          ${isNew(r.version) && html`<${Tag} kind="ok">${t('New')}</${Tag}>`}
          ${r.beta && html`<${Tag}>${t('Beta')}</${Tag}>`}
          ${r.current && html`<${Tag} kind="accent">${t('Installed')}</${Tag}>`}
          ${r.latest && !r.current && html`<${Tag} kind="ok">${t('Latest')}</${Tag}>`}
        </span>
      </summary>
      <${Notes} text=${r.notes} version=${r.version} />
    </details>`)}</div>`;
  }
  const footer = html`${data?.list?.length > 0 && html`<${Button} kind="ghost" icon="external" label="Open on GitHub" onClick=${() => openUrl(S.update?.repo ? `${S.update.repo}/releases` : data.list[0].page)} />`}
    ${error && html`<${Button} icon="refresh" label="Try again" onClick=${load} />`}
    <${Button} kind="primary" label="Close" onClick=${close} />`;
  return html`<${Dialog} title="What's new" wide onClose=${close} footer=${footer}>${body}</${Dialog}>`;
}
