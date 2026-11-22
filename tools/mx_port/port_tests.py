"""Tests' pass: instances, adapters, error scopes and polls of wgpu to mX's."""
import re, sys, glob

def process(path):
    s = open(path).read()
    o = s
    # a no-op instance
    s = re.sub(r'let mut (\w+) = mx::InstanceDescriptor::new_without_display_handle\(\);\s*\n\s*\1\.backends = mx::Backends::NOOP;\s*\n\s*\1\.backend_options\.noop = mx::NoopBackendOptions \{ enable: true \};\s*\n(\s*)let (\w+) = mx::Instance::new\(\1\);',
               r'\2let \3 = mx::Instance::with_backends(mx::Backends::NOOP);', s)
    s = s.replace('mx::Instance::new(mx::InstanceDescriptor::new_without_display_handle())', 'mx::Instance::new()')
    # adapters and devices are not futures
    s = re.sub(r'pollster::block_on\(\s*([\w.]+)\s*\.request_adapter\(&mx::RequestAdapterOptions::default\(\)\)\s*\)', r'\1.default_adapter()', s)
    s = re.sub(r'pollster::block_on\(\s*([\w.]+)\.request_device\((&[^()]*(?:\([^()]*\))?[^()]*)\)\s*\)', r'\1.request_device(\2)', s)
    # error scopes: the renderer's error watch
    s = re.sub(r'([\w.]+(?:\s*\.\s*renderer)?)\s*\.\s*device\s*\.\s*push_error_scope\(\s*mx::ErrorFilter::\w+\s*\)', r'\1.watch_errors()', s)
    s = re.sub(r'pollster::block_on\(\s*(\w+)\.pop\(\)\s*\)', r'\1.pop()', s)
    # waits
    s = re.sub(r'\.poll\(\s*mx::PollType::Wait\s*\{\s*submission_index:\s*None,\s*timeout:\s*None,?\s*\}\s*\)', '.wait_idle()', s)
    s = re.sub(r'\.poll\(\s*mx::PollType::Wait\s*\{\s*submission_index:\s*Some\(([^()]*)\),\s*timeout:\s*None,?\s*\}\s*\)', r'.wait(\1)', s)
    s = s.replace('.poll(mx::PollType::wait_indefinitely())', '.wait_idle()')
    s = re.sub(r'\bmx::naga\b', 'naga', s)
    s = s.replace('.get_info()', '.info()')
    if s != o:
        open(path, 'w').write(s)
        return True
    return False

if __name__ == '__main__':
    print(sum(process(p) for r in sys.argv[1:] for p in glob.glob(r + '/**/*.rs', recursive=True)), 'files')
