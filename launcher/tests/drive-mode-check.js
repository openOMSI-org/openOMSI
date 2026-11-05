import { html, render } from '../ui/vendor/preact-htm.js';
import { DriveMode } from '../ui/js/pages/drive-mode.js';
import { MultiplayerPage } from '../ui/js/pages/multiplayer.js';
import { S, set } from '../ui/js/store.js';
import { CHOICE_DEFAULTS } from '../ui/js/data.js';

export async function run(root, Component = DriveMode) {
  const saved = { choice: S.choice, poll: S.poll, page: S.page, driveBeforeJoin: S.driveBeforeJoin, maps: S.maps };
  const sleep = () => new Promise((r) => setTimeout(r, 60));
  const assert = (ok, message) => { if (!ok) throw new Error(message); };
  const report = [];
  try {
    set({ choice: { ...CHOICE_DEFAULTS, lan_mode: 'off', map: 'maps/Test/global.cfg', bus: 'vehicles/Test.bus', paint: 'custom', time: 615 }, poll: { instances: [] } });
    const before = { ...S.choice };
    render(html`<${Component} />`, root);
    await sleep();
    assert(root.querySelectorAll('.drive-mode-option > .icon').length === 2, 'Both play modes must have a recognizable icon');
    assert(root.querySelector('.drive-mode-divider[aria-hidden="true"]'), 'Join must be visually separated from mode selection');
    const radioStyle = getComputedStyle(root.querySelector('input[value="off"]'));
    assert(radioStyle.position === 'absolute' && radioStyle.clipPath !== 'none', 'Native radios must remain accessible without visible form circles');
    const optionsBox = root.querySelector('.drive-mode-options').getBoundingClientRect();
    const joinBox = root.querySelector('.drive-mode-join').getBoundingClientRect();
    if (joinBox.top < optionsBox.bottom) assert(joinBox.left - optionsBox.right < 50, 'Join must stay grouped beside the driving modes');
    report.push('PASS compact toolbar has icons, accessible radios and grouped Join action');
    root.querySelector('input[value="host"]').click();
    await sleep();
    assert(S.choice.lan_mode === 'host', 'Host mode must be selected');
    assert(root.innerText.includes('As the host, your game receives your players\' driver names, network addresses, chat and voice.'), 'The host data notice must be visible while Invite friends is selected');
    root.querySelector('input[value="off"]').click();
    await sleep();
    assert(Object.keys(before).every((k) => JSON.stringify(before[k]) === JSON.stringify(S.choice[k])), 'Switching play mode must retain the duty configuration');
    assert(root.querySelectorAll('input[type="radio"]:checked').length === 1, 'Exactly one mode must be visibly selected');
    report.push('PASS play mode preserves map, bus, paint and duty settings');
    set({ poll: { instances: [{ running: false, lan_status: { role: 'host', code: 'STOPPED' } }, { running: true, lan_status: { role: 'host', code: 'OMSI-LIVE', tunnel: false } }] } });
    await sleep();
    assert(root.textContent.includes('OMSI-LIVE') && !root.textContent.includes('STOPPED'), 'Only running hosts provide invite codes');
    assert(root.textContent.includes('Getting ready for friends on the internet...'), 'Pending tunnel must not be reported ready');
    set({ choice: { ...S.choice, lan_mode: 'off' } });
    await sleep();
    assert(root.textContent.includes('As the host, your game receives your players\' driver names, network addresses, chat and voice.'), 'The host data notice must remain visible while a hosted session is running');
    set({ poll: { instances: [{ running: true, lan_status: { role: 'host' } }] } });
    await sleep();
    assert(root.innerText.includes('As the host, your game receives'), 'A running host must show the data notice even before its code is available');
    report.push('PASS live code appears on Drive with truthful internet readiness');
    for (const hosting of [false, true]) {
      set({ choice: { ...S.choice, lan_mode: hosting ? 'host' : 'off' }, poll: { instances: hosting ? [{ running: true, lan_status: { role: 'host', code: 'OMSI-LIVE', tunnel: true } }] : [] } });
      await sleep();
      const choice = JSON.stringify(S.choice);
      const join = root.querySelector('button.drive-mode-join');
      assert(join && join.textContent.includes('Join friends'), 'Join friends must be a prominent standalone action');
      join.click();
      await sleep();
      assert(S.page === 'drive' && root.querySelector('[role="dialog"]'), 'Join must open a dialog on Drive');
      assert(root.querySelector('#multiplayer-code'), 'Join must show the code form even when hosting');
      assert(!root.querySelector('[role="dialog"]').textContent.includes('Use a server address'), 'Friend dialog must not show addresses');
      const dialog = root.querySelector('[role="dialog"]');
      const privacy = 'Sessions and servers are run by players, not by openOMSI.';
      assert(dialog.innerText.includes(privacy), 'The player-operated server and data notice must be visible before confirming');
      assert(![...dialog.querySelectorAll('details')].some((details) => details.textContent.includes(privacy)), 'The server and data notice must not be hidden in collapsed help');
      assert(JSON.stringify(S.choice) === choice, 'Opening Join must retain all duty settings');
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await sleep();
      assert(!root.querySelector('[role="dialog"]'), 'Escape must close Join');
    }
    report.push('PASS prominent Join opens a code-only dialog in solo and host modes');
    set({ choice: { ...S.choice, lan_mode: 'join', lan_map: 'maps/Remote/global.cfg' }, maps: [] });
    await sleep();
    assert(root.textContent.includes('Choose your bus and press Join and drive') && !root.querySelector('input[value="host"]'), 'Joining retains the existing selected-session workflow');
    assert(root.textContent.includes('Remote') && root.textContent.includes('it is fetched from the server on joining'), 'The Drive join notice must warn when the host map is not installed');
    set({ maps: [{ file: 'MAPS\\Remote\\global.cfg' }] });
    await sleep();
    assert(!root.textContent.includes('it is fetched from the server on joining'), 'Installed host maps must not show the missing-map warning');
    report.push('PASS joining keeps its existing confirmation and cancel actions');
    return report.join('\n');
  } finally {
    render(null, root);
    set(saved);
  }
}

export async function hostLaunchStaysOnDrive() {
  const { choose, start, pollNow } = await import('../ui/js/data.js');
  const saved = { choice: S.choice, page: S.page, poll: S.poll, gameStarted: S.gameStarted, toasts: S.toasts };
  const originalFetch = window.fetch;
  let launched;
  window.fetch = async (url, ...args) => {
    const cmd = /\/mock\/([^/.]+)\.json/.exec(String(url))?.[1];
    if (cmd === 'launch') { launched = true; return Response.json({ others: 0 }); }
    if (cmd === 'poll') return Response.json({ stamp: '', jobs: [], instances: [{ running: true, lan_status: { role: 'host', code: 'OMSI-LAUNCHED', tunnel: true } }] });
    return originalFetch(url, ...args);
  };
  try {
    choose({ lan_mode: 'host', lan_server: null });
    set({ page: 'drive' });
    const result = await start();
    const hasCode = () => document.querySelector('.drive-mode')?.textContent.includes('OMSI-LAUNCHED');
    const deadline = Date.now() + 3000;
    // A poll already in flight makes start() skip its poll; wait for a fresh answer and redraw.
    while (!hasCode() && Date.now() < deadline) {
      await pollNow();
      await new Promise((r) => setTimeout(r, 25));
    }
    if (!launched || !result || S.page !== 'drive') throw new Error('Hosting must launch successfully and remain on Drive');
    if (!hasCode()) throw new Error('The launched host code must appear on the same Drive page');
    return 'PASS actual host launch stays on Drive and shows its polled session code';
  } finally {
    window.fetch = originalFetch;
    set(saved);
  }
}

export async function mobileInvitations(root) {
  const { MobileApp } = await import('../ui/js/mobile/mobile.js');
  const phone = document.documentElement.classList.contains('phone');
  const height = root.style.height;
  document.documentElement.classList.add('phone');
  root.style.height = '600px';
  try { return (await run(root, MobileApp)).replaceAll('PASS ', 'PASS phone '); }
  finally {
    document.documentElement.classList.toggle('phone', phone);
    root.style.height = height;
  }
}
