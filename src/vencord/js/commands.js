// @api/Commands: plugin slash commands, offered in Fastcord's own slash menu.
export { ApplicationCommandInputType, ApplicationCommandOptionType, ApplicationCommandType } from "@vencord/discord-types/enums";

export const commands = {};
export const BUILT_IN = [];
export const OptionalMessageOption = { name: "message", description: "The message", type: 3, required: false };
export const RequiredMessageOption = { name: "message", description: "The message", type: 3, required: true };

export function registerCommand(command, plugin) {
    if (commands[command.name]) throw new Error(`Command '${command.name}' already exists.`);
    command.plugin = plugin;
    command.isVencordCommand = true;
    commands[command.name] = command;
}

export function unregisterCommand(name) {
    return delete commands[name];
}

export const prepareOption = o => o;

let nextId = 1;
export const generateId = () => `-${Date.now()}${nextId++}`;

export function sendBotMessage(channelId, message) {
    const msg = { id: generateId(), channel_id: channelId, author: { id: "-1", username: "Clyde", bot: true }, content: "", embeds: [], ...message };
    __out({ op: "bot", channel: String(channelId), content: String(msg.content ?? ""), embeds: msg.embeds ?? [] });
    return msg;
}

export function findOption(args, name, fallback) {
    return args?.find(a => a.name === name)?.value ?? fallback;
}
