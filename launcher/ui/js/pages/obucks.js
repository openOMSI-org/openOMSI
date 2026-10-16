// O-Bucks: the future in-launcher shop, intentionally offline and cosmetic.

import { html } from '../../vendor/preact-htm.js';
import { t } from '../i18n.js';
import { Card, Notice, PageHead } from '../ui.js';

export function OBucksPage() {
  return html`
    <${PageHead} title="O-Bucks" text="The O-Bucks menu is being built." />
    <${Card} title="O-Bucks">
      <${Notice} icon="info">
        <b>${t('This menu is under construction.')}</b>
        <p class="muted">${t('Soon you will be able to use your O-Bucks to buy liveries buses and other creations that players can upload.')}</p>
      </${Notice}>
    </${Card}>
  `;
}
