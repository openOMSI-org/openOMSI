"""Fifth pass: layout leftovers in one-line literals and calls, buffer slices, features, polls."""
import re, sys, glob

def process(path):
    s = open(path).read()
    o = s
    # one-line struct literals and calls
    s = re.sub(r'\b(?:layout|pl): (?:&?self\.)?[a-z_.]*(?:layout|pl)\b,\s*', '', s)
    s = re.sub(r',\s*\b(?:layout|pl): (?:&?self\.)?[a-z_.]*(?:layout|pl)\b(?=\s*[,}])', '', s)
    s = re.sub(r'(\b[A-Z][A-Za-z]* \{ )(?:layout|pl), ', r'\1', s)
    s = re.sub(r'(\b[A-Z][A-Za-z]* \{[^}\n]*?), (?:layout|pl)(?=[,} ])', r'\1', s)
    s = re.sub(r',\s*&?(?:[a-z_]+\.)?(?:camera_layout|probe_layout|material_layout|shadow_layout)(?=\s*[,)])', '', s)
    # buffer slices
    s = re.sub(r'\.set_vertex_buffer\(([^,()]+), &?([A-Za-z_][A-Za-z_0-9.\[\]()]*?)\.slice\(\.\.\)\)', r'.set_vertex_buffer(\1, &\2, 0)', s)
    s = re.sub(r'\.set_vertex_buffer\(([^,()]+), &?([A-Za-z_][A-Za-z_0-9.\[\]()]*?)\.slice\(([^.()]+)\.\.\)\)', r'.set_vertex_buffer(\1, &\2, \3)', s)
    s = re.sub(r'\.set_index_buffer\(&?([A-Za-z_][A-Za-z_0-9.\[\]()]*?)\.slice\(\.\.\), ', r'.set_index_buffer(&\1, 0, ', s)
    s = re.sub(r'\.set_index_buffer\(&?([A-Za-z_][A-Za-z_0-9.\[\]()]*?)\.slice\(([^.()]+)\.\.\), ', r'.set_index_buffer(&\1, \2, ', s)
    # texture views' textures in copy infos
    s = re.sub(r'texture: ([a-z_][a-z_0-9.]*)\.texture\(\),', r'texture: &\1.texture(),', s)
    # features
    s = re.sub(r'([a-z_][a-z_0-9.]*(?:\(\))?)\.features\(\)\s*\.contains\(mx::Features::TEXTURE_COMPRESSION_BC\)', r'\1.supports_bc()', s)
    s = re.sub(r'([a-z_][a-z_0-9.]*(?:\(\))?)\.features\(\)\s*\.contains\(mx::Features::CLEAR_TEXTURE\)', 'true', s)
    s = re.sub(r'([a-z_][a-z_0-9.]*(?:\(\))?)\.features\(\)\s*\.contains\(mx::Features::TIMESTAMP_QUERY\)', r'\1.supports_timestamps()', s)
    # polls
    s = s.replace('.poll(mx::PollType::Poll)', '.poll()')
    s = s.replace('.poll(mx::PollType::wait_indefinitely())', '.wait_idle()')
    s = re.sub(r'\.poll\(mx::PollType::Wait \{ submission_index: Some\(([^)]*)\), timeout: None \}\)', r'.wait(\1)', s)
    s = re.sub(r'\.poll\(mx::PollType::Wait \{ submission_index: None, timeout: None \}\)', '.wait_idle()', s)
    # destroy: mX frees an object when its last handle goes
    s = re.sub(r'^[ \t]*[A-Za-z_][A-Za-z_0-9.]*\.destroy\(\);[ \t]*\n', '', s, flags=re.M)
    if s != o:
        open(path, 'w').write(s)
        return True
    return False

if __name__ == '__main__':
    print(sum(process(p) for r in sys.argv[1:] for p in glob.glob(r + '/**/*.rs', recursive=True)), 'files')
