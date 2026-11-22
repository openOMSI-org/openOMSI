import re, sys, glob

def balanced_end(s, i):
    """s[i] == '(' ; index after the matching ')'."""
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
            if depth == 0:
                return j + 1
        j += 1
    return None

FALLIBLE = {
    'create_buffer': 'a buffer',
    'create_buffer_init': 'a buffer',
    'create_texture': 'a texture',
    'create_sampler': 'a sampler',
    'create_shader_module': 'a shader',
    'create_render_pipeline': 'a render pipeline',
    'create_compute_pipeline': 'a compute pipeline',
    'create_view': 'a texture view',
    'create_query_set': 'a query set',
    'create_blas': 'an acceleration structure',
    'write_buffer': 'a queue write',
    'write_texture': 'a queue write',
    'create_tlas': 'an acceleration structure',
}

def process(path):
    s = open(path).read()
    o = s
    # ShaderSource::Wgsl(x) -> &*x  (dropping .into() / Cow::Borrowed)
    out = []
    i = 0
    for m in re.finditer(r'mx::ShaderSource::Wgsl\(', s):
        pass
    while True:
        m = re.search(r'mx::ShaderSource::Wgsl\(', s[i:])
        if not m:
            break
        a = i + m.start()
        p = i + m.end() - 1
        e = balanced_end(s, p)
        arg = s[p + 1 : e - 1].strip()
        arg = re.sub(r'\.into\(\)$', '', arg)
        mm = re.fullmatch(r'(?:std::borrow::)?Cow::(?:Borrowed|Owned)\((.*)\)', arg, flags=re.S)
        if mm:
            arg = mm.group(1)
        rep = f'&*({arg})' if not re.fullmatch(r'[A-Za-z_][A-Za-z_0-9:]*|"[^"]*"', arg) else (arg if arg.startswith('"') else f'&*{arg}')
        s = s[:a] + rep + s[e:]
        i = a + len(rep)
    # create_command_encoder(&mx::CommandEncoderDescriptor { label: X }) -> create_command_encoder(X)
    s = re.sub(r'create_command_encoder\(&mx::CommandEncoderDescriptor \{\s*label:\s*([^}]*?),?\s*\}\)', r'create_command_encoder(\1)', s)
    s = re.sub(r'create_command_encoder\(&mx::CommandEncoderDescriptor::default\(\)\)', 'create_command_encoder(None)', s)
    s = re.sub(r'create_command_encoder\(&Default::default\(\)\)', 'create_command_encoder(None)', s)
    s = s.replace('mx::RenderPassTimestampWrites', 'mx::PassTimestampWrites').replace('mx::ComputePassTimestampWrites', 'mx::PassTimestampWrites')
    # set_bind_group(a, b, &[]) -> set_bind_group(a, b)
    s = re.sub(r'(\.set_bind_group\([^;]*?),\s*&\[\]\s*\)', r'\1)', s)
    # fallible creations: add .expect unless already handled
    for name, what in FALLIBLE.items():
        i = 0
        while True:
            m = re.search(r'\.' + name + r'\(', s[i:])
            if not m:
                break
            p = i + m.end() - 1
            e = balanced_end(s, p)
            if e is None:
                break
            rest = s[e:e + 40]
            if re.match(r'\s*(\?|\.expect|\.unwrap|\.map|\.ok\(|\.map_err|\.unwrap_or|\.context|\.with_context)', rest):
                i = e
                continue
            # skip calls whose Result is matched on
            line_start = s.rfind('\n', 0, i + m.start()) + 1
            if re.search(r'\bmatch\b|if let (?:Ok|Err)', s[line_start:i + m.start()]):
                i = e
                continue
            # skip definitions (`fn create_texture(`)
            if re.search(r'fn\s*$', s[max(0, i + m.start() - 4): i + m.start()]):
                i = e
                continue
            ins = f'.expect("mX: {what}")'
            s = s[:e] + ins + s[e:]
            i = e + len(ins)
    if s != o:
        open(path, 'w').write(s)
        return True
    return False

if __name__ == '__main__':
    n = 0
    for root in sys.argv[1:]:
        for p in glob.glob(root + '/**/*.rs', recursive=True):
            if process(p):
                n += 1
    print(n, 'files')
