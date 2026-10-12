// Profile: the driver whose personnel file the game writes, their figures and recent runs.

import { html, useState } from '../../vendor/preact-htm.js';
import { S, useStore, set, toast } from '../store.js';
import { t, ago } from '../i18n.js';
import { call } from '../api.js';
import { Icon } from '../icons.js';
import { Button, Card, Input, Select, Empty, PageHead, Segmented, useConfirm } from '../ui.js';
import { loadProfiles, busName, mapName } from '../data.js';

const hours = (h) => `${Math.floor(h)} h ${String(Math.round((h % 1) * 60)).padStart(2, '0')} min`;

async function select(name) {
  try {
    const config = await call('profile_select', { name });
    set({ config, profile: await call('profile', { name }) });
  } catch (e) {
    toast(e.message, 'error');
  }
}

export function ProfilePage() {
  const s = useStore();
  const p = s.profile;
  const [name, setName] = useState('');
  const [sex, setSex] = useState('M');
  const [ask, confirm] = useConfirm();
  const create = async () => {
    const n = name.trim();
    if (!n) return;
    try {
      await call('profile_create', { name: n, sex });
      setName('');
      await select(n);
      await loadProfiles();
      toast(t('Driver {name} created.', { name: n }), 'ok');
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const remove = async () => {
    const n = S.config?.profile;
    if (!n || !(await ask({ title: 'Delete this driver?', text: t('The personnel file of {name} is deleted with all its figures.', { name: n }), yes: 'Delete', danger: true }))) return;
    try {
      await call('profile_delete', { name: n });
      toast(t('Personnel file of {name} deleted.', { name: n }), 'ok');
      const rest = (S.profiles || []).filter((x) => x !== n);
      if (rest[0]) await select(rest[0]);
      await loadProfiles();
    } catch (e) {
      toast(e.message, 'error');
    }
  };
  const tiles = p ? [
    ['clock', hours(p.hours), 'hours driven'],
    ['route', `${p.km.toFixed(1)} km`, 'distance'],
    ['map', p.stops, 'stops served'],
    ['history', `${p.early} / ${p.late}`, 'early / late'],
    ['ticket', p.tickets.toFixed(0), 'tickets sold'],
    ['save', p.cash.toFixed(2), 'takings'],
    ['alert', p.crashes, 'crashes'],
    ['person', p.hurt, 'pedestrians hurt'],
    ['bus', `${p.rating_driving.toFixed(0)} %`, 'driving'],
    ['users', `${p.rating_comfort.toFixed(0)} %`, 'comfort'],
    ['ticket', `${p.rating_tickets.toFixed(0)} %`, 'ticket selling'],
    ['gamepad', p.sessions.length, 'runs'],
  ] : [];
  const from = p ? (p.level - 1) ** 2 * 250 : 0;
  const into = p ? Math.max(0, Math.min(1, (p.xp - from) / Math.max(1, p.next_level_xp - from))) : 0;
  return html`
    <${PageHead} title="Profile" text="The driver whose personnel file the game writes: hours, kilometres, punctuality, tickets." />
    <div class="stack">
      <${Card} title="Driver">
        <div class="grid-2">
          <div class="row">
            <div class="grow"><${Select} raw value=${s.config?.profile || ''} options=${(s.profiles || []).map((n) => [n, n])} onChange=${select} placeholder="Choose a driver" /></div>
            <${Button} kind="danger" icon="trash" title="Delete this driver" disabled=${!s.config?.profile} onClick=${remove} />
          </div>
          <div class="row">
            <div class="grow"><${Input} value=${name} placeholder="New driver's name" onInput=${setName} onKeyDown=${(e) => e.key === 'Enter' && create()} /></div>
            <${Segmented} value=${sex} options=${[['M', 'Male'], ['F', 'Female']]} onChange=${setSex} />
            <${Button} icon="plus" label="Create" disabled=${!name.trim()} onClick=${create} />
          </div>
        </div>
      </${Card}>
      ${!p
        ? html`<${Card}><${Empty} icon="user" title="No driver yet" text="Create a driver to start a personnel file." /></${Card}>`
        : html`
          <${Card}>
            <div class="row" style="gap:18px">
              <div class="avatar" style="width:64px;height:64px;font-size:26px">${p.name[0]?.toUpperCase()}</div>
              <div class="grow">
                <h2 style="font-size:22px">${p.name}${!p.exists ? html` <span class="muted small">${t('(no personnel file yet)')}</span>` : ''}</h2>
                <div class="muted" style="margin:2px 0 10px">${t('Level {n}', { n: p.level })} - ${t('{xp} XP, {left} to level {next}', { xp: p.xp, left: Math.max(0, p.next_level_xp - p.xp), next: p.level + 1 })}</div>
                <div class="progress" style="max-width:420px"><div style=${`width:${into * 100}%`}></div></div>
              </div>
            </div>
          </${Card}>
          <div class="stats">${tiles.map(([icon, v, label]) => html`<div class="stat"><div class="row muted" style="gap:6px;margin-bottom:6px"><${Icon} name=${icon} size=${16} /><span>${t(label)}</span></div><b>${v}</b></div>`)}</div>
          <${Card} title="Recent runs">
            ${p.sessions.length === 0
              ? html`<${Empty} icon="history" text="No runs yet. Drive a duty and it shows up here." />`
              : html`<table class="table">
                  <thead><tr><th>${t('When')}</th><th>${t('Duty')}</th><th>${t('Bus')}</th><th>${t('Time')}</th><th>${t('Distance')}</th><th>${t('Stops')}</th><th>${t('Tickets')}</th><th>${t('Crashes')}</th></tr></thead>
                  <tbody>${p.sessions.slice(0, 50).map((x) => html`<tr>
                    <td class="muted">${ago(x.time)}</td>
                    <td>${x.line ? html`<span class="line-badge">${x.line}</span> ${x.tour ? `/ ${x.tour}` : ''} <span class="muted">${mapName(x.map)}</span>` : html`${t('Free drive')} <span class="muted">${mapName(x.map)}</span>`}</td>
                    <td class="ellipsis" style="max-width:220px">${busName(x.bus)}</td>
                    <td class="num">${Math.floor(x.seconds / 3600)}:${String(Math.floor((x.seconds % 3600) / 60)).padStart(2, '0')}</td>
                    <td class="num">${(x.metres / 1000).toFixed(1)} km</td>
                    <td class="num">${x.stops}</td><td class="num">${x.tickets}</td><td class="num">${x.crashes}</td>
                  </tr>`)}</tbody>
                </table>`}
          </${Card}>`}
    </div>
    ${confirm}
  `;
}
