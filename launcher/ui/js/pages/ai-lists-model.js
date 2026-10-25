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

/** `io`: `call(cmd, args)`, `get()` the page's state, `up(patch)` to change it, `toast(text, kind)`
 * and `t(text)`. */
export function listsModel({ call, get, up, toast, t }) {
  let latest = 0;

  async function read(map, date) {
    const mine = ++latest;
    const key = `${map}|${date}`;
    up({ map, loaded: null, loading: key, data: null, error: null });
    try {
      const data = await call('ai_lists', { map, date });
      if (mine !== latest) return;
      const groups = data.groups.map(({ random, modified, ...g }) => g);
      const rules = data.lines.filter((l) => l.rule).map((l) => l.rule);
      up({ loaded: key, loading: null, data, groups: clone(groups), saved: clone(groups), rules: clone(rules), savedRules: clone(rules), pick: null });
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
    if (get().busy) return false;
    up(typeof patch === 'function' ? patch(get()) : patch);
    return true;
  };

  return { read, save, reset, change };
}
