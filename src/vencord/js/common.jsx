// @webpack/common: the Discord modules plugins reach for most, backed by Rustcord. Stores read
// its store as Discord-shaped records; anything not here is a stub.
import React, { mount, roots, unmount, useSyncExternalStore } from "react";

const q = (kind, a = "", b = "") => JSON.parse(__fc.q(kind, String(a ?? ""), String(b ?? "")));
const out = op => __out(op);

export { React };
export const { useState, useEffect, useLayoutEffect, useMemo, useCallback, useRef, useReducer, useContext, createContext, memo, forwardRef, Fragment, createElement, cloneElement, isValidElement, Children } = React;
export const ReactDOM = { createPortal: c => c, render() {}, unmountComponentAtNode() {}, findDOMNode: () => null };
export const createRoot = () => ({ render() {}, unmount() {} });

const CDN = "https://cdn.discordapp.com";
export class User {
    constructor(r) { Object.assign(this, r); this.globalName ??= null; this.avatarDecorationData ??= null; this.primaryGuild ??= null; }
    get tag() { return this.username; }
    get createdAt() { return new Date(Number(BigInt(this.id) >> 22n) + 1420070400000); }
    getAvatarURL(_guild, size = 128, animated = false) {
        if (!this.avatar) return `${CDN}/embed/avatars/${Number(BigInt(this.id) >> 22n) % 6}.png`;
        const ext = animated && this.avatar.startsWith("a_") ? "gif" : "webp";
        return `${CDN}/avatars/${this.id}/${this.avatar}.${ext}?size=${size}`;
    }
    hasFlag() { return false; }
    isStaff() { return false; }
    isClyde() { return false; }
    isSystemUser() { return false; }
    toString() { return this.username; }
}
export class Channel {
    constructor(r) { Object.assign(this, r); this.ownerId = r.owner_id; this.parentId = r.parent_id; this.rawRecipients = (r.recipients ?? []).map(id => getUser(id)).filter(Boolean); }
    getGuildId() { return this.guild_id; }
    isDM() { return this.type === 1; }
    isGroupDM() { return this.type === 3; }
    isMultiUserDM() { return this.type === 3; }
    isPrivate() { return this.type === 1 || this.type === 3; }
    isThread() { return [10, 11, 12].includes(this.type); }
    isForumChannel() { return this.type === 15; }
    isVocal() { return this.type === 2 || this.type === 13; }
    isGuildVoice() { return this.type === 2; }
    isGuildStageVoice() { return this.type === 13; }
    isCategory() { return this.type === 4; }
    isNSFW() { return !!this.nsfw; }
    getRecipientId() { return this.recipients?.[0]; }
    toString() { return this.name; }
}
export class Guild {
    constructor(r) { Object.assign(this, r); this.features = new Set(r.features ?? []); }
    getIconURL(size = 128) { return this.icon ? `${CDN}/icons/${this.id}/${this.icon}.webp?size=${size}` : null; }
    getRole(id) { return GuildRoleStore.getRole(this.id, id); }
    isOwner(u) { return (u?.id ?? u) === this.ownerId; }
    hasFeature(f) { return this.features.has(f); }
    toString() { return this.name; }
}
export class Message {
    constructor(r) {
        Object.assign(this, r);
        this.author = r.author ? new User(r.author) : null;
        this.mentionRoles = r.mention_roles ?? [];
        this.mentionEveryone = !!r.mention_everyone;
        this.messageReference = r.message_reference ?? null;
        this.editedTimestamp = r.edited ? new Date() : null;
        this.timestamp = new Date(r.timestamp ?? Date.now());
        this.state = "SENT";
        this.blocked = RelationshipStore.isBlocked(this.author?.id);
        this.reactions ??= [];
        this.stickerItems ??= [];
        this.components ??= [];
    }
    getChannelId() { return this.channel_id; }
    hasFlag(f) { return (this.flags & f) === f; }
    isSystemDM() { return false; }
    toJS() { return { ...this }; }
}

const record = (C, v) => v == null ? undefined : new C(v);
export const getUser = id => record(User, q("user", id));

const store = (name, methods) => ({
    getName: () => name,
    addChangeListener() {}, removeChangeListener() {}, addReactChangeListener() {}, removeReactChangeListener() {},
    emitChange() {},
    ...methods,
});

export const UserStore = store("UserStore", {
    getCurrentUser: () => record(User, q("me")),
    getUser,
    getUsers: () => ({}),
});
export const ChannelStore = store("ChannelStore", {
    getChannel: id => record(Channel, q("channel", id)),
    hasChannel: id => q("channel", id) != null,
    getDMFromUserId: u => q("private").map(id => q("channel", id)).find(c => c?.type === 1 && c.recipients?.includes(String(u)))?.id,
    getDMChannelFromUserId: u => record(Channel, q("private").map(id => q("channel", id)).find(c => c?.type === 1 && c.recipients?.includes(String(u)))),
    getSortedPrivateChannels: () => q("private").map(id => record(Channel, q("channel", id))).filter(Boolean),
    getPrivateChannelIds: () => q("private"),
    getMutablePrivateChannels: () => Object.fromEntries(q("private").map(id => [id, record(Channel, q("channel", id))])),
});
export const GuildStore = store("GuildStore", {
    getGuild: id => record(Guild, q("guild", id)),
    getGuilds: () => Object.fromEntries(q("guilds").map(id => [id, record(Guild, q("guild", id))])),
    getGuildIds: () => q("guilds"),
    getGuildCount: () => q("guilds").length,
    getRoles: g => q("roles", g),
    getRole: (g, r) => q("roles", g)[r],
});
export const GuildRoleStore = store("GuildRoleStore", {
    getRole: (g, r) => q("roles", g)[r],
    getRolesSnapshot: g => q("roles", g),
    getSortedRoles: g => Object.values(q("roles", g)).sort((a, b) => b.position - a.position),
    getUnsafeMutableRoles: g => q("roles", g),
});
export const GuildMemberStore = store("GuildMemberStore", {
    getMember: (g, u) => q("member", g, u) ?? undefined,
    getNick: (g, u) => q("member", g, u)?.nick ?? null,
    isMember: (g, u) => q("member", g, u) != null,
    getMemberIds: () => [],
    getMembers: () => [],
});
export const GuildChannelStore = store("GuildChannelStore", {
    getChannels: g => {
        const all = q("channels", g).map(id => record(Channel, q("channel", id))).filter(Boolean);
        return { SELECTABLE: all.filter(c => !c.isVocal() && !c.isCategory()).map(channel => ({ channel })), VOCAL: all.filter(c => c.isVocal()).map(channel => ({ channel })) };
    },
});
export const SelectedChannelStore = store("SelectedChannelStore", {
    getChannelId: () => q("selected").channel ?? undefined,
    getVoiceChannelId: () => q("selected").voice ?? undefined,
    getCurrentlySelectedChannelId: () => q("selected").channel ?? undefined,
    getLastSelectedChannelId: () => q("selected").channel ?? undefined,
});
export const SelectedGuildStore = store("SelectedGuildStore", {
    getGuildId: () => q("selected").guild ?? null,
    getLastSelectedGuildId: () => q("selected").guild ?? null,
});
export const MessageStore = store("MessageStore", {
    getMessage: (c, m) => record(Message, q("message", c, m)),
    getMessages: c => {
        const list = q("messages", c).map(m => new Message(m));
        return { _array: list, toArray: () => list, forEach: f => list.forEach(f), filter: f => list.filter(f), get: id => list.find(m => m.id === id), last: () => list.at(-1), length: list.length, hasMoreBefore: true };
    },
});
export const RelationshipStore = store("RelationshipStore", {
    getRelationships: () => q("relationships"),
    getRelationshipType: u => q("relationships")[u] ?? 0,
    isFriend: u => q("relationships")[u] === 1,
    isBlocked: u => q("relationships")[u] === 2,
    isIgnored: () => false,
    getFriendIDs: () => Object.entries(q("relationships")).filter(([, k]) => k === 1).map(([u]) => u),
    getPendingCount: () => Object.values(q("relationships")).filter(k => k === 3).length,
    getNickname: () => null,
    getSince: () => null,
});
export const PresenceStore = store("PresenceStore", {
    getStatus: u => q("presence", u).status,
    getActivities: u => q("presence", u).activities,
    getPrimaryActivity: u => q("presence", u).activities[0],
    getState: () => ({ clientStatuses: {} }),
    isMobileOnline: () => false,
});
export const VoiceStateStore = store("VoiceStateStore", {
    getVoiceStatesForChannel: c => Object.fromEntries(q("voice", c).map(s => [s.userId, s])),
    getVoiceStateForUser: u => q("voiceOf", u) ?? undefined,
    getVoiceStateForChannel: c => q("voice", c).find(s => s.userId === UserStore.getCurrentUser()?.id),
    getUserVoiceChannelId: (_, u) => q("voiceOf", u)?.channelId,
});
export const ReadStateStore = store("ReadStateStore", {
    hasUnread: c => q("unread", c).unread,
    getMentionCount: c => q("unread", c).mentions,
    lastMessageId: c => q("channel", c)?.last_message_id,
    getOldestUnreadMessageId: c => q("unread", c).lastAcked,
});
export const AuthenticationStore = store("AuthenticationStore", { getId: () => q("me")?.id, getSessionId: () => "rustcord", getToken: () => null });
export const PermissionStore = store("PermissionStore", { can: () => true, canBasicChannel: () => true, getChannelPermissions: () => -1n, getGuildPermissions: () => -1n });
export const TypingStore = store("TypingStore", { getTypingUsers: () => ({}), isTyping: () => false });
export const EmojiStore = store("EmojiStore", { getCustomEmojiById: () => undefined, getUsableCustomEmojiById: () => undefined, getGuilds: () => ({}), getGuildEmoji: () => [] });
export const StickersStore = store("StickersStore", { getStickerById: () => undefined, getAllGuildStickers: () => new Map(), getStickersByGuildId: () => [] });
export const UserProfileStore = store("UserProfileStore", { getUserProfile: () => undefined, getGuildMemberProfile: () => undefined, getMutualFriends: () => [], getMutualGuilds: () => [] });
export const PendingReplyStore = store("PendingReplyStore", { getPendingReply: () => undefined });
export const DraftStore = store("DraftStore", { getDraft: () => "" });
export const LocaleStore = store("LocaleStore", { locale: "en-US", systemLocale: "en-US" });
export const ThemeStore = store("ThemeStore", { theme: "dark" });
export const StreamerModeStore = store("StreamerModeStore", { enabled: false });
export const WindowStore = store("WindowStore", { isFocused: () => true, isVisible: () => true });
export const UserSettingsProtoStore = store("UserSettingsProtoStore", { settings: {} });
export const UserGuildSettingsStore = store("UserGuildSettingsStore", { isMuted: () => false, isChannelMuted: () => false, getMessageNotifications: () => 0 });
export const MediaEngineStore = store("MediaEngineStore", { isSelfMute: () => false, isSelfDeaf: () => false, getOutputVolume: () => 100 });

// Flux: gateway events reach subscribers as Discord's actions; plugins can dispatch their own
const subs = new Map();
export const FluxDispatcher = {
    subscribe(type, fn) { let s = subs.get(type); if (!s) subs.set(type, s = new Set()); s.add(fn); },
    unsubscribe(type, fn) { subs.get(type)?.delete(fn); },
    dispatch(action) {
        for (const fn of [...(subs.get(action.type) ?? [])]) {
            try { fn(action); } catch (e) { console.error(`flux ${action.type} handler threw:`, e); }
        }
        return Promise.resolve();
    },
    isDispatching: () => false,
    addInterceptor() {},
    wait: f => f(),
    _subscriptions: subs,
};
export const Flux = { Store: class { constructor() {} getName() { return this.constructor.name; } emitChange() {} addChangeListener() {} removeChangeListener() {} initialize() {} }, connectStores: (_, f) => c => c };
export const useStateFromStores = (_, get) => useSyncExternalStore(null, get);
export const useStateFromStoresArray = useStateFromStores;
export const useStateFromStoresObject = useStateFromStores;

// REST: Rustcord's own client sends it, so it goes with the account's token
const rest = method => async opts => {
    const o = typeof opts === "string" ? { url: opts } : opts;
    let url = o.url;
    if (o.query) url += (url.includes("?") ? "&" : "?") + new URLSearchParams(Object.entries(o.query).filter(([, v]) => v != null)).toString();
    const r = await __ask({ op: "rest", method, path: url, body: o.body ?? null });
    if (r.status >= 400) throw { ok: false, status: r.status, body: r.body };
    return { ok: true, status: r.status, body: r.body, text: JSON.stringify(r.body) };
};
export const RestAPI = { get: rest("GET"), post: rest("POST"), put: rest("PUT"), patch: rest("PATCH"), del: rest("DELETE"), delete: rest("DELETE") };
export const Constants = {
    Endpoints: new Proxy({
        USER: u => `/users/${u}`, USER_PROFILE: u => `/users/${u}/profile`, MESSAGES: c => `/channels/${c}/messages`, MESSAGE: (c, m) => `/channels/${c}/messages/${m}`,
        GUILD: g => `/guilds/${g}`, GUILD_MEMBER: (g, u) => `/guilds/${g}/members/${u}`, CHANNEL: c => `/channels/${c}`, INVITE: c => `/invites/${c}`,
        USER_RELATIONSHIPS: u => `/users/@me/relationships${u ? "/" + u : ""}`, AUTH_SESSIONS: "/auth/sessions", FRIEND_INVITES: "/users/@me/invites",
        APPLICATION_ASSETS: a => `/oauth2/applications/${a}/assets`, USER_CHANNELS: "/users/@me/channels", ME: "/users/@me",
    }, { get: (t, k) => t[k] ?? (typeof k === "string" ? (...a) => `/${k.toLowerCase()}/${a.join("/")}` : undefined) }),
    UserFlags: {},
};
export const PermissionsBits = new Proxy({}, { get: (_, k) => ({ ADMINISTRATOR: 8n, MANAGE_MESSAGES: 8192n, SEND_MESSAGES: 2048n, VIEW_CHANNEL: 1024n, MANAGE_GUILD: 32n, MANAGE_ROLES: 268435456n, MANAGE_CHANNELS: 16n, CREATE_INSTANT_INVITE: 1n, CONNECT: 1048576n, EMBED_LINKS: 16384n, ATTACH_FILES: 32768n, MANAGE_GUILD_EXPRESSIONS: 1073741824n, CREATE_GUILD_EXPRESSIONS: 8796093022208n })[k] ?? 0n });

export const showToast = (message, type, opts) => out({ op: "toast", text: String(message?.props?.children ?? message) });
export const Toasts = {
    Type: { MESSAGE: "message", SUCCESS: "success", FAILURE: "failure", CUSTOM: "custom", CLIP: "clip", LINK: "link", FORWARD: "forward", INVITE: "invite", BOOKMARK: "bookmark", CLOCK: "clock" },
    Position: { TOP: 0, BOTTOM: 1 },
    genId: () => String(Math.random()),
    create: (message, type, options) => ({ message, type, options }),
    show: t => showToast(t.message, t.type),
    pop() {},
};

export const MessageActions = {
    sendMessage: (channelId, msg) => (out({ op: "send", channel: String(channelId), content: String(msg?.content ?? "") }), Promise.resolve()),
    receiveMessage: (channelId, msg) => out({ op: "bot", channel: String(channelId), content: String(msg?.content ?? ""), embeds: msg?.embeds ?? [] }),
    editMessage: (c, m, msg) => RestAPI.patch({ url: `/channels/${c}/messages/${m}`, body: { content: msg.content } }),
    deleteMessage: (c, m) => RestAPI.del({ url: `/channels/${c}/messages/${m}` }),
    fetchMessages() {}, jumpToMessage: ({ channelId, messageId }) => out({ op: "jump", channel: String(channelId), message: String(messageId) }),
    _sendMessage() {},
};
export const ChannelActionCreators = { openPrivateChannel: u => out({ op: "dm", user: String(u) }), selectPrivateChannel: c => out({ op: "open", channel: String(c) }) };
export const ChannelRouter = { transitionToChannel: c => out({ op: "open", channel: String(c) }), transitionToThread: t => out({ op: "open", channel: String(t?.id ?? t) }) };
export const NavigationRouter = {
    transitionTo: path => { const m = /\/channels\/[^/]+\/(\d+)(?:\/(\d+))?/.exec(path); m ? out(m[2] ? { op: "jump", channel: m[1], message: m[2] } : { op: "open", channel: m[1] }) : out({ op: "url", url: `https://discord.com${path}` }); },
    transitionToGuild: g => out({ op: "guild", guild: String(g) }),
    back() {}, forward() {},
};
export const SettingsRouter = { open: () => out({ op: "openSettings" }), openUserSettings: () => out({ op: "openSettings" }) };
export const UserUtils = { getUser: async id => getUser(id) ?? record(User, (await RestAPI.get({ url: `/users/${id}` })).body) };
export const IconUtils = {
    getUserAvatarURL: (u, animated, size = 128) => new User(u).getAvatarURL(null, size, animated),
    getDefaultAvatarURL: id => `${CDN}/embed/avatars/${Number(BigInt(id) >> 22n) % 6}.png`,
    getGuildIconURL: ({ id, icon, size = 128 }) => icon ? `${CDN}/icons/${id}/${icon}.webp?size=${size}` : null,
    getGuildBannerURL: ({ id, banner }) => banner ? `${CDN}/banners/${id}/${banner}.webp?size=1024` : null,
    getUserBannerURL: ({ id, banner, size = 1024 }) => banner ? `${CDN}/banners/${id}/${banner}.webp?size=${size}` : null,
    getEmojiURL: ({ id, animated, size = 48 }) => `${CDN}/emojis/${id}.${animated ? "gif" : "webp"}?size=${size}`,
    getApplicationIconURL: ({ id, icon }) => `${CDN}/app-icons/${id}/${icon}.webp`,
};
export const SnowflakeUtils = {
    extractTimestamp: id => Number(BigInt(id) >> 22n) + 1420070400000,
    fromTimestamp: t => String(BigInt(t - 1420070400000) << 22n),
    age: id => Date.now() - SnowflakeUtils.extractTimestamp(id),
};
export const ComponentDispatch = {
    dispatchToLastSubscribed: (event, data) => { if (event === "INSERT_TEXT") out({ op: "insert", text: String(data?.plainText ?? data?.rawText ?? "") }); },
    dispatch() {}, subscribe() {}, unsubscribe() {},
};
export const DraftType = { ChannelMessage: 0, ThreadSettings: 1, FirstThreadMessage: 2, ApplicationLauncherCommand: 3 };
export const UploadHandler = { promptToUpload: () => {} };
export const ApplicationAssetUtils = { fetchAssetIds: async (app, keys) => keys };
export const i18n = { Messages: new Proxy({}, { get: (_, k) => String(k) }), getLocale: () => "en-US", intl: { string: k => String(k), formatToPlainString: k => String(k) } };
export const Parser = { parse: s => s, parseTopic: s => s, parseToAST: s => [s] };
export const lodash = { debounce: f => f, throttle: f => f, isEqual: (a, b) => JSON.stringify(a) === JSON.stringify(b), cloneDeep: structuredClone, uniq: a => [...new Set(a)], chunk: (a, n) => a.reduce((o, x, i) => (i % n ? o[o.length - 1].push(x) : o.push([x]), o), []), sortBy: (a, f) => [...a].sort((x, y) => (f(x) > f(y)) - (f(x) < f(y))), memoize: f => f };
export const moment = Object.assign(t => { const d = new Date(t ?? Date.now()); return { toDate: () => d, valueOf: () => +d, format: () => d.toLocaleString(), unix: () => Math.floor(+d / 1000), fromNow: () => d.toLocaleString() }; }, { duration: ms => ({ asSeconds: () => ms / 1000, humanize: () => `${Math.round(ms / 60000)} minutes` }) });
export const zustandCreate = f => { let s; const set = p => { s = { ...s, ...(typeof p === "function" ? p(s) : p) }; }; s = f(set, () => s); const hook = sel => sel ? sel(s) : s; hook.getState = () => s; hook.setState = set; hook.subscribe = () => () => {}; return hook; };
export const zustandPersist = f => f;
export const MessageTypeSets = new Proxy({}, { get: () => new Set() });

// UI, as elements Rustcord draws natively: menus become its menus, Discord's controls its own
// fc-* controls, everything else plain html
const el = VencordCreateElement;
const tag = name => props => el(name, props);
export const Menu = {
    Menu: p => p.children,
    MenuItem: tag("fc-item"),
    MenuCheckboxItem: p => el("fc-item", { ...p, checkbox: true }),
    MenuRadioItem: p => el("fc-item", { ...p, radio: true }),
    MenuGroup: tag("fc-group"),
    MenuSeparator: tag("fc-sep"),
    MenuControlItem: tag("fc-control"),
    MenuSliderControl: tag("fc-control"),
    MenuSearchControl: tag("fc-control"),
};
export const ContextMenuApi = { openContextMenu() {}, closeContextMenu() {}, openContextMenuLazy() {} };

const tipProps = { onMouseEnter() {}, onMouseLeave() {}, onClick() {}, onContextMenu() {}, onFocus() {}, onBlur() {} };
export const Tooltip = p => el("fc-tip", { tooltip: p.text, children: typeof p.children === "function" ? p.children(tipProps, { isShown: false }) : p.children });
export const TooltipContainer = p => el("fc-tip", { tooltip: p.text, children: p.children });

// Discord's "text-md/semibold" variants as css
const SIZES = { xxs: "10px", xs: "12px", sm: "14px", md: "16px", lg: "20px", xl: "24px", xxl: "32px" };
const WEIGHTS = { normal: 400, medium: 500, semibold: 600, bold: 700, extrabold: 800 };
const COLORS = { "text-muted": "var(--text-muted)", "header-primary": "var(--header-primary)", "header-secondary": "var(--header-secondary)", "text-danger": "var(--text-danger)", "text-positive": "var(--text-positive)", "text-link": "var(--text-link)", "interactive-normal": "var(--interactive-normal)" };
const variant = (v = "text-md/normal", color, style) => {
    const [kind, weight] = v.split("/");
    const size = kind.split("-").pop();
    return { fontSize: SIZES[size] ?? "16px", fontWeight: WEIGHTS[weight] ?? 400, ...(color ? { color: COLORS[color] ?? color } : {}), ...style };
};
export const Text = ({ variant: v, color, style, tag: t = "div", children, ...rest }) => el(t, { ...rest, style: variant(v, color, style), children });
export const Heading = ({ variant: v = "heading-md/semibold", color, style, tag: t = "h2", children, ...rest }) => el(t, { ...rest, style: variant(v, color ?? "header-primary", style), children });

const LOOK = { RED: "dangerPrimary", GREEN: "positive", BRAND: "primary", PRIMARY: "secondary", TRANSPARENT: "secondary", WHITE: "secondary", LINK: "link", BLURPLE: "primary" };
export const Button = Object.assign(
    ({ color = "BRAND", look, size, onClick, disabled, children, style }) => el("fc-button", { variant: look === "LINK" ? "link" : LOOK[color] ?? "primary", size, onClick, disabled, style, children }),
    { Colors: Object.fromEntries(Object.keys(LOOK).map(k => [k, k])), Looks: { FILLED: "FILLED", OUTLINED: "OUTLINED", LINK: "LINK", BLANK: "BLANK" }, Sizes: { NONE: "none", TINY: "xs", SMALL: "small", MEDIUM: "medium", LARGE: "large", MIN: "min", MAX: "max", ICON: "iconOnly" } },
);
export const Clickable = ({ onClick, onContextMenu, children, style }) => el("fc-clickable", { onClick, onContextMenu, style, children });
export const Forms = {
    FormTitle: ({ tag: t = "h5", children, style }) => el(t, { style: { fontSize: "12px", fontWeight: 600, color: "var(--header-secondary)", marginBottom: "8px", ...style }, children }),
    FormText: ({ children, style, type }) => el("p", { style: { fontSize: "14px", color: "var(--header-secondary)", ...style }, children }),
    FormSection: ({ title, children, style }) => el("section", { style, children: [title ? Forms.FormTitle({ children: title }) : null, children] }),
    FormDivider: ({ style }) => el("hr", { style }),
    FormItem: ({ title, children, style }) => el("div", { style, children: [title ? Forms.FormTitle({ children: title }) : null, children] }),
    FormLabel: ({ children }) => el("label", { style: { fontWeight: 600 }, children }),
    FormSwitch: p => Switch(p),
};
export const Card = ({ children, style, onClick }) => el("div", { onClick, style: { backgroundColor: "var(--background-secondary)", padding: "12px", borderRadius: "8px", ...style }, children });
export const Switch = ({ value, checked, onChange, disabled, note, children, hideBorder }) => el("fc-switch", { checked: !!(value ?? checked), onChange, disabled, label: children, note, hideBorder });
export const TextInput = ({ value, defaultValue, onChange, placeholder, type, disabled, maxLength, error, name }) => el("fc-input", { value: value ?? null, defaultValue, placeholder, type, disabled, maxLength, error, onChange: v => onChange?.(v, name) });
export const TextArea = ({ value, defaultValue, onChange, placeholder, rows, disabled }) => el("fc-textarea", { value: value ?? null, defaultValue, placeholder, rows, disabled, onChange: v => onChange?.(v) });
export const Select = ({ options = [], isSelected, select, placeholder, isDisabled }) => el("fc-select", { options: options.map(o => ({ ...o, default: !!isSelected?.(o.value) })), placeholder, disabled: isDisabled, onChange: v => select?.(v) });
export const SearchableSelect = ({ options = [], value, onChange, placeholder, multi }) => {
    const on = v => Array.isArray(value) ? value.some(x => (x?.value ?? x) === v) : (value?.value ?? value) === v;
    return el("fc-select", { options: options.map(o => ({ ...o, default: on(o.value) })), placeholder, onChange: v => onChange?.(multi ? [...(Array.isArray(value) ? value : []), v] : v) });
};
export const Slider = ({ initialValue, minValue = 0, maxValue = 100, markers, onValueChange, stickToMarkers, disabled }) => el("fc-slider", { value: initialValue, min: minValue, max: maxValue, markers, stickToMarkers, disabled, onChange: onValueChange });
export const Checkbox = ({ value, onChange, children, disabled }) => el("fc-checkbox", { checked: !!value, disabled, onChange: v => onChange?.({ target: { checked: v } }, v), label: children });
export const ColorPicker = ({ color, onChange }) => el("fc-color", { color: color ?? 0, onChange });
export const Avatar = ({ src, size }) => el("img", { src, width: parseInt(String(size).replace(/\D/g, "")) || 32, height: parseInt(String(size).replace(/\D/g, "")) || 32, style: { borderRadius: "50%" } });
export const Timestamp = p => el("span", { style: { color: "var(--text-muted)", fontSize: "12px" }, children: new Date(p.timestamp).toLocaleString() });
export const MaskedLink = ({ href, children }) => el("a", { href, children });
export const ScrollerThin = ({ children, style }) => el("div", { style, children });
export const Popout = p => typeof p.children === "function" ? p.children(tipProps, { isShown: false }) : null;
export const Dialog = ({ children, style }) => el("div", { style, children });
export const TabBar = Object.assign(
    ({ selectedItem, onItemSelect, children }) => el("fc-tabs", { selected: selectedItem, onChange: onItemSelect, children }),
    { Item: ({ id, children }) => el("fc-tab", { id, children }) },
);
export const FocusLock = p => p.children;
export const UserSummaryItem = ({ users = [], max = 10 }) => el("div", { style: { display: "flex", gap: "2px" }, children: users.slice(0, max).map(u => el("img", { src: u?.getAvatarURL?.(null, 32) ?? "", width: 20, height: 20, style: { borderRadius: "50%" } })) });

// modals: each a root Rustcord draws as a window over the app
let nextModal = 1;
export const ModalRoot = ({ children, size }) => el("fc-modal", { size, children });
export const ModalHeader = ({ children, separator }) => el("fc-modal-header", { children });
export const ModalContent = ({ children, style }) => el("fc-modal-content", { style, children });
export const ModalFooter = ({ children }) => el("fc-modal-footer", { children });
export const ModalCloseButton = ({ onClick }) => el("fc-modal-close", { onClick });
export const ModalSize = { SMALL: "small", MEDIUM: "medium", LARGE: "large", DYNAMIC: "dynamic" };
export function openModal(render, opts = {}) {
    const id = `modal:${nextModal++}`;
    const props = { transitionState: 1, onClose: () => closeModal(id) };
    const Host = () => render(props);
    mount(id, el(Host, {}));
    out({ op: "modal", root: id });
    opts.onCloseCallback && (closers[id] = opts.onCloseCallback);
    return id;
}
const closers = {};
export async function openModalLazy(factory, opts) {
    const render = await factory();
    return openModal(render, opts);
}
export function closeModal(id) {
    unmount(id);
    out({ op: "modalClose", root: id });
    closers[id]?.();
    delete closers[id];
}
export const closeAllModals = () => { for (const id of [...roots.keys()]) if (id.startsWith("modal:")) closeModal(id); };
export const Modal = { Root: ModalRoot, Header: ModalHeader, Content: ModalContent, Footer: ModalFooter, CloseButton: ModalCloseButton };
export const ConfirmModal = ({ header, children, confirmText = "Confirm", cancelText = "Cancel", onConfirm, onCancel, onClose, transitionState }) => el("fc-modal", {
    size: "small",
    children: [
        el("fc-modal-header", { children: el("h2", { children: header }) }),
        el("fc-modal-content", { children }),
        el("fc-modal-footer", { children: [el("fc-button", { variant: "dangerPrimary", onClick: () => { onConfirm?.(); onClose?.(); }, children: confirmText }), el("fc-button", { variant: "secondary", onClick: () => { onCancel?.(); onClose?.(); }, children: cancelText })] }),
    ],
});
export const openMediaModal = ({ items }) => out({ op: "image", url: String(items?.[0]?.url ?? "") });
