"""Mechanical part of openBus's port to mX: drop what mX has no concept of.

- descriptor fields: multiview_mask, depth_slice, compilation_options, cache: None,
  mapped_at_creation: false, pipeline `layout: Some(..)/None`, bind group `layout: &..`
- `let x = <...>.create_bind_group_layout(..);` and `create_pipeline_layout` statements
- `mx::BindGroupLayoutEntry { .. }` array literals left behind are removed with their statement
"""
import re, sys, glob

def find_stmt_end(s, i):
    """From index i (inside a statement), return the index after the terminating ';' at depth 0."""
    depth = 0
    j = i
    in_str = False
    while j < len(s):
        c = s[j]
        if in_str:
            if c == '\\':
                j += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif c in '([{':
            depth += 1
        elif c in ')]}':
            depth -= 1
            if depth < 0:
                return None
        elif c == ';' and depth == 0:
            return j + 1
        j += 1
    return None

FIELD_LINES = [
    r'multiview_mask: [^\n]*,',
    r'multiview: None,',
    r'depth_slice: [^\n]*,',
    r'compilation_options: [^\n]*,',
    r'cache: None,',
    r'mapped_at_creation: false,',
    r'layout: Some\([^\n]*\),',
    r'layout: None,',
    r'layout: &[^\n]*,',
]

def process(path):
    s = open(path).read()
    o = s
    for pat in FIELD_LINES:
        s = re.sub(r'^[ \t]*' + pat + r'[ \t]*\n', '', s, flags=re.M)
    removed = []
    while True:
        m = re.search(r'^[ \t]*let (mut )?([a-z_0-9]+)(: [^=]+)? = [^;]*?\.create_(bind_group|pipeline)_layout\(', s, flags=re.M)
        if not m:
            break
        end = find_stmt_end(s, m.start() + len(m.group(0)) - 1 - 0)
        # find_stmt_end from the '(' : step back to the start of the expression
        end = find_stmt_end(s, s.index('=', m.start()) + 1)
        if end is None:
            break
        # swallow the newline
        if end < len(s) and s[end] == '\n':
            end += 1
        removed.append(m.group(2))
        s = s[: m.start()] + s[end:]
    if s != o:
        open(path, 'w').write(s)
    return removed

if __name__ == '__main__':
    total = 0
    names = {}
    for root in sys.argv[1:]:
        for p in glob.glob(root + '/**/*.rs', recursive=True):
            r = process(p)
            if r:
                names[p] = r
                total += len(r)
    for p, r in names.items():
        print(p, ' '.join(r))
    print('removed', total, 'layout statements')
