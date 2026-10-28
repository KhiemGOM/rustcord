// @api/Settings: Vencord's settings tree, in Vencord's settings.json shape. A plugin's options
// read their defaults until set, and every change is saved by the app.
// Vencord's defaults under whatever the settings file has
const DEFAULTS = {
    autoUpdate: true, autoUpdateNotification: true, useQuickCss: true, themeLinks: [], enabledThemes: [], enableReactDevtools: false,
    frameless: false, transparent: false, winCtrlQ: false, disableMinSize: false, winNativeTitleBar: false, plugins: {},
    notifications: { timeout: 5000, position: "bottom-right", useNative: "not-focused", logLimit: 50 },
    cloud: { authenticated: false, url: "https://api.vencord.dev/", settingsSync: false, settingsSyncVersion: 0 },
    uiElements: { chatBarButtons: {}, messagePopoverButtons: {} },
};
const raw = globalThis.__fcSettings ??= {};
for (const [k, v] of Object.entries(DEFAULTS)) {
    if (!(k in raw)) raw[k] = structuredClone(v);
    else if (v && typeof v === "object" && !Array.isArray(v)) for (const [k2, v2] of Object.entries(v)) if (!(k2 in raw[k])) raw[k][k2] = v2;
}
export const defs = new Map();

const fallback = (plugin, key) => {
    const d = defs.get(plugin)?.[key];
    if (!d) return undefined;
    if ("default" in d) return d.default;
    if (d.type === 4) return d.options?.find(o => o.default)?.value;
    return undefined;
};

let saving = false;
export const save = () => {
    if (saving) return;
    saving = true;
    Promise.resolve().then(() => {
        saving = false;
        __out({ op: "settings", value: raw });
    });
};

const listeners = new Set();
const changed = path => {
    save();
    for (const l of listeners) try { l(path); } catch (e) { console.error(e); }
};

const pluginStore = name => new Proxy({}, {
    get(_, k) {
        const own = raw.plugins[name] ??= {};
        return k in own ? own[k] : fallback(name, k);
    },
    set(_, k, v) {
        (raw.plugins[name] ??= {})[k] = v;
        changed(`plugins.${name}.${String(k)}`);
        return true;
    },
    has(_, k) { return k in (raw.plugins[name] ?? {}) || defs.get(name)?.[k] !== undefined; },
    ownKeys() { return [...new Set([...Object.keys(raw.plugins[name] ?? {}), ...Object.keys(defs.get(name) ?? {})])]; },
    getOwnPropertyDescriptor(t, k) { return { enumerable: true, configurable: true, writable: true, value: this.get(t, k) }; },
    deleteProperty(_, k) { delete raw.plugins[name]?.[k]; changed(`plugins.${name}.${String(k)}`); return true; },
});

const stores = new Map();
const plugins = new Proxy({}, {
    get(_, name) {
        if (typeof name !== "string") return undefined;
        let s = stores.get(name);
        if (!s) stores.set(name, s = pluginStore(name));
        return s;
    },
    set(_, name, v) { raw.plugins[name] = v; changed(`plugins.${name}`); return true; },
    ownKeys: () => Object.keys(raw.plugins),
    getOwnPropertyDescriptor: () => ({ enumerable: true, configurable: true }),
    has: (_, name) => name in raw.plugins,
});

export const Settings = new Proxy(raw, {
    get(t, k) { return k === "plugins" ? plugins : t[k]; },
    set(t, k, v) { t[k] = v; changed(String(k)); return true; },
});
export const PlainSettings = raw;
export const SettingsStore = {
    store: Settings,
    plain: raw,
    addChangeListener: (_, l) => listeners.add(l),
    removeChangeListener: (_, l) => listeners.delete(l),
    addGlobalChangeListener: l => listeners.add(l),
    removeGlobalChangeListener: l => listeners.delete(l),
    markAsChanged: () => save(),
};
export const useSettings = () => Settings;

export function definePluginSettings(def, checks) {
    if (checks) for (const [k, c] of Object.entries(checks)) Object.assign(def[k], c);
    const s = {
        def,
        pluginName: "",
        get store() { return plugins[s.pluginName]; },
        get plain() { return raw.plugins[s.pluginName] ?? {}; },
        use: () => plugins[s.pluginName],
        withPrivateSettings() { return this; },
    };
    return s;
}

export function migratePluginSettings(name, ...oldNames) {
    if (name in raw.plugins) return;
    const old = oldNames.find(n => n in raw.plugins);
    if (old) {
        raw.plugins[name] = raw.plugins[old];
        delete raw.plugins[old];
        save();
    }
}

export function migratePluginSetting(plugin, oldSetting, newSetting) {
    const p = raw.plugins[plugin];
    if (p && oldSetting in p && !(newSetting in p)) {
        p[newSetting] = p[oldSetting];
        delete p[oldSetting];
        save();
    }
}
