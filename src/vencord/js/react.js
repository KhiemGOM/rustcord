// React, as much as plugin UI needs to run for real: a root keeps each component's hooks
// between renders, setState marks the root for drawing again, effects run after a render
// and clean up when their component goes. What a root renders is a tree of plain elements
// (html and Fastcord's own fc-* controls) that Fastcord draws natively.
export const Fragment = VencordFragment;
export const createElement = VencordCreateElement;
export const isValidElement = x => !!x?.$$vnode;
export const cloneElement = (el, props, ...children) => ({ ...el, props: { ...el.props, ...props, ...(children.length ? { children: children.length === 1 ? children[0] : children } : {}) } });

// the component being rendered: its root, its hook list and the next hook's index
let cur = null;
const scratch = () => ({ root: { dirtyNow() {}, usesStores: false }, hooks: [], i: 0 });
const hook = init => {
    const c = cur ?? scratch();
    const i = c.i++;
    if (!(i in c.hooks)) c.hooks[i] = init();
    return c.hooks[i];
};
const changed = (a, b) => !a || !b || a.length !== b.length || a.some((x, i) => !Object.is(x, b[i]));

export function useState(init) {
    const root = cur?.root;
    const h = hook(() => ({ v: typeof init === "function" ? init() : init }));
    h.set ??= v => {
        const next = typeof v === "function" ? v(h.v) : v;
        if (Object.is(next, h.v)) return;
        h.v = next;
        root?.dirtyNow();
    };
    return [h.v, h.set];
}
export function useReducer(reducer, init, f) {
    const [s, set] = useState(() => f ? f(init) : init);
    return [s, a => set(s => reducer(s, a))];
}
export const useRef = v => hook(() => ({ current: v }));
export function useMemo(f, deps) {
    const h = hook(() => ({}));
    if (!("v" in h) || changed(h.deps, deps)) { h.v = f(); h.deps = deps; }
    return h.v;
}
export const useCallback = (f, deps) => useMemo(() => f, deps);
export function useEffect(f, deps) {
    const h = hook(() => ({}));
    if (cur && (!h.ran || changed(h.deps, deps))) {
        h.deps = deps;
        h.ran = true;
        cur.root.effects.push(() => {
            try { h.cleanup?.(); } catch (e) { console.error(e); }
            try { const c = f(); h.cleanup = typeof c === "function" ? c : undefined; } catch (e) { console.error("effect threw:", e); }
        });
        cur.state.cleanups.add(h);
    }
}
export const useLayoutEffect = useEffect;
export const useInsertionEffect = useEffect;
export function useContext(c) {
    const stack = cur?.root.context;
    for (let i = (stack?.length ?? 0) - 1; i >= 0; i--) if (stack[i][0] === c) return stack[i][1];
    return c?._value;
}
export const useId = () => useMemo(() => "fc" + Math.random().toString(36).slice(2), []);
export function useSyncExternalStore(_, get) {
    if (cur) cur.root.usesStores = true;
    return get();
}
export const useTransition = () => [false, f => f()];
export const useDeferredValue = v => v;
export const useImperativeHandle = () => {};
export const useDebugValue = () => {};

export const createContext = v => {
    const c = { _value: v };
    c.Provider = Object.assign(p => p.children, { $$provider: c });
    c.Consumer = p => p.children?.(useContext(c));
    return c;
};
export const memo = c => c;
export const forwardRef = render => Object.assign(props => render(props, props.ref ?? null), { displayName: render.displayName ?? render.name });
export const lazy = load => {
    let C = null;
    let started = false;
    return props => {
        const [, force] = useState(0);
        if (!started) { started = true; load().then(m => { C = m.default ?? m; force(1); }); }
        return C ? createElement(C, props) : null;
    };
};
export const Suspense = p => p.children;
export const StrictMode = p => p.children;
export const createRef = () => ({ current: null });
export class Component {
    constructor(props) { this.props = props; this.state = {}; }
    setState(s, cb) {
        this.state = { ...this.state, ...(typeof s === "function" ? s(this.state, this.props) : s) };
        this.__root?.dirtyNow();
        cb?.();
    }
    forceUpdate() { this.__root?.dirtyNow(); }
}
export class PureComponent extends Component {}
const flat = c => c == null ? [] : Array.isArray(c) ? c.flat(Infinity) : [c];
export const Children = {
    map: (c, f) => flat(c).map(f),
    forEach: (c, f) => flat(c).forEach(f),
    toArray: flat,
    count: c => flat(c).length,
    only: c => flat(c)[0],
};

// roots: one per thing Fastcord draws from plugin UI (a modal, a settings panel, a message's accessories)
export const roots = new Map();
const dirty = new Set();

class Root {
    constructor(id, el) {
        this.id = id;
        this.el = el;
        this.states = new Map();
        this.effects = [];
        this.context = [];
        this.handlers = new Map();
        this.usesStores = false;
    }
    dirtyNow() {
        if (dirty.has(this.id) || !roots.has(this.id)) return;
        dirty.add(this.id);
        __out({ op: "dirty", root: this.id });
    }
}

export function mount(id, el) {
    let r = roots.get(id);
    if (!r) roots.set(id, r = new Root(id, el));
    r.el = el;
    return r;
}

export function unmount(id) {
    const r = roots.get(id);
    if (!r) return;
    roots.delete(id);
    dirty.delete(id);
    for (const s of r.states.values()) for (const h of s.cleanups) try { h.cleanup?.(); } catch (e) { console.error(e); }
}

export const storeRoots = () => [...roots.values()].filter(r => r.usesStores).map(r => r.id);

const KEEP = new Set(["viewBox", "preserveAspectRatio", "gradientUnits", "gradientTransform", "patternUnits", "clipPathUnits", "maskUnits", "spreadMethod"]);
export function svgMarkup(n) {
    if (typeof n === "string" || typeof n === "number") return String(n).replace(/&/g, "&amp;").replace(/</g, "&lt;");
    if (!n?.$$vnode) return "";
    if (typeof n.type !== "string") return n.type === Fragment ? flat(n.props.children).map(svgMarkup).join("") : svgMarkup(callPlain(n));
    let attrs = "";
    for (const [k, v] of Object.entries(n.props)) {
        if (["children", "className", "style", "ref", "key", "role"].includes(k) || k.startsWith("on") || k.startsWith("aria") || v == null || v === false || typeof v === "object" || typeof v === "function") continue;
        const name = KEEP.has(k) ? k : k.replace(/[A-Z]/g, c => "-" + c.toLowerCase());
        attrs += ` ${name}="${String(v).replace(/currentColor|var\([^)]*\)/g, "#ffffff").replace(/"/g, "&quot;")}"`;
    }
    if (n.type === "svg") {
        if (!("xmlns" in n.props)) attrs += ' xmlns="http://www.w3.org/2000/svg"';
        if (!("fill" in n.props)) attrs += ' fill="#ffffff"';
    }
    return `<${n.type}${attrs}>${flat(n.props.children).map(svgMarkup).join("")}</${n.type}>`;
}
const callPlain = n => { try { return n.type.prototype?.render ? new n.type(n.props).render() : n.type(n.props); } catch { return null; } };

const STYLE = new Set(["color", "backgroundColor", "background", "fontSize", "fontWeight", "fontStyle", "fontFamily", "textDecoration", "display", "flexDirection", "gap", "justifyContent", "alignItems", "flexWrap", "margin", "marginTop", "marginBottom", "marginLeft", "marginRight", "marginBlock", "marginInline", "padding", "paddingTop", "paddingBottom", "paddingLeft", "paddingRight", "width", "height", "maxWidth", "maxHeight", "minWidth", "minHeight", "borderRadius", "textAlign", "opacity", "whiteSpace"]);
const plainValue = v => v == null || ["string", "number", "boolean"].includes(typeof v);

/// Draws `root`'s element into plain data, components run with their hooks.
export function render(id) {
    const r = roots.get(id);
    if (!r) return null;
    dirty.delete(id);
    r.handlers = new Map();
    r.effects = [];
    r.context = [];
    const seen = new Set();
    let next = 1;
    const handler = fn => {
        const hid = String(next++);
        r.handlers.set(hid, fn);
        return hid;
    };
    const walk = (node, path) => {
        if (node == null || typeof node === "boolean") return [];
        if (typeof node === "string" || typeof node === "number" || typeof node === "bigint") return [String(node)];
        if (Array.isArray(node)) return node.flatMap((c, i) => walk(c, `${path}.${c?.key ?? i}`));
        if (!node.$$vnode) return [];
        const { type, props } = node;
        if (type === Fragment) return walk(props.children, path);
        if (type?.$$provider) {
            r.context.push([type.$$provider, props.value]);
            const out = walk(props.children, path);
            r.context.pop();
            return out;
        }
        if (typeof type === "function") {
            if (type.__fcStub) return [];
            const key = `${path}/${type.displayName ?? type.name ?? "c"}`;
            seen.add(key);
            let state = r.states.get(key);
            if (!state) r.states.set(key, state = { hooks: [], cleanups: new Set() });
            const prev = cur;
            cur = { root: r, state, hooks: state.hooks, i: 0 };
            let out;
            try {
                if (type.prototype?.render) {
                    const inst = state.inst ??= new type(props);
                    inst.props = props;
                    inst.__root = r;
                    out = inst.render();
                } else {
                    out = type(props);
                }
            } catch (e) {
                console.error(`plugin component ${type.displayName ?? type.name ?? ""} threw:`, e);
                out = null;
            } finally {
                cur = prev;
            }
            return walk(out, key);
        }
        if (typeof type !== "string") return [];
        if (type === "svg") return [{ t: "svg", s: svgMarkup(node), w: props.width ?? null, h: props.height ?? null }];
        const p = {};
        const h = {};
        for (const [k, v] of Object.entries(props)) {
            if (k === "children" || k === "key" || k === "ref") continue;
            if (typeof v === "function") h[k] = handler(v);
            else if (k === "style" && v && typeof v === "object") p.style = Object.fromEntries(Object.entries(v).filter(([s, x]) => STYLE.has(s) && plainValue(x)));
            else if (k === "options" && Array.isArray(v)) p.options = v.map(o => ({ label: typeof o.label === "string" ? o.label : walk(o.label, path).filter(x => typeof x === "string").join(""), value: o.value, default: !!o.default }));
            else if (k === "markers" && Array.isArray(v)) p.markers = v.filter(x => typeof x === "number");
            else if (k === "label" || k === "title" || k === "note" || k === "description" || k === "tooltip") p[k] = typeof v === "object" ? walk(v, `${path}.${k}`) : v;
            else if (plainValue(v)) p[k] = v;
        }
        return [{ t: type, p, h, c: walk(props.children, path) }];
    };
    const tree = walk(r.el, "");
    for (const [k, s] of r.states) {
        if (seen.has(k)) continue;
        for (const hk of s.cleanups) try { hk.cleanup?.(); } catch (e) { console.error(e); }
        r.states.delete(k);
    }
    for (const e of r.effects) e();
    r.effects = [];
    return tree;
}

/// A handler from the last render, called with what the control gave it.
export function fire(id, hid, args) {
    const fn = roots.get(id)?.handlers.get(hid);
    if (!fn) return false;
    // a dom event from Fastcord arrives as plain data; plugins call its methods
    const ev = a => a?.$event ? { preventDefault() {}, stopPropagation() {}, persist() {}, target: { value: a.value, checked: a.checked }, currentTarget: { value: a.value, checked: a.checked }, shiftKey: !!a.shift, ctrlKey: !!a.ctrl, altKey: !!a.alt, metaKey: false, button: a.button ?? 0, detail: 1, key: a.key } : a;
    try { fn(...args.map(ev)); } catch (e) { console.error("plugin UI handler threw:", e); }
    return true;
}

const React = { Fragment, createElement, isValidElement, cloneElement, useState, useReducer, useEffect, useLayoutEffect, useInsertionEffect, useMemo, useCallback, useRef, useContext, useId, useSyncExternalStore, useTransition, useDeferredValue, useImperativeHandle, useDebugValue, createContext, memo, forwardRef, lazy, Suspense, StrictMode, createRef, Component, PureComponent, Children, version: "19.0.0" };
export default React;
