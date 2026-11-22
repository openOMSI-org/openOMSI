"""Fourth pass: what is left of layout entries, buffer sizes, acceleration structures."""
import re, sys, glob

def end_of(s, i, stop=';'):
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
            if depth == 0 and stop == '}' and c == '}':
                return j + 1
        elif c == ';' and depth == 0 and stop == ';':
            return j + 1
        j += 1
    return None

LAYOUT = re.compile(r'\bmx::(BindGroupLayoutEntry|ShaderStages|BindingType|BufferBindingType|SamplerBindingType|StorageTextureAccess)\b')

def process(path):
    s = open(path).read()
    o = s
    # functions returning layout entries
    while True:
        m = re.search(r'^[ \t]*(?:pub(?:\([a-z]+\))? )?fn [a-z_0-9]+[^{;]*->[^{;]*mx::BindGroupLayoutEntry[^{;]*\{', s, flags=re.M)
        if not m:
            break
        e = end_of(s, s.index('{', m.start()), '}')
        while e < len(s) and s[e] == '\n':
            e += 1
        # doc comments above
        a = m.start()
        while True:
            prev = s.rfind('\n', 0, a - 1)
            line = s[prev + 1:a]
            if line.strip().startswith('///') or line.strip().startswith('#['):
                a = prev + 1
            else:
                break
        s = s[:a] + s[e:]
    # let statements that build layout entries
    i = 0
    while True:
        m = re.search(r'^[ \t]*let (?:mut )?[a-z_0-9]+[^=\n]*= ', s[i:], flags=re.M)
        if not m:
            break
        a = i + m.start()
        e = end_of(s, i + m.end())
        if e is None:
            break
        if LAYOUT.search(s[a:e]):
            while e < len(s) and s[e] == '\n':
                e += 1
                break
            s = s[:a] + s[e:]
            i = a
        else:
            i = e
    s = re.sub(r'mx::BufferSize::new\(([^()]*(?:\([^()]*\))?[^()]*)\)', r'Some(\1)', s)
    s = re.sub(r'([a-z_][a-z_0-9.]*)\.as_binding\(\)', r'mx::BindingResource::AccelerationStructure(&\1)', s)
    s = re.sub(r'^[ \t]*transform_buffer(?:_offset)?: [^\n]*,[ \t]*\n', '', s, flags=re.M)
    s = re.sub(r'mx::BlasGeometries::TriangleGeometries\(', '(', s)
    s = re.sub(r'^[ \t]*usage: None,[ \t]*\n', '', s, flags=re.M)
    if s != o:
        open(path, 'w').write(s)
        return True
    return False

if __name__ == '__main__':
    print(sum(process(p) for r in sys.argv[1:] for p in glob.glob(r + '/**/*.rs', recursive=True)), 'files')
