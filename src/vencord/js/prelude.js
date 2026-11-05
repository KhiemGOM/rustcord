// The browser a Vencord plugin expects, as much of it as plugins touch: console, timers,
// fetch, URL, base64, a little DOM, and the module loader that runs plugin code.
"use strict";
(() => {
const fc = globalThis.__fc;
const g = globalThis;
g.window = g;
g.self = g;
g.global = g;
g.IS_WEB = false;
g.IS_DISCORD_DESKTOP = true;
g.IS_VESKTOP = false;
g.IS_DEV = false;
g.IS_REPORTER = false;
g.VERSION = "rustcord";
g.IS_STANDALONE = false;
g.IS_USERSCRIPT = false;
g.IS_EXTENSION = false;
g.IS_UPDATER_DISABLED = true;
g.IS_EQUIBOP = false;

const show = x => {
    if (typeof x === "string") return x;
    if (x instanceof Error) return x.stack ? `${x}\n${x.stack}` : String(x);
    try { return JSON.stringify(x); } catch { return String(x); }
};
const say = level => (...a) => fc.log(level, a.map(show).join(" "));
g.console = { debug: say(0), log: say(1), info: say(1), warn: say(2), error: say(3), trace: say(0), group() {}, groupCollapsed() {}, groupEnd() {}, time() {}, timeEnd() {}, table: say(1) };

const out = op => fc.out(JSON.stringify(op, (_, v) => typeof v === "bigint" ? String(v) : v));
g.__out = out;

let nextTimer = 1;
const timers = new Map();
g.setTimeout = (fn, ms, ...args) => {
    const id = nextTimer++;
    timers.set(id, { fn, at: Date.now() + (+ms || 0), args });
    return id;
};
g.setInterval = (fn, ms, ...args) => {
    const id = nextTimer++;
    const every = Math.max(+ms || 0, 10);
    timers.set(id, { fn, at: Date.now() + every, every, args });
    return id;
};
g.clearTimeout = g.clearInterval = id => timers.delete(id);
g.setImmediate = (fn, ...args) => g.setTimeout(fn, 0, ...args);
g.queueMicrotask = fn => Promise.resolve().then(fn);
g.requestAnimationFrame = fn => g.setTimeout(() => fn(Date.now()), 16);
g.cancelAnimationFrame = g.clearTimeout;
g.requestIdleCallback = fn => g.setTimeout(() => fn({ timeRemaining: () => 10, didTimeout: false }), 1);
g.__timers = () => {
    const now = Date.now();
    for (const [id, t] of [...timers]) {
        if (t.at > now) continue;
        if (t.every) t.at = now + t.every;
        else timers.delete(id);
        try { t.fn(...t.args); } catch (e) { console.error(e); }
    }
    let next = Infinity;
    for (const t of timers.values()) next = Math.min(next, t.at);
    return next === Infinity ? -1 : Math.max(0, next - now);
};

// requests the app carries out, answered later through __host.settle
let nextAsk = 1;
const asked = new Map();
g.__ask = op => new Promise((resolve, reject) => {
    const id = nextAsk++;
    asked.set(id, { resolve, reject });
    out({ ...op, id });
});
g.__settle = (id, ok, value) => {
    const p = asked.get(id);
    if (!p) return;
    asked.delete(id);
    ok ? p.resolve(value) : p.reject(value);
};

const enc = s => {
    const out = [];
    for (const ch of String(s)) {
        let c = ch.codePointAt(0);
        if (c < 0x80) out.push(c);
        else if (c < 0x800) out.push(0xc0 | c >> 6, 0x80 | c & 63);
        else if (c < 0x10000) out.push(0xe0 | c >> 12, 0x80 | c >> 6 & 63, 0x80 | c & 63);
        else out.push(0xf0 | c >> 18, 0x80 | c >> 12 & 63, 0x80 | c >> 6 & 63, 0x80 | c & 63);
    }
    return new Uint8Array(out);
};
const dec = b => {
    b = b instanceof Uint8Array ? b : new Uint8Array(b.buffer ?? b);
    let s = "";
    for (let i = 0; i < b.length;) {
        const c = b[i++];
        let cp = c;
        if (c >= 0xf0) cp = (c & 7) << 18 | (b[i++] & 63) << 12 | (b[i++] & 63) << 6 | b[i++] & 63;
        else if (c >= 0xe0) cp = (c & 15) << 12 | (b[i++] & 63) << 6 | b[i++] & 63;
        else if (c >= 0xc0) cp = (c & 31) << 6 | b[i++] & 63;
        s += String.fromCodePoint(cp);
    }
    return s;
};
g.TextEncoder = class { encode(s = "") { return enc(s); } };
g.TextDecoder = class { decode(b) { return b ? dec(b) : ""; } };

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
g.btoa = s => {
    let o = "";
    for (let i = 0; i < s.length; i += 3) {
        const [a, b, c] = [s.charCodeAt(i), s.charCodeAt(i + 1), s.charCodeAt(i + 2)];
        o += B64[a >> 2] + B64[(a & 3) << 4 | (b || 0) >> 4] + (isNaN(b) ? "=" : B64[(b & 15) << 2 | (c || 0) >> 6]) + (isNaN(c) ? "=" : B64[c & 63]);
    }
    return o;
};
g.atob = s => {
    s = String(s).replace(/[^A-Za-z0-9+/]/g, "");
    let o = "";
    for (let i = 0; i < s.length; i += 4) {
        const [a, b, c, d] = [0, 1, 2, 3].map(k => B64.indexOf(s[i + k]));
        o += String.fromCharCode(a << 2 | b >> 4);
        if (c >= 0) o += String.fromCharCode((b & 15) << 4 | c >> 2);
        if (d >= 0) o += String.fromCharCode((c & 3) << 6 | d);
    }
    return o;
};

class URLSearchParams {
    constructor(init = "") {
        this.list = [];
        if (typeof init === "string") {
            for (const part of init.replace(/^\?/, "").split("&")) {
                if (!part) continue;
                const [k, ...v] = part.split("=");
                const d = x => { try { return decodeURIComponent(x.replace(/\+/g, " ")); } catch { return x; } };
                this.list.push([d(k), d(v.join("="))]);
            }
        } else if (init) {
            for (const [k, v] of Array.isArray(init) ? init : Object.entries(init)) this.list.push([String(k), String(v)]);
        }
    }
    get(k) { return this.list.find(e => e[0] === k)?.[1] ?? null; }
    getAll(k) { return this.list.filter(e => e[0] === k).map(e => e[1]); }
    has(k) { return this.list.some(e => e[0] === k); }
    set(k, v) { this.delete(k); this.list.push([k, String(v)]); }
    append(k, v) { this.list.push([k, String(v)]); }
    delete(k) { this.list = this.list.filter(e => e[0] !== k); }
    keys() { return this.list.map(e => e[0])[Symbol.iterator](); }
    values() { return this.list.map(e => e[1])[Symbol.iterator](); }
    entries() { return this.list.map(e => [...e])[Symbol.iterator](); }
    forEach(f) { for (const [k, v] of this.list) f(v, k, this); }
    [Symbol.iterator]() { return this.entries(); }
    get size() { return this.list.length; }
    toString() { return this.list.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&"); }
}
g.URLSearchParams = URLSearchParams;

class URL {
    constructor(url, base) {
        let s = String(url);
        if (base !== undefined && !/^[a-z][a-z0-9+.-]*:/i.test(s)) {
            const b = new URL(base);
            s = s.startsWith("/") ? b.origin + s : b.origin + b.pathname.replace(/[^/]*$/, "") + s;
        }
        const m = /^([a-z][a-z0-9+.-]*:)(?:\/\/(?:([^:@/]*)(?::([^@/]*))?@)?([^:/?#]*)(?::(\d+))?)?([^?#]*)(\?[^#]*)?(#.*)?$/i.exec(s);
        if (!m) throw new TypeError(`Invalid URL: ${s}`);
        this.protocol = m[1].toLowerCase();
        this.username = m[2] ?? "";
        this.password = m[3] ?? "";
        this.hostname = (m[4] ?? "").toLowerCase();
        this.port = m[5] ?? "";
        this.pathname = m[6] || (m[4] !== undefined ? "/" : "");
        this.searchParams = new URLSearchParams(m[7] ?? "");
        this.hash = m[8] ?? "";
    }
    get host() { return this.hostname + (this.port ? ":" + this.port : ""); }
    set host(h) { [this.hostname, this.port = ""] = h.split(":"); }
    get origin() { return `${this.protocol}//${this.host}`; }
    get search() { const q = this.searchParams.toString(); return q ? "?" + q : ""; }
    set search(q) { this.searchParams = new URLSearchParams(q); }
    get href() { return `${this.protocol}${this.hostname || this.protocol === "file:" ? "//" : ""}${this.username ? this.username + (this.password ? ":" + this.password : "") + "@" : ""}${this.host}${this.pathname}${this.search}${this.hash}`; }
    set href(h) { Object.assign(this, new URL(h)); }
    toString() { return this.href; }
    toJSON() { return this.href; }
    static canParse(u, b) { try { new URL(u, b); return true; } catch { return false; } }
}
g.URL = URL;

class Headers {
    constructor(h) {
        this.map = {};
        if (h) for (const [k, v] of h instanceof Headers ? Object.entries(h.map) : Array.isArray(h) ? h : Object.entries(h)) this.map[k.toLowerCase()] = String(v);
    }
    get(k) { return this.map[k.toLowerCase()] ?? null; }
    set(k, v) { this.map[k.toLowerCase()] = String(v); }
    append(k, v) { this.set(k, v); }
    has(k) { return k.toLowerCase() in this.map; }
    delete(k) { delete this.map[k.toLowerCase()]; }
    forEach(f) { for (const k in this.map) f(this.map[k], k, this); }
}
g.Headers = Headers;

class Blob {
    constructor(parts = [], opts = {}) { this.parts = parts; this.type = opts.type ?? ""; }
    async text() { return this.parts.map(p => typeof p === "string" ? p : dec(p)).join(""); }
    get size() { return this.parts.reduce((n, p) => n + (p.length ?? p.byteLength ?? 0), 0); }
}
g.Blob = Blob;
g.File = class extends Blob { constructor(parts, name, opts) { super(parts, opts); this.name = name; } };

class Response {
    constructor(r) {
        this.status = r.status;
        this.ok = r.status >= 200 && r.status < 300;
        this.statusText = "";
        this.url = r.url;
        this.headers = new Headers(r.headers ?? {});
        this.body64 = r.body ?? "";
    }
    bytes() { const s = atob(this.body64); const b = new Uint8Array(s.length); for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i); return b; }
    async text() { return dec(this.bytes()); }
    async json() { return JSON.parse(await this.text()); }
    async arrayBuffer() { return this.bytes().buffer; }
    async blob() { return new Blob([this.bytes()], { type: this.headers.get("content-type") ?? "" }); }
}
g.Response = Response;
g.fetch = async (url, init = {}) => {
    const r = await __ask({ op: "fetch", url: String(url), method: init.method ?? "GET", headers: new Headers(init.headers).map, body: init.body == null ? null : String(init.body) });
    return new Response(r);
};

const memory = new Map();
g.localStorage = g.sessionStorage = {
    getItem: k => memory.has(k) ? memory.get(k) : null,
    setItem: (k, v) => void memory.set(k, String(v)),
    removeItem: k => void memory.delete(k),
    clear: () => memory.clear(),
    key: i => [...memory.keys()][i] ?? null,
    get length() { return memory.size; },
};

g.navigator = {
    userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Rustcord",
    language: "en-US",
    languages: ["en-US"],
    platform: "Win32",
    clipboard: { writeText: async t => out({ op: "copy", text: String(t) }), readText: async () => "" },
};

g.SpeechSynthesisUtterance = class { constructor(text = "") { this.text = text; this.volume = 1; this.rate = 1; this.pitch = 1; this.voice = null; } };
g.speechSynthesis = {
    speak: u => out({ op: "speak", text: String(u.text), volume: +u.volume, rate: +u.rate }),
    cancel() {}, pause() {}, resume() {}, getVoices: () => [], addEventListener() {}, removeEventListener() {},
    speaking: false, pending: false, onvoiceschanged: null,
};

g.Notification = class { static permission = "granted"; static requestPermission = async () => "granted"; constructor(title, o = {}) { out({ op: "notify", title: String(title), body: String(o.body ?? "") }); } close() {} };
g.Audio = class { constructor(src) { this.src = src; this.volume = 1; } play() { return Promise.resolve(); } pause() {} addEventListener() {} };
g.Image = class { constructor() { this.onload = null; } set src(v) { this._src = v; } get src() { return this._src; } };

const element = tag => ({
    tagName: String(tag).toUpperCase(), style: {}, dataset: {}, children: [], childNodes: [],
    classList: { add() {}, remove() {}, toggle() {}, contains: () => false },
    appendChild(c) { this.children.push(c); return c; }, append(...c) { this.children.push(...c); }, prepend() {}, removeChild(c) { return c; }, remove() {},
    setAttribute() {}, getAttribute: () => null, removeAttribute() {}, addEventListener() {}, removeEventListener() {},
    querySelector: () => null, querySelectorAll: () => [], getElementsByTagName: () => [], getElementsByClassName: () => [],
    getBoundingClientRect: () => ({ x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 }),
    focus() {}, blur() {}, click() {}, contains: () => false, insertAdjacentElement() {}, insertBefore(c) { return c; }, cloneNode() { return element(tag); },
    textContent: "", innerHTML: "", innerText: "", value: "", isConnected: false,
});
g.document = {
    ...element("document"), readyState: "complete", visibilityState: "visible", hidden: false,
    createElement: element, createElementNS: (_, t) => element(t), createTextNode: t => ({ textContent: t }), createDocumentFragment: () => element("fragment"),
    getElementById: () => null, head: element("head"), body: element("body"), documentElement: element("html"), hasFocus: () => true,
};
g.addEventListener = g.removeEventListener = g.dispatchEvent = () => {};
g.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} });
g.getComputedStyle = () => ({ getPropertyValue: () => "" });
g.location = new URL("https://discord.com/channels/@me");
g.history = { pushState() {}, replaceState() {}, back() {}, forward() {} };
g.innerWidth = 1280;
g.innerHeight = 800;
g.devicePixelRatio = 1;
g.performance = { now: () => Date.now() };
g.crypto = { randomUUID: () => "10000000-1000-4000-8000-100000000000".replace(/[018]/g, c => (c ^ Math.random() * 256 & 15 >> c / 4).toString(16)), getRandomValues: a => { for (let i = 0; i < a.length; i++) a[i] = Math.random() * 256 | 0; return a; } };
g.structuredClone = v => v === undefined ? v : JSON.parse(JSON.stringify(v));
g.MutationObserver = g.ResizeObserver = g.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
g.AbortController = class { constructor() { this.signal = { aborted: false, addEventListener() {} }; } abort() { this.signal.aborted = true; } };
g.Event = class { constructor(type) { this.type = type; } };
g.CustomEvent = class extends g.Event { constructor(type, o = {}) { super(type); this.detail = o.detail; } };
g.EventTarget = class { addEventListener() {} removeEventListener() {} dispatchEvent() {} };
g.DOMParser = class { parseFromString() { return g.document; } };
g.FileReader = class { readAsDataURL(b) { this.result = `data:${b.type};base64,`; setTimeout(() => this.onload?.({ target: this })); } readAsText(b) { b.text().then(t => { this.result = t; this.onload?.({ target: this }); }); } };
g.Worker = class { postMessage() {} terminate() {} addEventListener() {} };
g.WebSocket = class { constructor() { this.readyState = 3; } send() {} close() {} addEventListener() {} };

// anything Rustcord can't give: callable, every property another stub, falsy when called
const stubs = new Map();
const stub = name => {
    let s = stubs.get(name);
    if (s) return s;
    const fn = function () {};
    s = new Proxy(fn, {
        get(_, k) {
            if (k === Symbol.toPrimitive) return () => "";
            if (k === Symbol.iterator) return function* () {};
            if (k === "then" || k === "$$vnode") return undefined;
            if (k === "__fcStub") return true;
            if (k === "__esModule") return true;
            if (k === "toString" || k === "valueOf") return () => "";
            if (k === "prototype") return {};
            if (k === "name" || k === "displayName") return name;
            return stub(`${name}.${String(k)}`);
        },
        apply: () => undefined,
        construct: () => ({}),
        has: () => false,
        ownKeys: () => ["prototype"],
        getOwnPropertyDescriptor: (_, k) => k === "prototype" ? { value: {}, writable: true } : undefined,
    });
    stubs.set(name, s);
    return s;
};
g.__stub = stub;
// a plugin's Electron-side half can't run here
g.VencordNative = stub("VencordNative");

if (!g.Intl) {
    const date = (d, o = {}) => {
        d = new Date(d ?? Date.now());
        if (o.timeStyle && !o.dateStyle) return d.toLocaleTimeString();
        if (o.dateStyle && !o.timeStyle) return d.toLocaleDateString();
        return d.toLocaleString();
    };
    g.Intl = {
        NumberFormat: class { constructor(_, o = {}) { this.o = o; } format(n) { return this.o.notation === "compact" && Math.abs(n) >= 1000 ? (n / 1000).toFixed(1).replace(/\.0$/, "") + "K" : Number(n).toLocaleString(); } formatToParts(n) { return [{ type: "integer", value: this.format(n) }]; } resolvedOptions() { return { locale: "en-US", ...this.o }; } },
        DateTimeFormat: class { constructor(_, o) { this.o = o; } format(d) { return date(d, this.o); } formatToParts(d) { return [{ type: "literal", value: date(d, this.o) }]; } resolvedOptions() { return { locale: "en-US", timeZone: "UTC", ...this.o }; } },
        PluralRules: class { select(n) { return n === 1 ? "one" : "other"; } },
        RelativeTimeFormat: class { format(n, unit) { const u = unit.replace(/s$/, "") + (Math.abs(n) === 1 ? "" : "s"); return n < 0 ? `${-n} ${u} ago` : `in ${n} ${u}`; } },
        Collator: class { compare(a, b) { return String(a).localeCompare(String(b)); } },
        ListFormat: class { format(l) { l = [...l]; return l.length < 2 ? l.join("") : `${l.slice(0, -1).join(", ")} and ${l.at(-1)}`; } },
        Segmenter: class { segment(s) { return [...s].map((segment, index) => ({ segment, index, input: s })); } },
        DisplayNames: class { of(c) { return c; } },
        getCanonicalLocales: l => [].concat(l ?? []),
        supportedValuesOf: () => [],
    };
}

g.__star = (exp, m) => {
    if (!m || m.__fcStub) return;
    for (const k of Object.keys(m)) if (k !== "default" && !(k in exp)) Object.defineProperty(exp, k, { enumerable: true, get: () => m[k] });
};

// modules by canonical path; a missing one is a stub, a broken one too after it says why.
// Vencord's own modules hand out stubs for any name they don't have, since a plugin may
// import something from Discord that Rustcord never made.
const modules = new Map();
const load = id => {
    let m = modules.get(id);
    if (m) return m.exports;
    m = { exports: {}, id };
    modules.set(id, m);
    const plugin = id.startsWith("src/plugins/") || id.startsWith("src/userplugins/");
    try {
        (0, eval)(fc.source(id))(m.exports, spec => require(id, spec), m);
        if (!plugin) {
            const own = m.exports;
            m.exports = new Proxy(own, { get: (t, k) => k in t || typeof k === "symbol" || k === "then" ? t[k] : stub(`${id}.${k}`) });
        }
    } catch (e) {
        console.error(`couldn't load ${id}:`, e);
        m.error = e;
        if (!plugin) m.exports = stub(id);
    }
    return m.exports;
};
const require = (from, spec) => {
    // text files and stylesheets bundled in at Vencord's build
    if (spec.startsWith("file://") || /\.css(\?|$)/.test(spec)) return { default: "", __esModule: true };
    const id = fc.resolve(from, spec);
    return id == null ? stub(spec) : load(id);
};
g.__load = load;
g.__module = id => modules.get(id);

// JSX, as Vencord's build compiles it
g.VencordFragment = "#fragment";
g.VencordCreateElement = (type, props, ...children) => {
    props = { ...props };
    if (children.length) props.children = children.length === 1 ? children[0] : children;
    const key = props.key;
    delete props.key;
    return { $$vnode: true, type, props, key };
};
})();
