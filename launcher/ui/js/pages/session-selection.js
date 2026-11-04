import { S, set } from '../store.js';
import { choose, allowedBus, serverJoined } from '../data.js';

export function selectSession(connection, host, name) {
  if (S.choice.lan_mode !== 'join') set({ driveBeforeJoin: { ...S.choice } });
  const map = (host?.map || '').replace(/\\/g, '/');
  const installed = map && S.maps?.some((m) => m.file.toLowerCase() === map.toLowerCase());
  choose({ lan_mode: 'join', ...connection, lan_map: map, ...serverJoined(host ? { ...host, name: name || host.name } : null), ...(installed ? { map } : {}) });
  choose(allowedBus());
  set({ page: 'drive' });
}

export function cancelSession() {
  const previous = S.driveBeforeJoin;
  set({ driveBeforeJoin: null });
  choose({ lan_mode: 'off', lan_addr: '', lan_ws_fallback: null, lan_map: '', lan_server: null, ...previous });
  // Undo must preserve a deliberately out-of-era date or empty fleet number.
  if (previous) choose({ date: previous.date, number: previous.number });
}
