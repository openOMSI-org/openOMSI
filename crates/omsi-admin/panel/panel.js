// openOMSI server administration panel. Talks to /api/admin/* of the server that served it
// (see docs/SERVER.md). Every text from the server is shown with textContent, never as HTML.
// The token lives in this tab's sessionStorage only; the password is never kept.
"use strict";

const T = {
  en: {
    title: "Server administration", logout: "Log out", login: "Log in", password: "Admin or moderator password", enter: "Log in",
    "login-hint": "The password of server.cfg (admin_password or moderator_password). It is sent to this server only, and kept nowhere in this page.",
    overview: "Server", stale: "The server has not reported for a while: what you see may be old.", players: "Players",
    search: "Search by name or #number", "sel-gone": "This player is no longer on the server (or reconnected). Pick them again from the list.",
    reason: "Reason (the player reads it)", kick: "Send away", ban: "Send away for this run", tell: "A word for this player only", send: "Send",
    announce: "Announcement", say: "A chat line to everybody", world: "World", "world-role": "A moderator leaves the world alone: these are for an admin.",
    clock: "Clock", "clock-lock": "The clock follows the server machine's real time (real_time = 1): it cannot be moved.", set: "Set", speed: "Speed",
    weather: "Weather", "weather-lock": "The weather follows the METAR report (metar_sync = 1): it cannot be changed.", next: "Next weather",
    "weather-file": "Or a weather file", traffic: "AI traffic", clear: "Clear the road", pax: "Passenger density",
    "pax-hint": "A factor on the map's own demand (1 = as the map says, 0 = no new people). People already waiting, walking or aboard stay.",
    "pax-off": "The server has no passengers (passengers = 0 in server.cfg).", events: "What happened", cancel: "Cancel", confirm: "Confirm",
    map: "Map", time: "Time", weatherNow: "Weather", playersNow: "Players", trafficNow: "AI traffic", pax2: "Passengers", version: "Version",
    waiting: "waiting", walking: "walking", aboard: "aboard", off: "off", density: "density", role: "Role",
    driving: "driving", on_foot: "on foot", aboard_of: "aboard #", joining: "joining", admin: "admin", moderator: "moderator",
    online: "online", noPlayers: "Nobody found.", noEvents: "Nothing yet.",
    confirmKick: "Send away {name} (number {id}, connection {conn})?", confirmBan: "Send away {name} (number {id}, connection {conn}) for the rest of this server run?",
    confirmClear: "Take every AI car off the road?", unreachable: "The server does not answer. Trying again…",
    expired: "The session has ended: log in again.", queued: "queued", applied: "applied", pending: "pending", rejected: "rejected",
    asked: "Asked: {what}", timeout: "No answer from the server in time.", noPerm: "Your role may not do this.",
    lockedRT: "real time", lockedMETAR: "METAR",
  },
  pt: {
    title: "Administração do servidor", logout: "Sair", login: "Entrar", password: "Senha de admin ou moderador", enter: "Entrar",
    "login-hint": "A senha do server.cfg (admin_password ou moderator_password). Ela vai só para este servidor e não fica guardada nesta página.",
    overview: "Servidor", stale: "O servidor não responde há um tempo: o que você vê pode estar velho.", players: "Jogadores",
    search: "Buscar por nome ou #número", "sel-gone": "Este jogador não está mais no servidor (ou reconectou). Escolha de novo na lista.",
    reason: "Motivo (o jogador lê)", kick: "Expulsar", ban: "Expulsar até reiniciar", tell: "Uma mensagem só para este jogador", send: "Enviar",
    announce: "Anúncio", say: "Uma linha no chat para todos", world: "Mundo", "world-role": "Moderador não mexe no mundo: isto é para admin.",
    clock: "Relógio", "clock-lock": "O relógio segue a hora real da máquina do servidor (real_time = 1): não dá para mudar.", set: "Aplicar", speed: "Velocidade",
    weather: "Clima", "weather-lock": "O clima segue o METAR (metar_sync = 1): não dá para mudar.", next: "Próximo clima",
    "weather-file": "Ou um arquivo de clima", traffic: "Tráfego de IA", clear: "Limpar as ruas", pax: "Densidade de passageiros",
    "pax-hint": "Um fator sobre a demanda do próprio mapa (1 = como o mapa diz, 0 = nenhuma pessoa nova). Quem já espera, anda ou está a bordo fica.",
    "pax-off": "O servidor está sem passageiros (passengers = 0 no server.cfg).", events: "O que aconteceu", cancel: "Cancelar", confirm: "Confirmar",
    map: "Mapa", time: "Hora", weatherNow: "Clima", playersNow: "Jogadores", trafficNow: "Tráfego de IA", pax2: "Passageiros", version: "Versão",
    waiting: "esperando", walking: "andando", aboard: "a bordo", off: "desligado", density: "densidade", role: "Papel",
    driving: "dirigindo", on_foot: "a pé", aboard_of: "a bordo do #", joining: "entrando", admin: "admin", moderator: "moderador",
    online: "online", noPlayers: "Ninguém encontrado.", noEvents: "Nada ainda.",
    confirmKick: "Expulsar {name} (número {id}, conexão {conn})?", confirmBan: "Expulsar {name} (número {id}, conexão {conn}) até o servidor reiniciar?",
    confirmClear: "Tirar todos os carros de IA das ruas?", unreachable: "O servidor não responde. Tentando de novo…",
    expired: "A sessão terminou: entre de novo.", queued: "na fila", applied: "aplicado", pending: "enviado", rejected: "recusado",
    asked: "Pedido: {what}", timeout: "O servidor não respondeu a tempo.", noPerm: "Seu papel não permite isso.",
    lockedRT: "hora real", lockedMETAR: "METAR",
  },
};

const $ = (id) => document.getElementById(id);
let lang = (sessionStorage.getItem("lang") || (navigator.language || "en")).toLowerCase().startsWith("pt") ? "pt" : "en";
const t = (k, vars) => {
  let s = (T[lang] && T[lang][k]) || T.en[k] || k;
  for (const [n, v] of Object.entries(vars || {})) s = s.split("{" + n + "}").join(String(v));
  return s;
};

const S = { token: sessionStorage.getItem("token") || "", session: null, server: null, players: [], selected: null, ops: new Map(), audit: 0, timer: 0, failures: 0 };

function translate() {
  document.documentElement.lang = lang === "pt" ? "pt-BR" : "en";
  document.querySelectorAll("[data-t]").forEach((e) => { e.textContent = t(e.dataset.t); });
  document.querySelectorAll("[data-tp]").forEach((e) => { e.placeholder = t(e.dataset.tp); });
  document.title = t("title");
  $("lang").textContent = lang === "pt" ? "EN" : "PT";
  if (S.server) render();
}

function banner(text) {
  const b = $("banner");
  b.hidden = !text;
  b.textContent = text || "";
}

// One request; null when the network failed or took too long (the panel stays usable).
async function api(method, path, body) {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), 8000);
  try {
    const r = await fetch(path, {
      method, signal: ctl.signal, cache: "no-store", credentials: "omit",
      headers: Object.assign({ "Content-Type": "application/json" }, S.token ? { Authorization: "Bearer " + S.token } : {}),
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    let data = {};
    try { data = await r.json(); } catch (_) { data = {}; }
    S.failures = 0;
    banner("");
    if (r.status === 401 && path !== "/api/admin/login") {
      expire(t("expired"));
      return null;
    }
    return { status: r.status, data };
  } catch (_) {
    S.failures++;
    banner(t("unreachable"));
    return null;
  } finally {
    clearTimeout(timer);
  }
}

function expire(msg) {
  S.token = "";
  S.session = null;
  sessionStorage.removeItem("token");
  showLogin(msg);
}

function showLogin(msg) {
  clearTimeout(S.timer);
  $("app").hidden = true;
  $("logout").hidden = true;
  $("who").textContent = "";
  $("login").hidden = false;
  $("login-msg").textContent = msg || "";
  $("password").focus();
}

async function login(ev) {
  ev.preventDefault();
  const pw = $("password").value;
  $("password").value = "";
  const r = await api("POST", "/api/admin/login", { password: pw });
  if (!r) return;
  if (r.status !== 200) {
    $("login-msg").textContent = r.data.error || String(r.status);
    return;
  }
  S.token = r.data.token;
  sessionStorage.setItem("token", S.token);
  start(r.data.session);
}

async function logout() {
  await api("POST", "/api/admin/logout", {});
  expire("");
}

function may(perm) {
  return !!S.session && S.session.permissions.includes(perm);
}

function start(session) {
  S.session = session;
  $("login").hidden = true;
  $("app").hidden = false;
  $("logout").hidden = false;
  refresh();
}

// The panel's rhythm: state, players and events every few seconds while the tab is shown;
// slower after failures; nothing while hidden.
async function refresh() {
  clearTimeout(S.timer);
  if (!S.token) return;
  // (one loop: a refresh started while another waits for its answers takes over from it)
  const run = (S.run = (S.run || 0) + 1);
  if (!document.hidden) {
    const q = encodeURIComponent($("search").value.trim());
    const [st, pl, ev] = await Promise.all([api("GET", "/api/admin/state"), api("GET", "/api/admin/players?q=" + q), api("GET", "/api/admin/events?audit_after=" + S.audit)]);
    if (!S.token) return;
    if (st && st.status === 200) { S.session = st.data.session; S.server = st.data.server; S.age = st.data.age_ms; }
    if (pl && pl.status === 200) { S.players = pl.data.players; S.total = pl.data.total; }
    // the player picked, when the search does not show them: still here, on the same connection?
    S.selHere = null;
    if (S.selected && !S.players.some((p) => p.id === S.selected.id)) {
      const one = await api("GET", "/api/admin/players?q=" + encodeURIComponent("#" + S.selected.id));
      if (one && one.status === 200) S.selHere = one.data.players.some((p) => p.id === S.selected.id && p.conn === S.selected.conn);
    }
    if (ev && ev.status === 200) { for (const o of ev.data.ops) S.ops.set(o.id, o); }
    if (run !== S.run) return;
    render();
  }
  if (run !== S.run) return;
  const wait = Math.min(30000, 3000 * Math.pow(2, Math.min(S.failures, 4)));
  clearTimeout(S.timer);
  S.timer = setTimeout(refresh, wait);
}

function fact(dl, k, v) {
  const dt = document.createElement("dt");
  dt.textContent = k;
  const dd = document.createElement("dd");
  dd.textContent = v;
  dl.append(dt, dd);
}

function render() {
  const s = S.server, sess = S.session;
  if (!s || !sess) return;
  $("who").textContent = sess.actor + " · " + t(sess.role);
  const dl = $("facts");
  dl.replaceChildren();
  fact(dl, t("map"), s.map);
  fact(dl, t("playersNow"), s.players + " / " + s.max_players);
  fact(dl, t("time"), s.time + "  ×" + s.clock_speed + (s.real_time ? " (" + t("lockedRT") + ")" : ""));
  fact(dl, t("weatherNow"), (s.weather || "—") + (s.metar ? " (" + t("lockedMETAR") + " " + s.metar + ")" : ""));
  fact(dl, t("trafficNow"), s.traffic + " / " + s.traffic_max + " · " + s.cars + " + " + s.buses);
  fact(dl, t("pax2"), s.passengers ? t("density") + " " + s.passenger_density.toFixed(2) + " · " + s.waiting + " " + t("waiting") + ", " + s.walking + " " + t("walking") + ", " + s.aboard + " " + t("aboard") : t("off"));
  fact(dl, t("version"), s.version);
  $("stale").hidden = !(S.age === null || S.age > 5000);
  // the world's controls, with their locks
  const world = may("world");
  $("world-role").hidden = world;
  $("clock-lock").hidden = !s.real_time;
  $("weather-lock").hidden = !s.metar;
  $("pax-off").hidden = s.passengers;
  document.querySelectorAll("[data-perm]").forEach((b) => { b.disabled = !may(b.dataset.perm); });
  ["clock-go", "hour-back", "hour-on", "speed-go"].forEach((id) => { if (s.real_time) $(id).disabled = true; });
  ["weather-next", "weather-go"].forEach((id) => { if (s.metar) $(id).disabled = true; });
  if (!s.passengers) $("pax-go").disabled = true;
  // (the inputs keep what the admin is typing)
  if (document.activeElement !== $("traffic")) $("traffic").value = s.traffic;
  $("traffic").max = s.traffic_max;
  if (document.activeElement !== $("pax")) $("pax").value = s.passenger_density;
  $("pax").max = s.passenger_density_max;
  if (document.activeElement !== $("clock") && !$("clock").value) $("clock").value = s.time;
  renderPlayers();
  renderEvents();
}

function playerLabel(p) {
  return (p.name || "#" + p.id) + " (#" + p.id + ")";
}

function playerMeta(p) {
  const what = p.state === "aboard" ? t("aboard_of") + p.aboard : t(p.state);
  const bus = p.vehicle ? " · " + p.vehicle.split("/").pop() : "";
  const role = p.admin ? " · " + t(p.admin) : "";
  return what + bus + role + " · " + Math.floor(p.online_secs / 60) + " min";
}

function renderPlayers() {
  const ul = $("players");
  ul.replaceChildren();
  $("count").textContent = S.players.length + " / " + (S.total ?? 0);
  if (!S.players.length) {
    const li = document.createElement("li");
    li.textContent = t("noPlayers");
    ul.append(li);
  }
  for (const p of S.players) {
    const li = document.createElement("li");
    li.tabIndex = 0;
    li.setAttribute("role", "option");
    const sel = S.selected && S.selected.id === p.id && S.selected.conn === p.conn;
    li.setAttribute("aria-selected", sel ? "true" : "false");
    const name = document.createElement("div");
    name.textContent = playerLabel(p);
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.textContent = playerMeta(p);
    li.append(name, meta);
    const pick = () => { S.selected = { id: p.id, conn: p.conn, name: p.name }; S.selHere = true; renderPlayers(); };
    li.addEventListener("click", pick);
    li.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); pick(); } });
    ul.append(li);
  }
  // the selection stays while the player is the same connection; a player gone (or back on a
  // new connection) is not acted on
  const box = $("selected");
  if (!S.selected) { box.hidden = true; return; }
  box.hidden = false;
  const now = S.players.find((p) => p.id === S.selected.id && p.conn === S.selected.conn);
  // (not in this list: here only if the server said so for the number and connection)
  const here = !!now || S.selHere === true;
  $("sel-name").textContent = playerLabel(S.selected);
  $("sel-facts").textContent = now ? playerMeta(now) : "";
  $("sel-gone").hidden = here;
  ["kick", "ban", "tell"].forEach((id) => { $(id).disabled = !may("moderate") || !here; });
}

function renderEvents() {
  const ul = $("events");
  ul.replaceChildren();
  const ops = [...S.ops.values()].sort((a, b) => b.id - a.id).slice(0, 40);
  if (!ops.length) {
    const li = document.createElement("li");
    li.textContent = t("noEvents");
    ul.append(li);
  }
  for (const o of ops) {
    const li = document.createElement("li");
    const st = document.createElement("span");
    st.className = "status " + o.status;
    st.textContent = t(o.status);
    const what = document.createElement("span");
    what.textContent = " " + o.summary + (o.message ? " — " + o.message : "");
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.textContent = new Date(o.at * 1000).toLocaleTimeString() + " · " + o.actor;
    li.append(st, what, meta);
    ul.append(li);
  }
}

// An operation: sent, then followed until the server says what became of it.
async function op(body) {
  const r = await api("POST", "/api/admin/ops", body);
  if (!r) return;
  if (r.status !== 202) {
    S.ops.set("local" + Date.now(), { id: Date.now() * 1000, status: "rejected", summary: body.op, message: r.data.error || String(r.status), at: Date.now() / 1000, actor: S.session ? S.session.actor : "" });
    renderEvents();
    return;
  }
  const rec = r.data.op;
  S.ops.set(rec.id, rec);
  renderEvents();
  for (let i = 0; i < 20; i++) {
    await new Promise((res) => setTimeout(res, 500));
    const f = await api("GET", "/api/admin/ops/" + rec.id);
    if (!f || f.status !== 200) break;
    S.ops.set(rec.id, f.data.op);
    renderEvents();
    if (f.data.op.status !== "queued") { refresh(); return; }
  }
  const last = S.ops.get(rec.id);
  if (last && last.status === "queued") { last.message = t("timeout"); renderEvents(); }
}

function confirmThen(text, then) {
  const d = $("confirm");
  $("confirm-text").textContent = text;
  d.returnValue = "";
  d.onclose = () => { if (d.returnValue === "ok") then(); };
  d.showModal();
}

function target() {
  return S.selected ? { id: S.selected.id, conn: S.selected.conn } : null;
}

function wire() {
  $("login-form").addEventListener("submit", login);
  $("logout").addEventListener("click", logout);
  $("lang").addEventListener("click", () => { lang = lang === "pt" ? "en" : "pt"; sessionStorage.setItem("lang", lang); translate(); });
  let typing = 0;
  $("search").addEventListener("input", () => { clearTimeout(typing); typing = setTimeout(refresh, 300); });
  const ask = (kind) => () => {
    const tg = target();
    if (!tg) return;
    const vars = { name: S.selected.name || "#" + tg.id, id: tg.id, conn: tg.conn };
    const reason = $("reason").value.trim();
    confirmThen(t(kind === "ban" ? "confirmBan" : "confirmKick", vars), () => op({ op: kind, target: tg, reason }));
  };
  $("kick").addEventListener("click", ask("kick"));
  $("ban").addEventListener("click", ask("ban"));
  $("tell").addEventListener("click", () => { const tg = target(); const text = $("tell-text").value.trim(); if (tg && text) { op({ op: "tell", target: tg, text }); $("tell-text").value = ""; } });
  $("say").addEventListener("click", () => { const text = $("say-text").value.trim(); if (text) { op({ op: "say", text }); $("say-text").value = ""; } });
  $("clock-go").addEventListener("click", () => { const v = $("clock").value; if (/^\d\d:\d\d$/.test(v)) { const [h, m] = v.split(":").map(Number); op({ op: "clock", seconds: h * 3600 + m * 60 }); } });
  $("hour-back").addEventListener("click", () => op({ op: "shift", seconds: -3600 }));
  $("hour-on").addEventListener("click", () => op({ op: "shift", seconds: 3600 }));
  $("speed-go").addEventListener("click", () => op({ op: "speed", factor: Number($("speed").value) }));
  $("weather-next").addEventListener("click", () => op({ op: "weather", next: true }));
  $("weather-go").addEventListener("click", () => { const f = $("weather-file").value.trim(); if (f) op({ op: "weather", file: f }); });
  $("traffic-go").addEventListener("click", () => op({ op: "traffic", density: Number($("traffic").value) }));
  $("traffic-clear").addEventListener("click", () => confirmThen(t("confirmClear"), () => op({ op: "traffic", clear: true })));
  $("pax-go").addEventListener("click", () => op({ op: "passenger_density", value: Number($("pax").value) }));
  document.addEventListener("visibilitychange", () => { if (!document.hidden) refresh(); });
}

async function boot() {
  translate();
  wire();
  if (!S.token) return showLogin("");
  const r = await api("GET", "/api/admin/session");
  if (r && r.status === 200) start(r.data.session);
  // (refused - off, another address, a lock: the login with the server's word, never a blank page)
  else showLogin((r && r.data && r.data.error) || "");
}

boot();
