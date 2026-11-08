// The small part of Markdown that release notes use, read into plain data. Nothing of the
// text becomes HTML: changelog.js draws the pieces as elements, so a note cannot run a
// script or load anything. Links use HTTPS; images are limited to GitHub release assets.

const ITEM = /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/;
const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE = /^\s*(?:-\s*){3,}$|^\s*(?:\*\s*){3,}$|^\s*(?:_\s*){3,}$/;
const INLINE = /<img\b[^>]*>|!\[([^\]]*)\]\(([^)\s]+)\)|\*\*(.+?)\*\*|__(.+?)__|`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)|(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])|(?<!\w)_(?!\s)(.+?)(?<!\s)_(?!\w)/gi;

function tableCells(line) {
  const cells = [];
  let cell = '';
  let inCode = false;
  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (char === '\\' && line[i + 1] === '|') {
      cell += '|';
      i++;
    } else if (char === '`') {
      inCode = !inCode;
      cell += char;
    } else if (char === '|' && !inCode) {
      cells.push(cell.trim());
      cell = '';
    } else {
      cell += char;
    }
  }
  cells.push(cell.trim());
  if (line.trimStart().startsWith('|')) cells.shift();
  if (line.trimEnd().endsWith('|')) cells.pop();
  return cells;
}

function tableAlignment(line) {
  const cells = tableCells(line);
  if (!cells.length || cells.some((cell) => !/^:?-{3,}:?$/.test(cell))) return null;
  return cells.map((cell) => cell.startsWith(':')
    ? cell.endsWith(':') ? 'center' : 'left'
    : cell.endsWith(':') ? 'right' : 'left');
}

function imageUrl(value) {
  if (!/^https:\/\//i.test(value) || /[\s"'<>]/.test(value)) return null;
  try {
    const url = new URL(value);
    const githubAttachment = url.hostname === 'github.com' && url.pathname.startsWith('/user-attachments/assets/');
    const privateImage = url.hostname === 'private-user-images.githubusercontent.com';
    return url.protocol === 'https:' && (githubAttachment || privateImage) && !url.username && !url.password ? url.href : null;
  } catch (error) {
    if (error instanceof TypeError) return null;
    throw error;
  }
}

function htmlImage(source) {
  const tag = /^<img\b([^>]*)\/?>$/i.exec(source);
  if (!tag) return null;
  let rest = tag[1].trim().replace(/\/\s*$/, '').trim();
  const attrs = {};
  while (rest) {
    const match = /^([a-z][\w:-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+))/i.exec(rest);
    if (!match) return null;
    const name = match[1].toLowerCase();
    if (!['src', 'alt', 'width', 'height'].includes(name) || Object.hasOwn(attrs, name)) return null;
    attrs[name] = (match[2] ?? match[3] ?? match[4])
      .replace(/&(?:amp|quot|apos|#39|lt|gt);/gi, (entity) => ({
        '&amp;': '&', '&quot;': '"', '&apos;': "'", '&#39;': "'", '&lt;': '<', '&gt;': '>',
      }[entity.toLowerCase()]));
    rest = rest.slice(match[0].length).trimStart();
  }
  const url = imageUrl(attrs.src || '');
  if (!url || ['width', 'height'].some((key) => attrs[key] !== undefined && !/^[1-9]\d{0,3}$|^10000$/.test(attrs[key]))) return null;
  return { kind: 'image', url, alt: attrs.alt || '', width: attrs.width ? Number(attrs.width) : undefined, height: attrs.height ? Number(attrs.height) : undefined };
}

/** The text's blocks: `{ kind: 'heading', level, text }`, `{ kind: 'list', items: [{ depth, text }] }`,
 * `{ kind: 'quote', text }`, `{ kind: 'table', headers, align, rows }` and
 * `{ kind: 'paragraph', text }` (text still with its inline marks). */
export function blocks(source) {
  const out = [];
  let open = null;
  const close = () => { open = null; };
  const text = String(source || '').replace(/\r\n?/g, '\n').replace(/<!--[\s\S]*?-->/g, ' ');
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim()) { close(); continue; }
    if (RULE.test(line)) { close(); continue; }
    const headers = tableCells(line);
    const align = i + 1 < lines.length ? tableAlignment(lines[i + 1]) : null;
    if (align && align.length === headers.length) {
      close();
      const rows = [];
      while (i + 2 < lines.length && lines[i + 2].includes('|')) {
        const row = tableCells(lines[i + 2]);
        if (row.length !== headers.length) break;
        rows.push(row);
        i++;
      }
      out.push({ kind: 'table', headers, align, rows });
      i++;
      continue;
    }
    const h = HEADING.exec(line);
    if (h) { close(); out.push({ kind: 'heading', level: h[1].length, text: h[2] }); continue; }
    if (/^\s*>/.test(line)) {
      const body = line.replace(/^\s*>\s?/, '');
      if (open?.kind !== 'quote') { open = { kind: 'quote', text: body }; out.push(open); } else open.text += ' ' + body;
      continue;
    }
    const item = ITEM.exec(line);
    if (item) {
      if (open?.kind !== 'list') { open = { kind: 'list', items: [] }; out.push(open); }
      open.items.push({ depth: Math.min(3, Math.floor(item[1].replace(/\t/g, '  ').length / 2)), text: item[2].trim() });
      continue;
    }
    if (open?.kind === 'list' && /^\s/.test(line)) { open.items[open.items.length - 1].text += ' ' + line.trim(); continue; }
    if (open?.kind === 'paragraph' || open?.kind === 'quote') { open.text += ' ' + line.trim(); continue; }
    open = { kind: 'paragraph', text: line.trim() };
    out.push(open);
  }
  return out;
}

/** A line's pieces: text marks, HTTPS links, and HTTPS images (Markdown or allowlisted HTML). */
export function inline(source) {
  const s = String(source || '');
  const out = [];
  let at = 0;
  for (const m of s.matchAll(INLINE)) {
    if (m.index > at) out.push({ kind: 'text', text: s.slice(at, m.index) });
    if (/^<img\b/i.test(m[0])) out.push(htmlImage(m[0]) || { kind: 'text', text: m[0] });
    else if (m[1] !== undefined) {
      const url = imageUrl(m[2]);
      out.push(url ? { kind: 'image', alt: m[1], url, width: undefined, height: undefined } : { kind: 'text', text: m[0] });
    } else if (m[3] !== undefined || m[4] !== undefined) out.push({ kind: 'bold', text: m[3] ?? m[4] });
    else if (m[5] !== undefined) out.push({ kind: 'code', text: m[5] });
    else if (m[6] !== undefined) out.push(/^https:\/\/[^\s]+$/i.test(m[7]) ? { kind: 'link', text: m[6], url: m[7] } : { kind: 'text', text: m[6] });
    else out.push({ kind: 'italic', text: m[8] ?? m[9] });
    at = m.index + m[0].length;
  }
  if (at < s.length) out.push({ kind: 'text', text: s.slice(at) });
  return out;
}

/** Versions as numbers to compare (`0.3.42` > `0.3.9`; a suffix such as `-beta` is left out). */
export function compareVersions(a, b) {
  const pa = String(a || '').split(/[.-]/).map((x) => parseInt(x, 10) || 0);
  const pb = String(b || '').split(/[.-]/).map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d) return d;
  }
  return 0;
}

/** The versions shown open: those since the one whose notes were last seen, up to this one;
 * this one alone when none was seen yet (or after a downgrade). */
export function freshVersions(list, current, seen) {
  const fresh = list.filter((r) => seen && compareVersions(r.version, seen) > 0 && compareVersions(r.version, current) <= 0).map((r) => r.version);
  return fresh.length ? fresh : list.some((r) => r.version === current) ? [current] : list.slice(0, 1).map((r) => r.version);
}
