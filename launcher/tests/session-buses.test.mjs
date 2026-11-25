import assert from 'node:assert/strict';
import test from 'node:test';

globalThis.window = globalThis.window || {};
const { duty, CHOICE_DEFAULTS } = await import('../ui/js/data.js');
const { S } = await import('../ui/js/store.js');

test('duty includes session_buses when hosting a LAN session', async () => {
  S.choice = {
    ...CHOICE_DEFAULTS,
    lan_mode: 'host',
    session_buses: 'MAN_SD200; Vehicles/Solaris/solaris.bus',
  };
  const d = await duty();
  assert.equal(d.lan, 'host');
  assert.equal(d.session_buses, 'MAN_SD200; Vehicles/Solaris/solaris.bus');
});

test('duty omits session_buses when lan_mode is off or join or empty', async () => {
  S.choice = {
    ...CHOICE_DEFAULTS,
    lan_mode: 'off',
    session_buses: 'MAN_SD200',
  };
  assert.equal((await duty()).session_buses, null);

  S.choice = {
    ...CHOICE_DEFAULTS,
    lan_mode: 'join',
    lan_addr: '127.0.0.1:4242',
    session_buses: 'MAN_SD200',
  };
  assert.equal((await duty()).session_buses, null);

  S.choice = {
    ...CHOICE_DEFAULTS,
    lan_mode: 'host',
    session_buses: '   ',
  };
  assert.equal((await duty()).session_buses, null);
});
