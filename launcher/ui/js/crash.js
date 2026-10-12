import { html } from '../vendor/preact-htm.js';
import { S, set, toast, useStore } from './store.js';
import { t } from './i18n.js';
import { call, on } from './api.js';
import { Button, Dialog } from './ui.js';
import { saveSettings } from './data.js';

const REPO = 'https://github.com/openOMSI-org/openOMSI';

export function watchGameEnds() {
  on('game-crashed', (c) => set({ crash: c }));
  on('game-disconnected', (d) => set({ disconnected: d.why }));
}

function report(c) {
  return `openOMSI ${c.version} (${c.os})\n${c.what}\n\n${c.tail}`;
}

function issueUrl(c) {
  const cut = c.tail.indexOf(`\n${c.gap}\n`);
  const [machine, end] = cut >= 0 ? [c.tail.slice(0, cut), c.tail.slice(cut + c.gap.length + 2)] : ['', c.tail];
  const about = machine ? `The computer:\n\`\`\`\n${machine}\n\`\`\`\n\n` : '';
  const body = (lines) => `openOMSI ${c.version} on ${c.os}\n\n\`\`\`\n${c.what}\n\`\`\`\n\n${about}The end of the log:\n\`\`\`\n${lines}\n\`\`\`\n`;
  const lines = end.split('\n');
  let shown = 0;
  while (shown < lines.length && encodeURIComponent(body(lines.slice(lines.length - shown - 1).join('\n'))).length <= 6500) shown++;
  const title = `Crash: ${Array.from(c.what).slice(0, 80).join('')}`;
  return `${REPO}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body(lines.slice(lines.length - shown).join('\n')))}`;
}

async function copy(c) {
  try {
    await navigator.clipboard.writeText(report(c));
    toast(t('The report is copied: paste it into a GitHub issue or a message.'), 'ok');
  } catch (e) {
    toast(e.message, 'error');
  }
}

function hint(c) {
  if (c.lost) {
    return S.boot?.platform === 'windows'
      ? 'The graphics driver stopped the game. Updating the graphics driver usually helps; you can also let the game draw with DirectX 12 instead of Vulkan (the button below, or Settings → Graphics API).'
      : 'The graphics driver stopped the game. Updating the graphics driver usually helps; Settings → Graphics API can switch to OpenGL.';
  }
  if (c.vulkan) return 'The Vulkan graphics driver stopped while compiling shaders. Starting the game again will switch to OpenGL (or change it in Settings → Graphics API).';
  if (c.silent) return 'The system closed the game while it was running, typically because the device ran out of memory (RAM). Lowering texture resolution or reducing AI traffic in Settings helps prevent memory exhaustion.';
  return "Copy the report (the end of the game's log), or open a GitHub issue with it: it tells what went wrong on this computer.";
}

export function GameEndDialogs() {
  const s = useStore();
  if (s.disconnected) {
    const close = () => set({ disconnected: null });
    return html`<${Dialog} title="Disconnected from the server" onClose=${close} footer=${html`<${Button} kind="primary" label="Close" onClick=${close} />`}>
      <div class="stack" style="gap:10px">
        <p class="dim">${t('The server ended your game. Its message:')}</p>
        <p class="selectable"><b>${s.disconnected}</b></p>
      </div>
    </${Dialog}>`;
  }
  const c = s.crash;
  if (!c) return null;
  const close = () => set({ crash: null });
  const dx12 = c.lost && S.boot?.platform === 'windows' && s.settings?.graphics_api !== 'dx12';
  const footer = html`
    ${c.silent
      ? html`<${Button} kind="ghost" icon="sliders" label="Settings" onClick=${() => set({ crash: null, page: 'settings' })} />`
      : html`<${Button} kind="ghost" icon="external" label="Report on GitHub" onClick=${() => (navigator.clipboard?.writeText(report(c)).catch(() => {}), call('open_url', { url: issueUrl(c) }).catch((e) => toast(e.message, 'error')))} />`}
    ${dx12 && html`<${Button} label="Use DirectX 12" onClick=${async () => {
      if (!(await saveSettings({ graphics_api: 'dx12' }))) return;
      close();
      toast(t('The game draws with DirectX 12 from the next start (Settings → Graphics API to change it back).'), 'ok');
    }} />`}
    <${Button} kind="primary" icon="copy" label="Copy report" onClick=${() => copy(c)} />
    <${Button} label="Close" onClick=${close} />`;
  return html`<${Dialog} title=${c.silent ? 'The game was closed by the system' : 'The game closed on an error'} onClose=${close} footer=${footer}>
    <div class="stack" style="gap:12px">
      <p class="selectable" style="overflow-wrap:anywhere;max-height:30vh;overflow:auto">${c.what}</p>
      <p class="dim">${t(hint(c))}</p>
    </div>
  </${Dialog}>`;
}
