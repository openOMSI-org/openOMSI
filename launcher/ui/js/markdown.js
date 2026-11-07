// The small part of Markdown that release notes use (headings, lists, quotes, paragraphs;
// **bold**, *italic*, `code` and [links](https://…)), read into plain data. Nothing of the
// text becomes HTML: changelog.js draws the pieces as elements, so a note cannot run a
// script or load anything, and a link opens in the browser only when it is https.

const ITEM = /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/;
const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const RULE = /^\s*(?:-\s*){3,}$|^\s*(?:\*\s*){3,}$|^\s*(?:_\s*){3,}$/;

/** The text's blocks: `{ kind: 'heading', level, text }`, `{ kind: 'list', items: [{ depth, text }] }`,
 * `{ kind: 'quote', text }` and `{ kind: 'paragraph', text }` (text still with its inline marks). */
export function blocks(source) {
  const out = [];
  let open = null;
  const close = () => { open = null; };
  const text = String(source || '').replace(/\r\n?/g, '\n').replace(/<!--[\s\S]*?-->/g, ' ');
  for (const line of text.split('\n')) {
    if (!line.trim()) { close(); continue; }
    if (RULE.test(line)) { close(); continue; }
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

const INLINE = /\*\*(.+?)\*\*|__(.+?)__|`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)|(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])|(?<!\w)_(?!\s)(.+?)(?<!\s)_(?!\w)/g;

/** A line's pieces: `{ kind: 'text' | 'bold' | 'italic' | 'code', text }` and
 * `{ kind: 'link', text, url }` (`url` only when it is https, else the link is plain text). */
export function inline(source) {
  const s = String(source || '');
  const out = [];
  let at = 0;
  for (const m of s.matchAll(INLINE)) {
    if (m.index > at) out.push({ kind: 'text', text: s.slice(at, m.index) });
    if (m[1] !== undefined || m[2] !== undefined) out.push({ kind: 'bold', text: m[1] ?? m[2] });
    else if (m[3] !== undefined) out.push({ kind: 'code', text: m[3] });
    else if (m[4] !== undefined) out.push(/^https:\/\/[^\s]+$/i.test(m[5]) ? { kind: 'link', text: m[4], url: m[5] } : { kind: 'text', text: m[4] });
    else out.push({ kind: 'italic', text: m[6] ?? m[7] });
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
