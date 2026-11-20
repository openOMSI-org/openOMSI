// AI Lists: reading, saving and resetting one map's lists. Kept apart from the page (no
// browser parts) so that the order of the answers can be tested: an answer that is not for the
// latest request is dropped, and nothing is edited while a save or a reset is under way (its
// answer would replace the edits made in the meantime).

const clone = (x) => JSON.parse(JSON.stringify(x));

/** Whether the lists unsaved changes were made on differ from what was read. */
export const dirty = (a) => JSON.stringify(a.groups) !== JSON.stringify(a.saved) || JSON.stringify(a.rules) !== JSON.stringify(a.savedRules);

/** Whether the lists of `map` on `date` are to be read now: they are not already read or being read,
 * no save is under way, and there are no unsaved changes (a date changed on the Drive page must not
 * replace them: the lists stay those of the date they were read on, the one a save compares with). */
export const shouldRead = (a, map, date) => !!map && a.loaded !== `${map}|${date}` && a.loading !== `${map}|${date}` && !a.busy && !dirty(a);

// (one type at a time: its buses share its repaints, so the paint chosen exists on all of them)
export const tickedRows = (ticked, at, n) => (ticked && ticked.at === at && ticked.n === n ? ticked.rows : []);

export function tickRow(ticked, at, n, i) {
  const rows = tickedRows(ticked, at, n);
  const next = rows.includes(i) ? rows.filter((r) => r !== i) : [...rows, i].sort((a, b) => a - b);
  return next.length ? { at, n, rows: next } : null;
}

export function tickAll(ticked, at, n) {
  return tickedRows(ticked, at, n).length === n || !n ? null : { at, n, rows: [...Array(n).keys()] };
}

export function sharedPaint(entries, rows) {
  const paints = new Set(rows.map((i) => entries[i]?.paint || ''));
  return paints.size === 1 ? [...paints][0] : null;
}

const shape = (groups) =>
  JSON.stringify((groups || []).map((g) => [g.name, g.depot, (g.typgroups || []).map((tg) => [String(tg.file || '').replace(/\\/g, '/').toLowerCase(), tg.entries?.length || 0])]));

export function paintRows(entries, rows, paint) {
  for (const i of rows) if (entries[i]) entries[i].paint = paint;
}

/** `io`: `call(cmd, args)`, `get()` the page's state, `up(patch)` to change it, `toast(text, kind)`
 * and `t(text)`. */
export function listsModel({ call, get, up, toast, t }) {
  let latest = 0;

  async function read(map, date) {
    const mine = ++latest;
    const key = `${map}|${date}`;
    up({ map, loaded: null, loading: key, data: null, error: null, ticked: null });
    try {
      const data = await call('ai_lists', { map, date });
      if (mine !== latest) return;
      const groups = data.groups.map(({ random, modified, ...g }) => g);
      const rules = data.lines.filter((l) => l.rule).map((l) => l.rule);
      up({ loaded: key, loading: null, data, groups: clone(groups), saved: clone(groups), rules: clone(rules), savedRules: clone(rules), pick: null, ticked: null });
    } catch (e) {
      if (mine !== latest) return;
      up({ loaded: key, loading: null, error: String(e.message || e) });
    }
  }

  /** Run `command` for the map shown, then read it again; edits wait until it is done. */
  async function work(command, args, done) {
    const a = get();
    if (a.busy) return;
    up({ busy: true });
    try {
      const r = await call(command, args(a));
      toast(done(r), 'ok');
      await read(a.map, a.date);
    } catch (e) {
      toast(String(e.message || e), 'error');
    }
    up({ busy: false });
  }

  const save = () =>
    work(
      'ai_lists_save',
      (a) => ({ map: a.map, date: a.date, groups: a.groups, rules: a.rules }),
      (r) => (r.groups + r.rules > 0 ? t('Saved. The next game you start uses these lists.') : t('Saved: the map has its own lists again.')),
    );

  const reset = () => work('ai_lists_reset', (a) => ({ map: a.map }), () => t('The map has its own lists again.'));

  /** A change made by the player: refused while a save or a reset is under way. */
  const change = (patch) => {
    const a = get();
    if (a.busy) return false;
    const p = typeof patch === 'function' ? patch(a) : patch;
    // (once the rows change, the rows ticked are no longer the buses ticked)
    up(p.groups && a.ticked && shape(p.groups) !== shape(a.groups) ? { ...p, ticked: null } : p);
    return true;
  };

  return { read, save, reset, change };
}
