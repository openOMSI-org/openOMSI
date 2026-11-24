"""Third pass of the port: pipeline stage constants, depth state, inline leftovers."""
import re, sys, glob

def balanced(s, i, open_c='{', close_c='}'):
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

def process(path):
    s = open(path).read()
    o = s
    s = s.replace('create_shader_module(mx::ShaderModuleDescriptor', 'create_shader_module(&mx::ShaderModuleDescriptor')
    # compilation_options: mx::PipelineCompilationOptions { constants: X, .. } -> constants: X
    while True:
        m = re.search(r'compilation_options:\s*mx::PipelineCompilationOptions\s*\{', s)
        if not m:
            break
        e = balanced(s, m.end() - 1)
        body = s[m.end():e - 1]
        c = re.search(r'constants:\s*(.*?)(?:,\s*(?:zero_initialize_workgroup_memory|\.\.)|,?\s*$)', body, flags=re.S)
        rep = f'constants: {c.group(1).strip().rstrip(",")}' if c else 'constants: &[]'
        end = e
        if end < len(s) and s[end] == ',':
            pass
        s = s[:m.start()] + rep + s[end:]
    s = re.sub(r'compilation_options:\s*(?:mx::PipelineCompilationOptions::default\(\)|Default::default\(\)|[a-z_.]*compilation_options[a-z_.()]*),?\s*', '', s)
    # stages without constants
    for kind in ('VertexState', 'FragmentState'):
        i = 0
        while True:
            m = re.search(r'mx::' + kind + r'\s*\{', s[i:])
            if not m:
                break
            a = i + m.end() - 1
            e = balanced(s, a)
            body = s[a:e]
            if 'constants' not in body and '..' not in body:
                s = s[:a + 1] + ' constants: &[],' + s[a + 1:]
                e += len(' constants: &[],')
            i = e
    s = re.sub(r'depth_write_enabled:\s*Some\(([^()]*?)\)', r'depth_write_enabled: \1', s)
    s = re.sub(r'depth_compare:\s*Some\(([^()]*(?:\([^()]*\))?[^()]*?)\)', r'depth_compare: \1', s)
    s = re.sub(r'^[ \t]*cache: [^\n]*,[ \t]*\n', '', s, flags=re.M)
    for f in ('depth_slice', 'multiview_mask', 'multiview'):
        s = re.sub(r',\s*' + f + r':\s*None(?=\s*[,}])', '', s)
        s = re.sub(r'\b' + f + r':\s*None,\s*', '', s)
    s = re.sub(r',\s*mapped_at_creation:\s*false(?=\s*[,}])', '', s)
    s = re.sub(r'\bmapped_at_creation:\s*false,\s*', '', s)
    if s != o:
        open(path, 'w').write(s)
        return True
    return False

if __name__ == '__main__':
    n = sum(process(p) for root in sys.argv[1:] for p in glob.glob(root + '/**/*.rs', recursive=True))
    print(n, 'files')
