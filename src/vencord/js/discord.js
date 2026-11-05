// @utils/discord: Vencord's helpers for acting on Discord, done by Rustcord.
import { ChannelStore, GuildStore, RestAPI, SelectedChannelStore, SelectedGuildStore, showToast } from "@webpack/common";

const out = op => __out(op);

export const getIntlMessage = key => String(key);
export const getIntlMessageFromHash = (hash, values, key) => String(key ?? hash);
export const getCurrentChannel = () => ChannelStore.getChannel(SelectedChannelStore.getChannelId());
export const getCurrentGuild = () => GuildStore.getGuild(SelectedGuildStore.getGuildId());
export const openPrivateChannel = id => out({ op: "dm", user: String(id) });
export const Theme = { Dark: 1, Light: 2 };
export const getTheme = () => Theme.Dark;
export const insertTextIntoChatInputBox = text => out({ op: "insert", text: String(text) });

export async function copyWithToast(text, toast = "Copied to clipboard!") {
    out({ op: "copy", text: String(text) });
    showToast(toast);
}

export function sendMessage(channelId, data, waitForChannelReady, options = {}) {
    out({ op: "send", channel: String(channelId), content: String(data?.content ?? ""), reply: options.messageReference?.message_id ?? null });
    return Promise.resolve();
}

export function openImageModal(item) {
    out({ op: "image", url: String(item?.url ?? item?.original ?? "") });
}

export async function openInviteModal(code) {
    out({ op: "url", url: `https://discord.gg/${code}` });
}

export async function openUserProfile(id) {
    out({ op: "profile", user: String(id) });
}

export async function fetchUserProfile(id) {
    return (await RestAPI.get({ url: `/users/${id}/profile` })).body;
}

export const getUniqueUsername = user => user.discriminator === "0" ? user.username : `${user.username}#${user.discriminator}`;
export const getGuildAcronym = guild => guild.name.replace(/'s /g, " ").replace(/\w+/g, w => w[0]).replace(/\s/g, "");
export const hasGuildFeature = (guild, feature) => guild.features?.has(feature) ?? false;
