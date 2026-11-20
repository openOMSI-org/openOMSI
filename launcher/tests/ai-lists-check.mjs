// AI Lists: the order of the answers (launcher/ui/js/pages/ai-lists-model.js), no browser needed:
//   node launcher/tests/ai-lists-check.mjs
import assert from 'node:assert/strict';
import { listsModel, shouldRead, tickedRows, tickRow, tickAll, sharedPaint, paintRows } from '../ui/js/pages/ai-lists-model.js';

const lists = (map, weight = 1) => ({
  groups: [{ name: map, depot: false, vehicles: [{ file: `Vehicles\\${map}.bus`, weight }], typgroups: [], random: true, modified: false }],
  lines: [],
});

/** A page with a `call` whose answers are held until the test lets them go. */
function page(answers) {
  const state = { map: '', date: 'd', busy: false, groups: [], saved: [], rules: [], savedRules: [], loaded: null, data: null, error: null };
  const held = [];
  const toasts = [];
  const model = listsModel({
    call: (cmd, args) => new Promise((resolve, reject) => held.push({ cmd, args, resolve: () => resolve(answers(cmd, args)), reject })),
    get: () => state,
    up: (p) => Object.assign(state, p),
    toast: (text, kind) => toasts.push([text, kind]),
    t: (x) => x,
  });
  const tick = () => new Promise((r) => setTimeout(r, 0));
  return { state, held, toasts, model, tick };
}

// 1. Edits made while a save is pending are refused, so the answer cannot replace them.
{
  const p = page((cmd, a) => (cmd === 'ai_lists' ? lists(a.map) : { groups: 1, rules: 0 }));
  const first = p.model.read('A', 'd');
  p.held.shift().resolve();
  await first;
  assert.equal(p.state.loaded, 'A|d');
  p.model.change({ groups: [{ ...p.state.groups[0], vehicles: [{ file: 'x', weight: 1 }] }] });
  const sending = p.model.save();
  await p.tick();
  assert.equal(p.state.busy, true);
  assert.equal(p.model.change({ groups: [] }), false, 'an edit during the save is refused');
  assert.equal(p.state.groups.length, 1);
  p.held.shift().resolve(); // the save
  await p.tick();
  p.held.shift().resolve(); // the read after it
  await sending;
  assert.equal(p.state.busy, false);
  assert.equal(p.model.change({ groups: [] }), true, 'edits are open again');
  assert.deepEqual(p.toasts.map((x) => x[1]), ['ok']);
}

// 2. A second save or reset while one is under way does nothing.
{
  const p = page(() => ({ groups: 0, rules: 0 }));
  const first = p.model.save();
  await p.tick();
  await p.model.reset();
  assert.equal(p.held.length, 1, 'only the first request was sent');
  p.held.shift().resolve();
  await p.tick();
  p.held.shift()?.resolve();
  await first.catch(() => {});
}

// 3. Of two reads, the latest wins whatever the order they come back in.
{
  const p = page((cmd, a) => lists(a.map));
  const a = p.model.read('A', 'd');
  const b = p.model.read('B', 'd');
  p.held[1].resolve(); // B first
  await b;
  p.held[0].resolve(); // A late
  await a;
  assert.equal(p.state.map, 'B');
  assert.equal(p.state.loaded, 'B|d');
  assert.equal(p.state.groups[0].name, 'B', 'the lists are those of the map shown');
}

// 4. A late failure of an old read does not show as the new map's error.
{
  const p = page((cmd, a) => lists(a.map));
  const a = p.model.read('A', 'd');
  const b = p.model.read('B', 'd');
  p.held[1].resolve();
  await b;
  p.held[0].reject(new Error('A broke'));
  await a;
  assert.equal(p.state.error, null);
  assert.equal(p.state.loaded, 'B|d');
}

// 5. The first read fails, nothing newer: the error shows and the map counts as read (no retry loop).
{
  const p = page(() => ({}));
  const a = p.model.read('A', 'd');
  p.held[0].reject(new Error('no map'));
  await a;
  assert.equal(p.state.error, 'no map');
  assert.equal(p.state.loaded, 'A|d');
  assert.equal(p.state.loading, null);
}

// 6. Unsaved edits are not replaced when the date changes: no automatic re-read while there are any.
{
  const p = page((cmd, a) => lists(a.map));
  const first = p.model.read('A', 'd1');
  p.held.shift().resolve();
  await first;
  assert.equal(shouldRead(p.state, 'A', 'd1'), false, 'already read');
  assert.equal(shouldRead(p.state, 'A', 'd2'), true, 'a new date is read while nothing is changed');
  p.model.change({ groups: [{ ...p.state.groups[0], vehicles: [{ file: 'x', weight: 5 }] }] });
  assert.equal(shouldRead(p.state, 'A', 'd2'), false, 'not over unsaved edits');
  p.model.change({ groups: p.state.saved }); // discarded
  assert.equal(shouldRead(p.state, 'A', 'd2'), true, 'read again once they are gone');
  p.state.busy = true;
  assert.equal(shouldRead(p.state, 'A', 'd2'), false, 'not while a save is under way');
}

// 7. Depot buses ticked to be given one paint: one type at a time, lapsing when its rows change.
{
  const A = 'map|d:depot|0|vehicles\\man\\a.bus';
  const B = 'map|d:depot|1|vehicles\\man\\b.bus';
  let ticked = tickRow(null, A, 4, 2);
  ticked = tickRow(ticked, A, 4, 0);
  assert.deepEqual(tickedRows(ticked, A, 4), [0, 2], 'rows in order');
  assert.deepEqual(tickedRows(ticked, A, 5), [], 'a bus added: the ticks lapse');
  assert.deepEqual(tickedRows(ticked, B, 4), [], 'not shown on another type');
  assert.deepEqual(tickedRows(tickRow(ticked, B, 3, 1), A, 4), [], 'ticking another type starts again there');
  assert.equal(tickRow(tickRow(null, A, 4, 1), A, 4, 1), null, 'unticking the last one leaves none');
  assert.deepEqual(tickedRows(tickAll(ticked, A, 4), A, 4), [0, 1, 2, 3], 'select all');
  assert.equal(tickAll(tickAll(null, A, 4), A, 4), null, 'select all again: none');
  assert.equal(tickAll(null, A, 0), null, 'a type without buses has nothing to tick');

  const entries = [{ paint: 'red' }, { paint: '' }, { paint: 'red' }, { paint: 'blue' }];
  assert.equal(sharedPaint(entries, [0, 2]), 'red');
  assert.equal(sharedPaint(entries, [0, 1]), null, 'different paints');
  paintRows(entries, [0, 1, 3], 'green');
  assert.deepEqual(entries.map((e) => e.paint), ['green', 'green', 'red', 'green'], 'only the ticked rows');
  paintRows(entries, [0], '');
  assert.equal(entries[0].paint, '', "back to the bus's own paint");
}

// 8. The ticks are cleared, not only hidden, by an edit of the rows and by reading the lists
//    again: a bus removed and another added, or a type replaced and put back, must not tick
//    the rows again (#4021 review).
{
  const depot = (file, n) => [{ name: 'D', depot: true, vehicles: [], typgroups: [{ file, entries: [...Array(n)].map((_, i) => ({ number: String(i + 1), registration: '', paint: '' })) }] }];
  const A = 'map|d:D|0|vehicles\\man\\a.bus';
  const p = page((cmd) => (cmd === 'ai_lists' ? { groups: depot('Vehicles\\MAN\\a.bus', 3), lines: [] } : { groups: 1, rules: 0 }));
  const reading = p.model.read('map', 'd');
  p.held.shift().resolve();
  await reading;
  const fresh = () => (p.state.ticked = tickRow(tickRow(null, A, 3, 0), A, 3, 2));

  fresh();
  p.model.change({ groups: depot('Vehicles\\MAN\\a.bus', 2) }); // a bus removed
  assert.equal(p.state.ticked, null, 'a bus removed clears the ticks');
  p.model.change({ groups: depot('Vehicles\\MAN\\a.bus', 3) }); // another added
  assert.deepEqual(tickedRows(p.state.ticked, A, 3), [], 'they do not come back on the new rows');

  fresh();
  p.model.change({ groups: depot('Vehicles\\MAN\\b.bus', 3) }); // the type replaced
  p.model.change({ groups: depot('Vehicles\\MAN\\a.bus', 3) }); // and put back
  assert.deepEqual(tickedRows(p.state.ticked, A, 3), [], 'a type replaced and put back has no ticks');

  fresh();
  const painted = depot('Vehicles\\MAN\\a.bus', 3);
  paintRows(painted[0].typgroups[0].entries, [0, 2], 'red');
  painted[0].typgroups[0].entries[1].number = '42';
  p.model.change({ groups: painted });
  assert.deepEqual(tickedRows(p.state.ticked, A, 3), [0, 2], 'a paint or a number changed keeps the ticks');

  const again = p.model.read('map', 'd');
  assert.equal(p.state.ticked, null, 'reading the lists again (reload, save, reset) clears the ticks');
  fresh();
  p.held.shift().resolve();
  await again;
  assert.equal(p.state.ticked, null, 'ticks made while they were read are cleared with the new lists');

  fresh();
  const resetting = p.model.reset();
  await p.tick();
  p.held.shift().resolve(); // the reset
  await p.tick();
  p.held.shift().resolve(); // the read after it
  await resetting;
  assert.equal(p.state.ticked, null, 'a reset clears the ticks');
}

console.log('ai-lists-check: ok');
