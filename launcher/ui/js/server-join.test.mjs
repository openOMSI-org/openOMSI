import assert from 'node:assert/strict';
import test from 'node:test';
import { serverJoinSettings, serverJoinProtocol } from './server-join.js';

const serverJoinAddress = (...args) => serverJoinSettings(...args).lan_addr;

const info = { reached_at: 'https://gateway.example.org' };

test('bare addresses use WS in Auto, explicit ports use UDP', () => {
  for (const address of ['play.example.org', '192.168.1.4', '::1', '[::1]']) {
    assert.equal(serverJoinAddress(address, 'auto', info), info.reached_at);
    assert.equal(serverJoinAddress(address, 'udp', info), address);
    assert.equal(serverJoinAddress(address, 'websocket', info), info.reached_at);
  }
  for (const address of ['play.example.org:27015', 'play.example.org:27025', '192.168.1.4:47015', '[::1]:27015', 'play.example.org:65535']) {
    assert.equal(serverJoinAddress(address, 'auto', info), address);
    assert.equal(serverJoinAddress(address, 'udp', info), address);
    assert.equal(serverJoinAddress(address, 'websocket', info), info.reached_at);
  }
  for (const address of ['play.example.org:65536', 'play.example.org:notaport', '2001:db8::27015', 'play.example.org/abc.trycloudflare.com/', 'play.example.org?abc.trycloudflare.com', 'play.example.org#abc.trycloudflare.com', 'evil.example\\.trycloudflare.com/ws', 'abc.trycloudflare.com@evil.example/ws', 'abc.trycloudflare.com:1@evil.example/ws', 'evil.example%5C.trycloudflare.com/ws', '.trycloudflare.com', 'a..trycloudflare.com', 'abc.trycloudflare.com:x/ws', '\u212Aevil.trycloudflare.com']) {
    assert.equal(serverJoinAddress(address, 'auto', info), info.reached_at);
  }
  assert.equal(serverJoinAddress('  play.example.org:27015  ', 'auto', info), 'play.example.org:27015');
});

test('web links, official alias and tunnels require WS', () => {
  for (const address of ['https://play.example.org', 'http://play.example.org:27025', 'https://play.example.org:27015', 'ws://play.example.org/ws', 'wss://play.example.org/ws', 'openbus', 'OpenBus', 'abc.trycloudflare.com', 'abc.trycloudflare.com/ws', 'abc.trycloudflare.com:443/ws', 'ABC.TryCloudflare.com/ws', 'me@abc.trycloudflare.com/ws']) {
    assert.equal(serverJoinAddress(address, 'auto', info), address);
    assert.equal(serverJoinAddress(address, 'websocket', info), address);
    assert.throws(() => serverJoinAddress(address, 'udp', info), /UDP needs the game's address/);
  }
});

test('all selections require gateway status', () => {
  for (const address of ['play.example.org', 'play.example.org:27015', 'https://play.example.org']) {
    for (const protocol of ['auto', 'udp', 'websocket']) {
      for (const status of [undefined, null]) {
        assert.throws(() => serverJoinAddress(address, protocol, status));
      }
    }
  }
});


test('only Auto UDP joins carry the discovered gateway as a fallback', () => {
  assert.deepEqual(serverJoinSettings('play.example.org:27015', 'auto', info), {
    lan_addr: 'play.example.org:27015', lan_ws_fallback: info.reached_at,
  });
  for (const protocol of ['udp', 'websocket']) {
    assert.equal(serverJoinSettings('play.example.org:27015', protocol, info).lan_ws_fallback, null);
  }
  for (const address of ['play.example.org', 'https://play.example.org:27015', 'openbus']) {
    assert.equal(serverJoinSettings(address, 'auto', info).lan_ws_fallback, null);
  }
});

test('Auto uses the typed port immediately when it answered HTTP status', () => {
  for (const address of ['play.example.org:27025', '192.168.1.4:47025', '[::1]:27025', '  play.example.org:27025  ']) {
    const target = address.trim();
    const status = { reached_at: `http://${target}` };
    assert.equal(serverJoinProtocol(address, 'auto', status), 'websocket');
    assert.deepEqual(serverJoinSettings(address, 'auto', status), {
      lan_addr: status.reached_at, lan_ws_fallback: null,
    });
    assert.equal(serverJoinProtocol(address, 'udp', status), 'udp');
    assert.deepEqual(serverJoinSettings(address, 'udp', status), {
      lan_addr: target, lan_ws_fallback: null,
    });
    assert.equal(serverJoinAddress(address, 'websocket', status), status.reached_at);
    assert.equal(serverJoinProtocol(address, 'auto', info), 'udp');
  }
});
