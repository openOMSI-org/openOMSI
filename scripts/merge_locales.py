"""A git merge driver for the interface texts (`crates/omsi-app/locales/*.yml`): a three-way
merge by text and language, so that the project's new texts and the fork's translations meet
without a conflict (git's line merge saw a conflict wherever both added texts nearby - 208 of
them in app.yml at openOMSI 0.2.27).

    git config merge.omsihub-locales.driver "python scripts/merge_locales.py %O %A %B"

with `.gitattributes`: `crates/omsi-app/locales/*.yml merge=omsihub-locales`. The result goes
into %A (ours). Per text and language: a side that changed it wins; both changed it - the
fork's own languages keep the fork's, the others take the project's; a text the project took
out goes (its code no longer asks for it). Files that are not in the `_version: 2` shape
(a key line, its languages indented under it) are left to git: the driver then fails, which
git takes as a conflict.
"""
import re
import sys

KEY = re.compile(r'^(".*"):\s*$')
LANG = re.compile(r'^  ([a-z][a-z-]*): (".*")\s*$')
OTHER = re.compile(r'^(#.*|\s*|_version: \d+\s*)$')
FORK_LANGS = {'nl', 'de', 'fr', 'ru', 'uk', 'pl'}


def parse(text):
    head, entries, comments, key = [], {}, [], None
    for line in text.replace('\r\n', '\n').split('\n'):
        m = KEY.match(line)
        if m:
            key = m.group(1)
            entries.setdefault(key, {'comments': comments, 'langs': {}})
            comments = []
            continue
        m = LANG.match(line)
        if m and key is not None:
            entries[key]['langs'][m.group(1)] = m.group(2)
            continue
        if not OTHER.match(line):
            raise ValueError('not a texts file: ' + line[:60])
        if key is None:
            head.append(line)
        elif line.startswith('#'):
            comments.append(line)
    return head, entries


def merge_langs(b, o, t):
    out = {}
    for lang in list(o) + [x for x in t if x not in o] + [x for x in b if x not in o and x not in t]:
        bv, ov, tv = b.get(lang), o.get(lang), t.get(lang)
        if ov == tv:
            v = ov
        elif ov == bv:
            v = tv
        elif tv == bv:
            v = ov
        elif ov is None or tv is None:
            v = ov if tv is None else tv
        else:
            v = ov if lang in FORK_LANGS else tv
        if v is not None:
            out[lang] = v
    return out


def merge(base, ours, theirs):
    _, B = parse(base)
    _, O = parse(ours)
    head, T = parse(theirs)
    lines = list(head)
    while lines and lines[-1] == '':
        lines.pop()
    lines.append('')
    for k in list(O) + [k for k in T if k not in O]:
        in_b, in_o, in_t = k in B, k in O, k in T
        b = B[k]['langs'] if in_b else {}
        if in_b and not in_t:
            continue
        if in_b and not in_o and T[k]['langs'] == b:
            continue
        langs = merge_langs(b, O[k]['langs'] if in_o else {}, T[k]['langs'] if in_t else {})
        if not langs:
            continue
        lines.extend(O[k]['comments'] if in_o else T[k]['comments'])
        lines.append(k + ':')
        lines.extend('  {}: {}'.format(lang, v) for lang, v in langs.items())
    return '\n'.join(lines) + '\n'


def main():
    base_p, ours_p, theirs_p = sys.argv[1:4]
    out_p = sys.argv[4] if len(sys.argv) > 4 else ours_p
    rd = lambda p: open(p, encoding='utf-8').read()
    try:
        text = merge(rd(base_p), rd(ours_p), rd(theirs_p))
    except ValueError as e:
        print('merge_locales: ' + str(e), file=sys.stderr)
        sys.exit(1)
    open(out_p, 'w', encoding='utf-8', newline='\n').write(text)


if __name__ == '__main__':
    main()
