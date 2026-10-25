// The browser a page sees: the DOM, events, timers and `window.omsi`, on top of the host's
// primitives (`__host(op, ...)`, which the game's engine provides and this script hides).
(function (g) {
  'use strict';
  const H = g.__host;
  delete g.__host;
  const J = (s) => JSON.parse(s);
  const kebab = (s) => s.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase());
  const camel = (s) => s.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const cache = new Map();

  // ---------------------------------------------------------------- events
  class Event {
    constructor(type, init) {
      this.type = type;
      init = init || {};
      this.bubbles = init.bubbles !== false;
      this.cancelable = !!init.cancelable;
      this.defaultPrevented = false;
      this.target = null;
      this.currentTarget = null;
      this.timeStamp = H('now') * 1000;
      this.cancelBubble = false;
    }
    stopPropagation() { this.cancelBubble = true; }
    stopImmediatePropagation() { this.cancelBubble = true; this._stop = true; }
    preventDefault() { this.defaultPrevented = true; }
  }
  class CustomEvent extends Event {
    constructor(type, init) { super(type, init); this.detail = init && init.detail !== undefined ? init.detail : null; }
  }
  class EventTarget {
    constructor() { this._l = Object.create(null); }
    addEventListener(type, fn, opts) {
      if (typeof fn !== 'function' && !(fn && typeof fn.handleEvent === 'function')) return;
      const l = this._l[type] || (this._l[type] = []);
      const once = opts && typeof opts === 'object' && opts.once;
      if (!l.some((x) => x.fn === fn)) l.push({ fn, once });
    }
    removeEventListener(type, fn) {
      const l = this._l[type];
      if (l) this._l[type] = l.filter((x) => x.fn !== fn);
    }
    _fire(ev) {
      ev.currentTarget = this;
      const l = this._l[ev.type];
      if (l) {
        for (const x of l.slice()) {
          if (x.once) this.removeEventListener(ev.type, x.fn);
          try {
            if (typeof x.fn === 'function') x.fn.call(this, ev); else x.fn.handleEvent(ev);
          } catch (e) { H('log', 'listener error: ' + e); }
          if (ev._stop) break;
        }
      }
      const h = this['on' + ev.type];
      if (typeof h === 'function') {
        try { h.call(this, ev); } catch (e) { H('log', 'handler error: ' + e); }
      }
    }
    dispatchEvent(ev) {
      ev.target = ev.target || this;
      this._fire(ev);
      return !ev.defaultPrevented;
    }
  }
  const inlineCode = new Map();
  function runInline(el, ev) {
    const code = H('attr', el._i, 'on' + ev.type);
    if (code === null || code === undefined) return;
    let f = inlineCode.get(code);
    if (!f) {
      try { f = new Function('event', code); } catch (e) { H('log', 'on' + ev.type + ' attribute: ' + e); f = () => {}; }
      inlineCode.set(code, f);
    }
    try { g.event = ev; f.call(el, ev); } catch (e) { H('log', 'on' + ev.type + ' handler failed: ' + e); }
  }

  // ---------------------------------------------------------------- nodes
  class Node extends EventTarget {
    constructor(i) { super(); this._i = i; }
    get parentNode() { return wrap(H('parent', this._i)); }
    get parentElement() { const p = this.parentNode; return p && p.nodeType === 1 ? p : null; }
    get childNodes() { return J(H('kids', this._i)).map(wrap); }
    get children() { return this.childNodes.filter((n) => n.nodeType === 1); }
    get firstChild() { return this.childNodes[0] || null; }
    get lastChild() { const c = this.childNodes; return c[c.length - 1] || null; }
    get firstElementChild() { return this.children[0] || null; }
    get lastElementChild() { const c = this.children; return c[c.length - 1] || null; }
    get childElementCount() { return this.children.length; }
    _sib(d, elementsOnly) {
      const p = this.parentNode;
      if (!p) return null;
      const list = elementsOnly ? p.children : p.childNodes;
      const at = list.indexOf(this);
      return list[at + d] || null;
    }
    get nextSibling() { return this._sib(1, false); }
    get previousSibling() { return this._sib(-1, false); }
    get nextElementSibling() { return this._sib(1, true); }
    get previousElementSibling() { return this._sib(-1, true); }
    hasChildNodes() { return this.childNodes.length > 0; }
    contains(n) { while (n) { if (n === this) return true; n = n.parentNode; } return false; }
    appendChild(c) { H('append', this._i, c._i); return c; }
    insertBefore(c, ref) { H('insertBefore', this._i, c._i, ref ? ref._i : -1); return c; }
    removeChild(c) { H('remove', c._i); return c; }
    replaceChild(n, old) { H('insertBefore', this._i, n._i, old._i); H('remove', old._i); return old; }
    remove() { H('remove', this._i); }
    cloneNode(deep) { return wrap(H('clone', this._i, !!deep)); }
    get textContent() { return H('text', this._i); }
    set textContent(v) { H('setText', this._i, String(v)); }
    get ownerDocument() { return document; }
    get isConnected() { return document.documentElement.contains(this) || document.body.contains(this); }
    getRootNode() { return document; }
  }
  class Text extends Node {
    get nodeType() { return 3; }
    get nodeName() { return '#text'; }
    get data() { return H('text', this._i); }
    set data(v) { H('setText', this._i, String(v)); }
    get nodeValue() { return this.data; }
    set nodeValue(v) { this.data = v; }
    get wholeText() { return this.data; }
  }
  const REFLECT = ['src', 'href', 'title', 'alt', 'type', 'name', 'value', 'placeholder', 'width', 'height', 'for', 'role', 'lang', 'dir'];
  const EVENTS = ['click', 'dblclick', 'mousedown', 'mouseup', 'mousemove', 'mouseover', 'mouseout', 'pointerdown', 'pointerup', 'pointermove', 'touchstart', 'touchend', 'touchmove', 'keydown', 'keyup', 'input', 'change', 'focus', 'blur', 'wheel', 'contextmenu', 'load', 'error'];
  function classList(el) {
    const get = () => H('classes', el._i).split(/\s+/).filter(Boolean);
    const set = (a) => H('setClasses', el._i, a.join(' '));
    return {
      add(...c) { const a = get(); for (const x of c) if (!a.includes(x)) a.push(x); set(a); },
      remove(...c) { set(get().filter((x) => !c.includes(x))); },
      toggle(c, force) {
        const has = get().includes(c);
        const on = force === undefined ? !has : !!force;
        if (on && !has) this.add(c); else if (!on && has) this.remove(c);
        return on;
      },
      contains(c) { return get().includes(c); },
      replace(a, b) { const l = get(); const i = l.indexOf(a); if (i < 0) return false; l[i] = b; set(l); return true; },
      item(i) { return get()[i] || null; },
      get length() { return get().length; },
      toString() { return get().join(' '); },
      [Symbol.iterator]() { return get()[Symbol.iterator](); },
      forEach(f) { get().forEach(f); },
    };
  }
  function styleProxy(el) {
    const api = {
      getPropertyValue: (p) => H('styleGet', el._i, kebab(p)) || '',
      setProperty: (p, v) => H('styleSet', el._i, kebab(p), String(v)),
      removeProperty: (p) => { const old = H('styleGet', el._i, kebab(p)) || ''; H('styleSet', el._i, kebab(p), ''); return old; },
    };
    return new Proxy(api, {
      get(t, k) {
        if (k === 'cssText') return H('styleText', el._i);
        if (typeof k === 'symbol') return undefined;
        if (k in t) return t[k];
        return H('styleGet', el._i, kebab(k)) || '';
      },
      set(t, k, v) {
        if (k === 'cssText') { H('setAttr', el._i, 'style', String(v)); return true; }
        H('styleSet', el._i, kebab(k), v === null || v === undefined ? '' : String(v));
        return true;
      },
    });
  }
  function datasetProxy(el) {
    return new Proxy({}, {
      get(_, k) { if (typeof k === 'symbol') return undefined; const v = H('attr', el._i, 'data-' + kebab(k)); return v === null ? undefined : v; },
      set(_, k, v) { H('setAttr', el._i, 'data-' + kebab(k), String(v)); return true; },
      has(_, k) { return H('attr', el._i, 'data-' + kebab(String(k))) !== null; },
      deleteProperty(_, k) { H('removeAttr', el._i, 'data-' + kebab(String(k))); return true; },
    });
  }
  class Element extends Node {
    get nodeType() { return 1; }
    get tagName() { return H('tag', this._i).toUpperCase(); }
    get nodeName() { return this.tagName; }
    get localName() { return H('tag', this._i); }
    get id() { return H('attr', this._i, 'id') || ''; }
    set id(v) { H('setAttr', this._i, 'id', String(v)); }
    get className() { return H('classes', this._i); }
    set className(v) { H('setClasses', this._i, String(v)); }
    get classList() { return this._cl || (this._cl = classList(this)); }
    get style() { return this._st || (this._st = styleProxy(this)); }
    get dataset() { return this._ds || (this._ds = datasetProxy(this)); }
    get innerText() { return H('text', this._i); }
    set innerText(v) { H('setText', this._i, String(v)); }
    get innerHTML() { return H('html', this._i); }
    set innerHTML(v) { H('setHtml', this._i, String(v)); }
    get outerHTML() { return H('outer', this._i); }
    get hidden() { return H('attr', this._i, 'hidden') !== null; }
    set hidden(v) { if (v) H('setAttr', this._i, 'hidden', ''); else H('removeAttr', this._i, 'hidden'); }
    get checked() { return H('attr', this._i, 'checked') !== null; }
    set checked(v) { if (v) H('setAttr', this._i, 'checked', ''); else H('removeAttr', this._i, 'checked'); }
    get disabled() { return H('attr', this._i, 'disabled') !== null; }
    set disabled(v) { if (v) H('setAttr', this._i, 'disabled', ''); else H('removeAttr', this._i, 'disabled'); }
    getAttribute(n) { const v = H('attr', this._i, String(n).toLowerCase()); return v === undefined ? null : v; }
    setAttribute(n, v) { H('setAttr', this._i, String(n).toLowerCase(), String(v)); }
    removeAttribute(n) { H('removeAttr', this._i, String(n).toLowerCase()); }
    hasAttribute(n) { return this.getAttribute(n) !== null; }
    getAttributeNames() { return J(H('attrNames', this._i)); }
    toggleAttribute(n, force) { const has = this.hasAttribute(n); const on = force === undefined ? !has : !!force; if (on && !has) this.setAttribute(n, ''); else if (!on && has) this.removeAttribute(n); return on; }
    get attributes() { return this.getAttributeNames().map((name) => ({ name, value: this.getAttribute(name) })); }
    querySelector(s) { return wrap(H('query', String(s), this._i)); }
    querySelectorAll(s) { return J(H('queryAll', String(s), this._i)).map(wrap); }
    getElementsByClassName(c) { return this.querySelectorAll('.' + String(c).trim().split(/\s+/).join('.')); }
    getElementsByTagName(t) { return this.querySelectorAll(t); }
    matches(s) { return !!H('matches', this._i, String(s)); }
    closest(s) { let n = this; while (n && n.nodeType === 1) { if (n.matches(s)) return n; n = n.parentNode; } return null; }
    append(...n) { for (const x of n) this.appendChild(typeof x === 'object' ? x : document.createTextNode(String(x))); }
    prepend(...n) { const first = this.firstChild; for (const x of n) this.insertBefore(typeof x === 'object' ? x : document.createTextNode(String(x)), first); }
    before(...n) { const p = this.parentNode; if (p) for (const x of n) p.insertBefore(typeof x === 'object' ? x : document.createTextNode(String(x)), this); }
    after(...n) { const p = this.parentNode; const nx = this.nextSibling; if (p) for (const x of n) p.insertBefore(typeof x === 'object' ? x : document.createTextNode(String(x)), nx); }
    replaceWith(...n) { this.before(...n); this.remove(); }
    replaceChildren(...n) { for (const c of this.childNodes) c.remove(); this.append(...n); }
    insertAdjacentHTML(pos, html) {
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      const nodes = tmp.childNodes;
      if (pos === 'beforeend') this.append(...nodes);
      else if (pos === 'afterbegin') this.prepend(...nodes);
      else if (pos === 'beforebegin') this.before(...nodes);
      else if (pos === 'afterend') this.after(...nodes);
    }
    insertAdjacentElement(pos, el) { this.insertAdjacentHTML(pos, ''); (pos === 'beforeend' ? this.appendChild(el) : pos === 'afterbegin' ? this.prepend(el) : pos === 'beforebegin' ? this.before(el) : this.after(el)); return el; }
    getBoundingClientRect() {
      const r = J(H('rect', this._i));
      return { x: r[0], y: r[1], left: r[0], top: r[1], width: r[2], height: r[3], right: r[0] + r[2], bottom: r[1] + r[3] };
    }
    getClientRects() { return [this.getBoundingClientRect()]; }
    get offsetWidth() { return Math.round(J(H('rect', this._i))[2]); }
    get offsetHeight() { return Math.round(J(H('rect', this._i))[3]); }
    get offsetLeft() { return Math.round(J(H('rect', this._i))[0]); }
    get offsetTop() { return Math.round(J(H('rect', this._i))[1]); }
    get clientWidth() { return this.offsetWidth; }
    get clientHeight() { return this.offsetHeight; }
    get scrollWidth() { return this.offsetWidth; }
    get scrollHeight() { return this.offsetHeight; }
    get scrollTop() { return 0; }
    set scrollTop(v) {}
    get scrollLeft() { return 0; }
    set scrollLeft(v) {}
    get offsetParent() { return this.parentElement; }
    scrollIntoView() {}
    focus() {}
    blur() {}
    click() { dispatchPointer(this._i, 'click', 0, 0); }
    animate() { return { cancel() {}, finish() {}, onfinish: null }; }
  }
  for (const n of REFLECT) {
    Object.defineProperty(Element.prototype, n, {
      get() { return H('attr', this._i, n) || ''; },
      set(v) { H('setAttr', this._i, n, String(v)); },
      configurable: true,
    });
  }
  for (const n of EVENTS) {
    Object.defineProperty(EventTarget.prototype, 'on' + n, {
      get() { return this['_on' + n] || null; },
      set(v) { this['_on' + n] = typeof v === 'function' ? v : null; },
      configurable: true,
    });
  }
  function wrap(i) {
    if (i < 0) return null;
    let e = cache.get(i);
    if (!e) {
      e = H('isText', i) ? new Text(i) : new Element(i);
      cache.set(i, e);
    }
    return e;
  }

  // ---------------------------------------------------------------- document and window
  class Document extends EventTarget {
    get nodeType() { return 9; }
    get body() { return wrap(H('body')); }
    get documentElement() { return wrap(H('root')); }
    get head() { return this.querySelector('head'); }
    get title() { const t = this.querySelector('title'); return t ? t.textContent : ''; }
    set title(v) {}
    get readyState() { return 'complete'; }
    get activeElement() { return this.body; }
    get hidden() { return false; }
    get visibilityState() { return 'visible'; }
    get cookie() { return ''; }
    set cookie(v) {}
    get location() { return g.location; }
    getElementById(id) { return wrap(H('byId', String(id))); }
    querySelector(s) { return wrap(H('query', String(s), -1)); }
    querySelectorAll(s) { return J(H('queryAll', String(s), -1)).map(wrap); }
    getElementsByClassName(c) { return this.querySelectorAll('.' + String(c).trim().split(/\s+/).join('.')); }
    getElementsByTagName(t) { return this.querySelectorAll(t); }
    createElement(t) { return wrap(H('create', String(t).toLowerCase())); }
    createElementNS(ns, t) { return this.createElement(t); }
    createTextNode(t) { return wrap(H('createText', String(t))); }
    createDocumentFragment() { return this.createElement('div'); }
    createEvent(t) { return new Event(''); }
    hasFocus() { return true; }
    elementFromPoint(x, y) { return wrap(H('hit', +x, +y)); }
  }
  const document = new Document();
  g.document = document;
  g.window = g;
  g.self = g;
  g.globalThis = g;
  g.top = g;
  g.parent = g;
  g.Node = Node; g.Element = Element; g.HTMLElement = Element; g.Text = Text; g.Document = Document;
  g.Event = Event; g.CustomEvent = CustomEvent; g.EventTarget = EventTarget;
  g.MouseEvent = Event; g.PointerEvent = Event; g.KeyboardEvent = Event;
  const winTarget = new EventTarget();
  for (const k of ['addEventListener', 'removeEventListener', 'dispatchEvent']) g[k] = winTarget[k].bind(winTarget);
  g.innerWidth = H('width'); g.innerHeight = H('height');
  g.outerWidth = g.innerWidth; g.outerHeight = g.innerHeight;
  g.devicePixelRatio = 1; g.scrollX = 0; g.scrollY = 0; g.pageXOffset = 0; g.pageYOffset = 0;
  g.screen = { width: g.innerWidth, height: g.innerHeight, availWidth: g.innerWidth, availHeight: g.innerHeight, colorDepth: 24 };
  g.navigator = { userAgent: 'openOMSI', language: 'en', languages: ['en'], platform: 'openOMSI', onLine: true };
  g.location = { href: 'omsi://page', origin: 'omsi://page', protocol: 'omsi:', host: '', hostname: '', pathname: '/', search: '', hash: '', reload() {}, assign() {}, replace() {} };
  g.history = { length: 1, pushState() {}, replaceState() {}, back() {}, forward() {} };
  g.matchMedia = (q) => ({ matches: false, media: String(q), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} });
  g.getComputedStyle = (el) => new Proxy({}, {
    get(_, k) {
      if (k === 'getPropertyValue') return (p) => H('computed', el._i, kebab(p)) || '';
      if (typeof k === 'symbol') return undefined;
      return H('computed', el._i, kebab(k)) || '';
    },
  });
  for (const n of ['MutationObserver', 'ResizeObserver', 'IntersectionObserver']) {
    g[n] = class { constructor() {} observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } };
  }
  g.alert = (m) => H('log', 'alert: ' + m);
  g.confirm = () => false;
  g.prompt = () => null;
  g.open = () => null;
  g.scrollTo = () => {};
  g.scrollBy = () => {};
  g.focus = () => {};
  g.blur = () => {};
  g.structuredClone = (v) => (v === undefined ? v : JSON.parse(JSON.stringify(v)));
  g.queueMicrotask = (f) => { Promise.resolve().then(f); };
  const store = () => { const m = new Map(); return { getItem: (k) => (m.has(String(k)) ? m.get(String(k)) : null), setItem: (k, v) => { m.set(String(k), String(v)); }, removeItem: (k) => { m.delete(String(k)); }, clear: () => m.clear(), key: (i) => [...m.keys()][i] ?? null, get length() { return m.size; } }; };
  g.localStorage = store();
  g.sessionStorage = store();
  g.performance = { now: () => H('now') * 1000, timeOrigin: 0, mark() {}, measure() {} };
  g.atob = (s) => H('atob', String(s));
  g.btoa = (s) => H('btoa', String(s));

  // console
  const fmt = (a) => a.map((x) => { if (typeof x === 'string') return x; try { return typeof x === 'object' ? JSON.stringify(x) : String(x); } catch (e) { return String(x); } }).join(' ');
  g.console = { log: (...a) => H('log', fmt(a)), info: (...a) => H('log', fmt(a)), debug: (...a) => H('log', fmt(a)), warn: (...a) => H('log', 'warn: ' + fmt(a)), error: (...a) => H('log', 'error: ' + fmt(a)), assert: (c, ...a) => { if (!c) H('log', 'assertion failed: ' + fmt(a)); }, table: (...a) => H('log', fmt(a)), group() {}, groupEnd() {}, time() {}, timeEnd() {}, trace() {}, dir: (...a) => H('log', fmt(a)) };

  // fetch from the page's own files
  g.fetch = (url) => new Promise((res, rej) => {
    const text = H('read', String(url));
    if (text === null || text === undefined) { res({ ok: false, status: 404, statusText: 'Not Found', url: String(url), text: () => Promise.resolve(''), json: () => Promise.reject(new SyntaxError('not found')) }); return; }
    res({ ok: true, status: 200, statusText: 'OK', url: String(url), text: () => Promise.resolve(text), json: () => { try { return Promise.resolve(JSON.parse(text)); } catch (e) { return Promise.reject(e); } } });
  });

  // ---------------------------------------------------------------- timers
  let timers = [];
  let nextTimer = 1;
  let clock = 0;
  const addTimer = (f, ms, every, args) => {
    if (typeof f !== 'function' || timers.length >= 64) return 0;
    ms = Number(ms); if (!isFinite(ms) || ms < 0) ms = 0;
    const id = nextTimer++;
    timers.push({ id, due: clock + ms / 1000, every: every ? Math.max(ms / 1000, 0.016) : null, f, args });
    return id;
  };
  g.setTimeout = (f, ms, ...a) => addTimer(f, ms, false, a);
  g.setInterval = (f, ms, ...a) => addTimer(f, ms, true, a);
  g.clearTimeout = g.clearInterval = (id) => { timers = timers.filter((t) => t.id !== id); };
  g.requestAnimationFrame = (f) => addTimer(() => f(clock * 1000), 16, false, []);
  g.cancelAnimationFrame = g.clearTimeout;
  g.requestIdleCallback = (f) => addTimer(() => f({ didTimeout: false, timeRemaining: () => 10 }), 1, false, []);
  g.__tick = (now) => {
    clock = now;
    const due = timers.filter((t) => t.due <= clock).sort((a, b) => a.due - b.due || a.id - b.id);
    if (!due.length) return 0;
    // an interval is set up again before it runs, so it can stop itself
    timers = timers.filter((t) => t.due > clock || t.every !== null);
    for (const t of due) if (t.every !== null) t.due = clock + t.every;
    for (const t of due) {
      try { t.f(...t.args); } catch (e) { H('log', 'timer ' + t.id + ' failed: ' + e); }
    }
    return due.length;
  };

  // ---------------------------------------------------------------- pointer and events from the host
  function dispatchPointer(i, type, x, y) {
    const el = wrap(i);
    if (!el) return;
    const ev = new Event(type, { bubbles: true, cancelable: true });
    ev.x = ev.clientX = ev.pageX = ev.offsetX = x;
    ev.y = ev.clientY = ev.pageY = ev.offsetY = y;
    ev.button = 0; ev.buttons = type === 'mouseup' || type === 'pointerup' ? 0 : 1;
    ev.target = el;
    g.event = ev;
    let n = el;
    while (n && n.nodeType === 1 && !ev.cancelBubble) {
      ev.currentTarget = n;
      runInline(n, ev);
      if (ev.cancelBubble) break;
      n._fire(ev);
      n = n.parentNode;
    }
    if (!ev.cancelBubble) { document._fire(ev); }
    if (!ev.cancelBubble) { winTarget._fire(ev); }
  }
  g.__dispatch = dispatchPointer;
  g.__fire = (type) => {
    const ev = new Event(type, { bubbles: false });
    document._fire(ev);
    if (type === 'load') winTarget._fire(ev);
  };

  // ---------------------------------------------------------------- window.omsi
  const omsi = g.omsi = {
    apiVersion: 1,
    timestamp: 0,
    time: { hour: 0, minute: 0, second: 0, asString: '00:00:00' },
    date: { day: 1, month: 1, year: 1970, asString: '01/01/1970' },
    locale: 'en',
    vars: { num: {}, str: {} },
    departures: {},
    setVar(name, v) {
      const n = typeof v === 'boolean' ? (v ? 1 : 0) : Number(v);
      H('setVar', String(name), n);
    },
    trigger(name) { H('trigger', String(name)); },
    getVar(name) {
      const k = String(name).toLowerCase();
      if (k in omsi.vars.num) return omsi.vars.num[k];
      if (k in omsi.vars.str) return omsi.vars.str[k];
      return undefined;
    },
    getDepartures(stop) {
      const key = String(stop === undefined ? '' : stop).trim().toLowerCase();
      if (!key) return [];
      H('wantDepartures', key);
      const d = omsi.departures[key];
      return Array.isArray(d) ? d.slice() : [];
    },
  };
  if (H('vehiclePage')) {
    Object.assign(omsi, {
      setRoute(i) { H('request', 'route', Number(i)); },
      setDestination(i) { H('request', 'destination', Number(i)); },
      setNextStop(i) { H('request', 'nextstop', Number(i)); },
      setLine(t) { H('request', 'line', String(t === undefined ? '' : t)); },
      clearLine() { H('request', 'clearline', 0); },
      depot: { name: '', lines: [], routes: [], destinations: [] },
      vehicle: {},
    });
  }
  g.__omsi_set = (name, json) => { omsi[name] = J(json); };
  g.__omsi_merge = (json) => { Object.assign(omsi, J(json)); };
  g.__store_vars = (numJson, strJson) => {
    const num = J(numJson), str = J(strJson);
    for (const k in num) { omsi.vars.num[k.toLowerCase()] = num[k]; if (k.toLowerCase() !== k) omsi.vars.num[k] = num[k]; }
    for (const k in str) { omsi.vars.str[k.toLowerCase()] = str[k]; if (k.toLowerCase() !== k) omsi.vars.str[k] = str[k]; }
  };
  g.__call_update = (numJson, strJson) => {
    const f = omsi.update;
    if (typeof f !== 'function') return false;
    const arg = { num: J(numJson), str: J(strJson), vars: omsi.vars };
    if ('vehicle' in omsi) arg.vehicle = omsi.vehicle;
    f.call(omsi, arg);
    return true;
  };
})(globalThis);
