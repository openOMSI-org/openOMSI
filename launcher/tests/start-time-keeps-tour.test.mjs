// A start a few minutes before the bus leaves must keep the picked trip (#4334).
import assert from 'node:assert/strict';
import test from 'node:test';

globalThis.window = globalThis.window || {};
globalThis.requestAnimationFrame = globalThis.requestAnimationFrame || (() => 0);
const { CHOICE_DEFAULTS, choose, duty, pickedTrip } = await import('../ui/js/data.js');
const { S } = await import('../ui/js/store.js');

function pickedDuty() {
  S.choice = {
    ...CHOICE_DEFAULTS,
    map: 'maps/Test/global.cfg',
    bus: 'vehicles/Test.bus',
    line: '5',
    tour: '2',
    time: 10 * 60,
    start_trip: ['5', '2', 3, 10 * 60],
    season: 'auto',
    lan_mode: 'off',
  };
  S.lines = {
    key: 'maps/Test/global.cfg|1989-05-30',
    list: [{ name: '5', tours: [{ id: '2', number: '2', trips: [] }] }],
  };
  S.depot = [];
}

test('moving the start time earlier keeps the tour and the whole duty', async () => {
  pickedDuty();
  assert.equal(pickedTrip(), 3);
  assert.equal((await duty()).trip, '3');
  choose({ time: 10 * 60 - 5 });
  assert.equal(S.choice.tour, '2');
  assert.equal(pickedTrip(), 3);
  const d = await duty();
  assert.equal(d.tour, '2');
  assert.equal(d.trip, '3');
  assert.equal(d.whole_tour, true);
  assert.equal(d.time, '09:55');
});

test('picking another tour or line still drops the old trip', () => {
  pickedDuty();
  choose({ tour: '3' });
  assert.equal(pickedTrip(), null);
  pickedDuty();
  choose({ line: '7' });
  assert.equal(pickedTrip(), null);
});
