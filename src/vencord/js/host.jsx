// The plugin manager, and what the app calls: plugins load, start and stop as in Vencord's
// PluginManager, gateway events reach them as Discord's flux actions, and what they add to
// menus and bars comes back as data for Rustcord to draw.
import { Settings, defs } from "@api/Settings";
import { commands, registerCommand, unregisterCommand } from "@api/Commands";
import * as events from "@api/MessageEvents";
import { addContextMenuPatch, build, removeContextMenuPatch } from "@api/ContextMenu";
import { ChatBarButtonMap, addChatBarButton, removeChatBarButton } from "@api/ChatButtons";
import { MessagePopoverButtonMap, addMessagePopoverButton, removeMessagePopoverButton } from "@api/MessagePopover";
import { ChannelStore, FluxDispatcher, GuildStore, Message, UserStore, closeModal, getUser } from "@webpack/common";
import { addMessageAccessory, accessories, addMessageDecoration, decorationsFactories, addMemberListDecorator, decoratorsFactories, removeMessageAccessory, removeMessageDecoration, removeMemberListDecorator } from "@api/MessageAccessories";
import { _getBadges, addProfileBadge, removeProfileBadge } from "@api/Badges";
import { fire, mount, render, roots, storeRoots, unmount } from "react";
import { clearLog, deleteNotification, getLog, handlers as notifyHandlers } from "@api/Notifications";
import { PlainSettings, save } from "@api/Settings";
import * as DataStore from "@api/DataStore";

const plugins = new Map();

function load(path) {
    for (const e of plugins.values()) if (e.path === path) return e;
    const m = __load(path);
    const failed = __module(path)?.error;
    const p = m?.default;
    if (failed || !p?.name) {
        const e = { path, name: path.split("/")[2], error: String(failed ?? "not a plugin") };
        plugins.set(e.name, e);
        return e;
    }
    const e = { path, name: p.name, p, started: false };
    if (p.settings) {
        p.settings.pluginName = p.name;
        defs.set(p.name, p.settings.def);
    } else if (p.options) {
        defs.set(p.name, p.options);
    }
    plugins.set(p.name, e);
    return e;
}

const enabled = e => e.p && (Settings.plugins[e.name].enabled ?? (!!e.p.enabledByDefault || !!e.p.required));

function start(e) {
    if (e.started || !e.p) return;
    const p = e.p;
    try {
        for (const c of p.commands ?? []) registerCommand(c, p.name);
        e.flux = Object.entries(p.flux ?? {}).map(([type, fn]) => {
            const h = a => { try { const r = fn.call(p, a); r?.catch?.(err => console.error(`${p.name} ${type}:`, err)); } catch (err) { console.error(`${p.name} ${type}:`, err); } };
            FluxDispatcher.subscribe(type, h);
            return [type, h];
        });
        for (const [nav, patch] of Object.entries(p.contextMenus ?? {})) addContextMenuPatch(nav, patch);
        if (p.chatBarButton) addChatBarButton(p.name, p.chatBarButton.render, p.chatBarButton.icon);
        if (p.messagePopoverButton) addMessagePopoverButton(p.name, p.messagePopoverButton.render, p.messagePopoverButton.icon);
        if (p.onBeforeMessageSend) events.addMessagePreSendListener(e.send = p.onBeforeMessageSend.bind(p));
        if (p.onBeforeMessageEdit) events.addMessagePreEditListener(e.edit = p.onBeforeMessageEdit.bind(p));
        if (p.onMessageClick) events.addMessageClickListener(e.click = p.onMessageClick.bind(p));
        if (p.renderMessageAccessory) addMessageAccessory(p.name, p.renderMessageAccessory);
        if (p.renderMessageDecoration) addMessageDecoration(p.name, p.renderMessageDecoration);
        if (p.renderMemberListDecorator) addMemberListDecorator(p.name, p.renderMemberListDecorator);
        if (p.userProfileBadge) addProfileBadge(p.userProfileBadge);
        e.started = true;
        p.start?.();
    } catch (err) {
        e.error = String(err?.stack ?? err);
        console.error(`${p.name} didn't start:`, err);
    }
}

function stop(e) {
    if (!e.started) return;
    const p = e.p;
    e.started = false;
    try { p.stop?.(); } catch (err) { console.error(`${p.name} didn't stop cleanly:`, err); }
    for (const c of p.commands ?? []) unregisterCommand(c.name);
    for (const [type, h] of e.flux ?? []) FluxDispatcher.unsubscribe(type, h);
    for (const [nav, patch] of Object.entries(p.contextMenus ?? {})) removeContextMenuPatch(nav, patch);
    removeChatBarButton(p.name);
    removeMessagePopoverButton(p.name);
    removeMessageAccessory(p.name);
    removeMessageDecoration(p.name);
    removeMemberListDecorator(p.name);
    if (p.userProfileBadge) removeProfileBadge(p.userProfileBadge);
    for (const id of [...roots.keys()]) if (id.startsWith(`about:${p.name}`) || id.startsWith(`setting:${p.name}:`)) unmount(id);
    events.removeMessagePreSendListener(e.send);
    events.removeMessagePreEditListener(e.edit);
    events.removeMessageClickListener(e.click);
}

// gateway dispatches as the actions Discord's client makes of them
function actions(t, d, x = {}) {
    switch (t) {
        case "READY": return [{ type: "CONNECTION_OPEN" }, { type: "POST_CONNECTION_OPEN" }];
        case "MESSAGE_CREATE": return [{ type: t, channelId: d.channel_id, guildId: d.guild_id, message: d, optimistic: false, isPushNotification: false }];
        case "MESSAGE_UPDATE": return [{ type: t, message: d, guildId: d.guild_id }];
        case "MESSAGE_DELETE": return [{ type: t, id: d.id, channelId: d.channel_id, guildId: d.guild_id }];
        case "MESSAGE_DELETE_BULK": return [{ type: t, ids: d.ids, channelId: d.channel_id, guildId: d.guild_id }];
        case "MESSAGE_REACTION_ADD":
        case "MESSAGE_REACTION_REMOVE": return [{ type: t, channelId: d.channel_id, messageId: d.message_id, userId: d.user_id, emoji: d.emoji, burst: !!d.burst, guildId: d.guild_id, optimistic: false }];
        case "TYPING_START": return [{ type: t, channelId: d.channel_id, userId: d.user_id, guildId: d.guild_id }];
        case "PRESENCE_UPDATE": return [{ type: "PRESENCE_UPDATES", updates: [{ user: d.user, status: d.status, clientStatus: d.client_status, activities: d.activities, guildId: d.guild_id }] }];
        case "VOICE_STATE_UPDATE": return [{
            type: "VOICE_STATE_UPDATES",
            voiceStates: [{ userId: d.user_id, channelId: d.channel_id, oldChannelId: x.oldChannel ?? null, guildId: d.guild_id, sessionId: d.session_id, mute: d.mute, deaf: d.deaf, selfMute: d.self_mute, selfDeaf: d.self_deaf, selfVideo: d.self_video, selfStream: d.self_stream, suppress: d.suppress }],
        }];
        case "RELATIONSHIP_ADD":
        case "RELATIONSHIP_UPDATE":
        case "RELATIONSHIP_REMOVE": return [{ type: t, relationship: { ...d, type: d.type ?? x.kind } }];
        case "GUILD_CREATE": return [{ type: t, guild: d }];
        case "GUILD_DELETE": return [{ type: t, guild: { id: d.id, unavailable: !!d.unavailable } }];
        case "CHANNEL_CREATE":
        case "CHANNEL_UPDATE":
        case "CHANNEL_DELETE": return [{ type: t, channel: d }];
        case "CHANNEL_RECIPIENT_ADD":
        case "CHANNEL_RECIPIENT_REMOVE": return [{ type: t, channelId: d.channel_id, user: d.user }];
        case "CALL_CREATE":
        case "CALL_UPDATE": return [{ type: t, channelId: d.channel_id, ringing: d.ringing ?? [], call: { channel_id: d.channel_id, ringing: d.ringing ?? [], message_id: d.message_id, region: d.region } }];
        case "CALL_DELETE": return [{ type: t, channelId: d.channel_id }];
        case "MESSAGE_ACK": return [{ type: t, channelId: d.channel_id, messageId: d.message_id }];
        case "GUILD_MEMBER_UPDATE": return [{ type: t, guildId: d.guild_id, user: d.user, roles: d.roles, nick: d.nick }];
        default: return [{ type: t, ...d }];
    }
}

// what plugins draw, read off their elements: functions become action ids the app hands back
const scopes = {};
const act = (scope, fn) => {
    if (typeof fn !== "function") return null;
    const s = scopes[scope] ??= { next: 1, fns: new Map() };
    const id = `${scope}:${s.next++}`;
    s.fns.set(id, fn);
    return id;
};
const reset = scope => void (scopes[scope] = { next: 1, fns: new Map() });

const expand = node => {
    if (node == null || typeof node === "boolean") return [];
    if (Array.isArray(node)) return node.flatMap(expand);
    if (typeof node !== "object") return [String(node)];
    if (!node.$$vnode) return [];
    const { type, props } = node;
    if (type === VencordFragment) return expand(props.children);
    if (typeof type === "function") {
        if (type.__fcStub) return [];
        try { return expand(type.prototype?.render ? new type(props).render() : type(props)); } catch (e) { console.error("rendering a plugin element threw:", e); return []; }
    }
    return typeof type === "string" ? [node] : [];
};
const text = node => expand(node).map(n => typeof n === "string" ? n : text(n.props.children)).join("");

const KEEP = new Set(["viewBox", "preserveAspectRatio", "gradientUnits", "gradientTransform", "patternUnits", "clipPathUnits", "maskUnits", "spreadMethod"]);
const markup = n => {
    if (typeof n === "string") return n.replace(/&/g, "&amp;").replace(/</g, "&lt;");
    let attrs = "";
    for (const [k, v] of Object.entries(n.props)) {
        if (["children", "className", "style", "ref", "key", "role", "aria-hidden"].includes(k) || k.startsWith("on") || v == null || v === false || typeof v === "object" || typeof v === "function") continue;
        const name = KEEP.has(k) ? k : k.replace(/[A-Z]/g, c => "-" + c.toLowerCase());
        const val = String(v).replace(/currentColor|var\([^)]*\)/g, "#ffffff").replace(/"/g, "&quot;");
        attrs += ` ${name}="${val}"`;
    }
    if (n.type === "svg") {
        if (!("xmlns" in n.props)) attrs += ' xmlns="http://www.w3.org/2000/svg"';
        if (!("fill" in n.props)) attrs += ' fill="#ffffff"';
    }
    return `<${n.type}${attrs}>${expand(n.props.children).map(markup).join("")}</${n.type}>`;
};
const findSvg = nodes => {
    for (const n of nodes) {
        if (typeof n === "string") continue;
        if (n.type === "svg") return markup(n);
        const inner = findSvg(expand(n.props.children));
        if (inner) return inner;
    }
    return null;
};
const svg = icon => {
    if (!icon) return null;
    const node = typeof icon === "function" || icon.__fcStub ? VencordCreateElement(icon, { width: 20, height: 20, className: "" }) : icon;
    return findSvg(expand(node));
};

const items = (scope, nodes) => nodes.flatMap(n => {
    if (typeof n === "string") return [];
    const p = n.props;
    switch (n.type) {
        case "fc-item": {
            const kids = items(scope, expand(p.children));
            return [{ kind: "item", id: p.id ?? null, label: text(p.label), action: act(scope, p.action), disabled: !!p.disabled, danger: p.color === "danger", check: p.checkbox || p.radio ? !!p.checked : null, icon: svg(p.icon), children: kids }];
        }
        case "fc-group": return [{ kind: "sep" }, ...items(scope, expand(p.children))];
        case "fc-sep": return [{ kind: "sep" }];
        default: return items(scope, expand(p.children));
    }
});

const event = (o = {}) => ({ preventDefault() {}, stopPropagation() {}, persist() {}, detail: o.detail ?? 1, shiftKey: !!o.shift, ctrlKey: !!o.ctrl, altKey: !!o.alt, metaKey: false, button: o.button ?? 0, currentTarget: document.body, target: document.body, clientX: 0, clientY: 0 });
const msg = raw => raw ? new Message(raw) : undefined;
const optionValue = (name, key) => {
    const v = Settings.plugins[name][key];
    return typeof v === "bigint" ? String(v) : v;
};

const api = {
    // every plugin entry path; returns what the plugins page shows
    list({ paths }) {
        return paths.map(load).map(e => {
            const p = e.p ?? {};
            const def = defs.get(e.name) ?? {};
            const evalBool = f => { try { return typeof f === "function" ? !!f() : !!f; } catch { return false; } };
            return {
                name: e.name, path: e.path, description: p.description ?? "", authors: (p.authors ?? []).map(a => a?.name).filter(Boolean),
                enabled: !!enabled(e), started: !!e.started, error: e.error ?? null, hidden: !!p.hidden || !!p.required, about: !!p.settingsAboutComponent,
                patches: (p.patches ?? []).length,
                parts: ["start", "flux", "commands", "contextMenus", "chatBarButton", "messagePopoverButton", "onBeforeMessageSend", "onBeforeMessageEdit", "onMessageClick"].filter(k => p[k] && (!Array.isArray(p[k]) || p[k].length) && (typeof p[k] !== "object" || Object.keys(p[k]).length)),
                options: Object.entries(def).map(([key, d]) => ({
                    key, type: d.type, description: d.description ?? "", hidden: evalBool(d.hidden), disabled: evalBool(d.disabled),
                    value: optionValue(e.name, key), choices: d.options?.map(o => ({ label: String(o.label), value: o.value })) ?? null,
                    markers: d.markers ?? null, min: d.markers?.[0] ?? null, max: d.markers?.at?.(-1) ?? null, stickToMarkers: !!d.stickToMarkers,
                })),
            };
        });
    },
    // loads the plugins given and starts those that are on
    start({ paths }) {
        for (const path of paths) {
            const e = load(path);
            if (enabled(e)) start(e);
        }
        return null;
    },
    setEnabled({ name, on }) {
        const e = plugins.get(name);
        if (!e?.p) return null;
        Settings.plugins[name].enabled = on;
        on ? start(e) : stop(e);
        return null;
    },
    setOption({ name, key, value }) {
        Settings.plugins[name][key] = value;
        return null;
    },
    dispatch({ t, d, x }) {
        for (const a of actions(t, d ?? {}, x)) FluxDispatcher.dispatch(a);
        for (const id of storeRoots()) roots.get(id)?.dirtyNow();
        return null;
    },
    // plugin UI: a root's tree, a control's event (answered with the tree after it), closing one
    root: ({ root }) => render(root),
    event({ root, id, args }) {
        fire(root, id, args ?? []);
        return roots.has(root) ? render(root) : null;
    },
    close({ root }) {
        root.startsWith("modal:") ? closeModal(root) : unmount(root);
        return null;
    },
    // what plugins add that Rustcord draws: so it only asks for what someone is adding
    features: () => ({ accessories: accessories.size, decorations: decorationsFactories.size, members: decoratorsFactories.size }),
    settingUi({ plugin, key }) {
        const d = defs.get(plugin)?.[key];
        if (!d?.component) return null;
        const id = `setting:${plugin}:${key}`;
        mount(id, VencordCreateElement(d.component, { id: key, option: d, setValue: v => { Settings.plugins[plugin][key] = v; }, setError() {}, onChange: v => { Settings.plugins[plugin][key] = v; }, pluginSettings: Settings.plugins[plugin] }));
        return render(id);
    },
    about({ plugin }) {
        const p = plugins.get(plugin)?.p;
        if (!p?.settingsAboutComponent) return null;
        mount(`about:${plugin}`, VencordCreateElement(p.settingsAboutComponent, {}));
        return render(`about:${plugin}`);
    },
    accessories({ message, channel }) {
        if (!accessories.size) return null;
        const props = { message: msg(message), channel: ChannelStore.getChannel(channel), compact: false };
        const all = [...accessories.values()].map(a => VencordCreateElement(a.render, { ...props, key: a.render.name }));
        const id = `acc:${message.id}`;
        mount(id, VencordCreateElement(VencordFragment, { children: all }));
        return render(id);
    },
    decorations({ message, channel, nick }) {
        if (!decorationsFactories.size) return null;
        const m = msg(message);
        const props = { message: m, channel: ChannelStore.getChannel(channel), compact: false, author: { nick: nick ?? m.author?.username, colorString: null }, decorations: [null, []] };
        const id = `dec:${message.id}`;
        mount(id, VencordCreateElement(VencordFragment, { children: [...decorationsFactories.values()].map(f => VencordCreateElement(f, props)) }));
        return render(id);
    },
    member({ user, guild, channel }) {
        if (!decoratorsFactories.size) return null;
        const type = guild ? "guild" : "dm";
        const props = { type, user: getUser(user), channel: ChannelStore.getChannel(channel), isOwner: !!guild && GuildStore.getGuild(guild)?.ownerId === user };
        const id = `mem:${type}:${user}`;
        const list = [...decoratorsFactories.values()].filter(d => !d.onlyIn || d.onlyIn === type).map(d => VencordCreateElement(d.render, props));
        mount(id, VencordCreateElement(VencordFragment, { children: list }));
        return render(id);
    },
    badges({ user, guild }) {
        reset("badge");
        return _getBadges({ userId: user, guildId: guild ?? null }).map(b => {
            let tree = null;
            if (b.component) {
                const id = `badge:${user}:${b.key ?? b.id ?? b.description}`;
                mount(id, VencordCreateElement(b.component, b));
                tree = { root: id, tree: render(id) };
            }
            return { tooltip: b.description ?? b.key ?? "", icon: b.iconSrc ?? null, link: b.link ?? null, click: act("badge", b.onClick && (ev => b.onClick(ev, b))), component: tree };
        });
    },
    preSend({ token, channel, content, reply, files }) {
        const m = { content, validNonShortcutEmojis: [], invalidEmojis: [], tts: false };
        const options = { messageReference: reply ? { message_id: reply, channel_id: channel } : undefined, location: "normal", stickerIds: [] };
        const props = { content, channel: ChannelStore.getChannel(channel), hasStickers: false, hasAttachments: !!files, openWarningPopout() {} };
        events._handlePreSend(channel, m, options, props).then(cancel => __out({ op: "sent", token, content: cancel ? null : m.content }));
        return null;
    },
    preEdit({ token, channel, message, content }) {
        const m = { content };
        events._handlePreEdit(channel, message, m).then(cancel => __out({ op: "edited", token, content: cancel ? null : m.content }));
        return null;
    },
    hasSendListeners: () => events.sendListeners.size > 0,
    hasEditListeners: () => events.editListeners.size > 0,
    commands() {
        return Object.values(commands).map(c => ({
            name: c.name, description: c.description ?? "", plugin: c.plugin,
            options: (c.options ?? []).map(o => ({ name: o.name, description: o.description ?? "", type: o.type, required: !!o.required, choices: o.choices ?? null })),
        }));
    },
    runCommand({ name, args, channel, guild }) {
        const c = commands[name];
        if (!c) return null;
        const ctx = { channel: ChannelStore.getChannel(channel), guild: GuildStore.getGuild(guild) };
        Promise.resolve()
            .then(() => c.execute(args, ctx))
            .then(r => { if (r?.content) __out({ op: "send", channel, content: String(r.content) }); })
            .catch(e => { console.error(`/${name} threw:`, e); __out({ op: "toast", text: `/${name} failed: ${e?.message ?? e}` }); });
        return null;
    },
    menu({ nav, message, channel, user, guild }) {
        reset("menu");
        const props = {
            message: msg(message), channel: ChannelStore.getChannel(channel), user: user ? getUser(user) : undefined,
            guild: GuildStore.getGuild(guild), guildId: guild ?? undefined, onClose() {},
        };
        return items("menu", expand(build(nav, props)));
    },
    popover({ message }) {
        reset("popover");
        const m = msg(message);
        const out = [];
        for (const [key, { render }] of MessagePopoverButtonMap) {
            try {
                const it = render(m);
                if (it) out.push({ key, label: String(it.label ?? key), icon: svg(it.icon), click: act("popover", it.onClick), menu: act("popover", it.onContextMenu) });
            } catch (e) { console.error(`${key} popover button threw:`, e); }
        }
        return out;
    },
    chatBar({ channel, empty }) {
        reset("chatbar");
        const props = { channel: ChannelStore.getChannel(channel), disabled: false, isEmpty: !!empty, isMainChat: true, isAnyChat: true, type: { analyticsName: "normal", attachments: true, commands: { enabled: true }, emojis: { button: true }, gifs: { button: true, allowSending: true }, stickers: { button: true, allowSending: true }, submit: { button: false }, users: { allowMentioning: true } } };
        const out = [];
        for (const [key, { render }] of ChatBarButtonMap) {
            try {
                for (const b of expand(render(props))) {
                    const find = n => typeof n === "string" ? [] : n.type === "fc-chatbar" ? [n] : expand(n.props.children).flatMap(find);
                    for (const n of find(b)) out.push({ key, tooltip: String(n.props.tooltip ?? key), icon: findSvg(expand(n.props.children)), click: act("chatbar", n.props.onClick), menu: act("chatbar", n.props.onContextMenu) });
                }
            } catch (e) { console.error(`${key} chat bar button threw:`, e); }
        }
        return out;
    },
    act({ id, shift, ctrl, alt, button }) {
        const fn = scopes[id.split(":")[0]]?.fns.get(id);
        try { fn?.(event({ shift, ctrl, alt, button })); } catch (e) { console.error("a plugin action threw:", e); }
        return null;
    },
    messageClick({ message, channel, shift, ctrl, alt, detail }) {
        events._handleClick(msg(message), ChannelStore.getChannel(channel), event({ shift, ctrl, alt, detail }));
        return null;
    },
    settle({ id, ok, value }) {
        __settle(id, ok, value);
        return null;
    },
    // Vencord's own settings pages: the whole tree, and a value set by its path
    settings: () => PlainSettings,
    setPath({ path, value }) {
        let o = PlainSettings;
        for (const k of path.slice(0, -1)) o = o[k] ??= {};
        o[path.at(-1)] = value;
        save();
        return null;
    },
    // a whole settings file, as Backup & Restore and cloud sync bring one in
    importSettings({ settings }) {
        for (const k of Object.keys(PlainSettings)) delete PlainSettings[k];
        Object.assign(PlainSettings, settings);
        save();
        return null;
    },
    // the DataStore, where Vencord keeps its cloud secret
    async dataGet({ key }) {
        __out({ op: "data_value", key, value: (await DataStore.get(key)) ?? null });
        return null;
    },
    dataSet({ key, value }) {
        value == null ? DataStore.del(key) : DataStore.set(key, value);
        return null;
    },
    // the notification log, and a notification's click or close
    async log() {
        __out({ op: "log", value: await getLog() });
        return null;
    },
    logDelete({ id }) {
        (id ? deleteNotification(id) : clearLog()).then(() => api.log());
        return null;
    },
    notifyAct({ id, kind }) {
        const h = notifyHandlers.get(id);
        try { (kind === "click" ? h?.onClick : h?.onClose)?.(); } catch (e) { console.error("a notification handler threw:", e); }
        if (kind === "close") notifyHandlers.delete(id);
        return null;
    },
    // fires due timers; how long until the next one, or -1
    tick: () => __timers(),
    me: () => UserStore.getCurrentUser()?.id ?? null,
};

globalThis.__host = Object.fromEntries(Object.entries(api).map(([k, f]) => [k, json => JSON.stringify(f(JSON.parse(json)) ?? null, (_, v) => typeof v === "bigint" ? String(v) : v)]));
