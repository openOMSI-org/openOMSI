// Add-ons: projects others make for openOMSI, each a link to its own page. The list is read
// from openOMSI-org/add-ons-list each time the page opens (docs/ADDONS.md says how one gets in);
// without a connection the page says so. Nothing of an add-on is downloaded, installed or run
// here: the button opens the project's page in the browser, after saying so.

import { html, useEffect, useState } from '../../vendor/preact-htm.js';
import { t, language } from '../i18n.js';
import { call } from '../api.js';
import { Button, Card, Dialog, Empty, Notice, PageHead, Picture, Spinner, Tag } from '../ui.js';
import { toast } from '../store.js';

export function AddonsPage() {
  const [list, setList] = useState(null);
  const [error, setError] = useState('');
  const [leaving, setLeaving] = useState(null);
  const [loading, setLoading] = useState(false);
  const read = () => {
    setLoading(true);
    setError('');
    call('addons')
      .then((v) => setList(Array.isArray(v) ? v : []))
      .catch((e) => (setList(null), setError(e.message || String(e))))
      .finally(() => setLoading(false));
  };
  // (read anew each time: an entry added to or taken out of the list shows at once)
  useEffect(read, []);
  const open = () => {
    setLeaving(null);
    call('open_url', { url: leaving.homepage }).catch((e) => toast(e.message, 'error'));
  };
  return html`
    <${PageHead} title="Add-ons" text="Projects made for openOMSI by others. Their own websites say what they do and how to get them." />
    ${error && html`<${Notice} kind="danger">
      <div class="stack">
        <span>${t('The add-on list could not be loaded. Check the internet connection and try again.')}</span>
        <span class="muted small selectable">${error}</span>
        <div><${Button} small kind="ghost" icon="refresh" label="Refresh" disabled=${loading} onClick=${read} /></div>
      </div>
    </${Notice}>`}
    ${list == null && !error && html`<div class="row muted"><${Spinner} /></div>`}
    ${list?.length === 0 && html`<${Card}><${Empty} icon="globe" title="No add-on yet" /></${Card}>`}
    <div class="grid-2">
      ${(list || []).map((a) => html`<${Card} key=${a.id}>
        <div class="stack">
          <div class="row">
            <${Picture} src=${a.icon?.startsWith('data:image/') ? a.icon : null} icon="globe" class="addon-icon" />
            <div class="grow"><b>${a.name}</b><div class="muted small">${t('by {author}', { author: a.author })}</div></div>
          </div>
          <p class="dim">${a.translations?.[language()] || a.description}</p>
          ${a.tags?.length > 0 && html`<div class="row wrap">${a.tags.map((x) => html`<${Tag}>${x}</${Tag}>`)}</div>`}
          <div><${Button} icon="external" label="Open website" onClick=${() => setLeaving(a)} /></div>
        </div>
      </${Card}>`)}
    </div>
    ${leaving && html`<${Dialog} title="Open the website?" onClose=${() => setLeaving(null)} footer=${html`
        <${Button} label="Cancel" onClick=${() => setLeaving(null)} />
        <${Button} kind="primary" icon="external" label="Open website" onClick=${open} />`}>
      <p class="dim">${t('Opens {site} in your browser. The website is not part of openOMSI.', { site: new URL(leaving.homepage).host })}</p>
    </${Dialog}>`}
  `;
}
