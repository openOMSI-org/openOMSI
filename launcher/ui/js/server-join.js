// Keep these rules in step with server_join_address in omsi-launcher-model.
function isWebAddress(address) {
  return /^(https?|wss?):\/\//.test(address)
    || address.toLowerCase() === 'openbus'
    || isTunnel(address);
}

function isTunnel(address) {
  const host = address.split(/[/?#\\]/)[0].split('@').pop().replace(/:[0-9]*$/, '');
  return /^[a-z0-9-]+(\.[a-z0-9-]+)*\.trycloudflare\.com$/i.test(host);
}

export function serverJoinProtocol(address, protocol = 'auto', info) {
  address = address.trim();
  const port = address.match(/^(?:[^:\s/]+|\[[^\]\s]+\]):([0-9]+)$/)?.[1];
  return protocol === 'udp' || (protocol === 'auto' && !isWebAddress(address) && port != null && Number(port) <= 65535 && info?.reached_at !== `http://${address}`)
    ? 'udp' : 'websocket';
}

export function serverJoinSettings(address, protocol = 'auto', info) {
  address = address.trim();
  const web = isWebAddress(address);
  const udp = serverJoinProtocol(address, protocol, info) === 'udp';
  if (udp && web) throw new Error("UDP needs the game's address (host or host:port), not a web link");
  if (!info) throw new Error('The server has not answered yet (is its address right? is it running?)');
  return {
    lan_addr: !udp && !web && info.reached_at ? info.reached_at : address,
    lan_ws_fallback: protocol === 'auto' && udp ? info.reached_at || null : null,
  };
}
