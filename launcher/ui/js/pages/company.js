import { html, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t } from '../i18n.js';
import { call } from '../api.js';
import { Card, Empty, Input, Button, PageHead } from '../ui.js';

export function CompanyPage() {
  const s = useStore();
  const [name, setName] = useState('');
  const create = async () => {
    const value = name.trim();
    if (!value) return;
    try {
      set({ company: await call('company_create', { name: value }) });
      setName('');
      toast(t('Company {name} created.', { name: value }), 'ok');
    } catch (e) {
      toast(t(e.message), 'error');
    }
  };
  const c = s.company;
  return html`
    <${PageHead} title="Company" text="Build a bus company from completed duties, fleet work, and reliable service." />
    ${!c ? html`
      <${Card} title="Start a company">
        <div class="row">
          <div class="grow"><${Input} value=${name} placeholder="Company name" onInput=${setName} onKeyDown=${(e) => e.key === 'Enter' && create()} /></div>
          <${Button} icon="plus" label="Create" disabled=${!name.trim()} onClick=${create} />
        </div>
      </${Card}>
    ` : html`
      <${Card}>
        <div class="row" style="gap:18px">
          <div class="avatar" style="width:64px;height:64px;font-size:26px">${c.name[0]?.toUpperCase()}</div>
          <div class="grow"><h2 style="font-size:22px">${c.name}</h2><div class="muted">${t('Level {n}', { n: c.level })} - ${c.xp} XP</div></div>
        </div>
      </${Card}>
      <div class="stats">
        ${[['cash', c.cash.toFixed(2), 'cash'], ['star', `${c.reputation.toFixed(0)} %`, 'reputation'], ['route', c.completed_sessions, 'completed duties'], ['trending', Math.max(0, c.next_level_xp - c.xp), 'XP to next level']].map(([icon, value, label]) => html`<div class="stat"><div class="muted" style="margin-bottom:6px">${t(label)}</div><b>${value}</b></div>`)}
      </div>
      <${Card} title="Recent company duties">
        ${c.sessions.length === 0 ? html`<${Empty} icon="history" text="Complete a duty to start the company ledger." />` : html`<table class="table"><thead><tr><th>${t('Map')}</th><th>${t('Line')}</th><th>${t('Distance')}</th><th>${t('Takings')}</th></tr></thead><tbody>${c.sessions.map((x) => html`<tr><td>${x.map}</td><td>${x.line || t('Free drive')}</td><td class="num">${(x.metres / 1000).toFixed(1)} km</td><td class="num">${x.cash.toFixed(2)}</td></tr>`)}</tbody></table>`}
      </${Card}>
    `}
  `;
}