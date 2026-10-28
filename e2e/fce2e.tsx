/*
 * A Vencord userplugin the e2e run installs, to check Fastcord's Vencord host end to end:
 * settings, a slash command, a send listener, a message menu item, a chat bar button and flux.
 */

import { ChatBarButton } from "@api/ChatButtons";
import { definePluginSettings } from "@api/Settings";
import definePlugin, { OptionType } from "@utils/types";
import { Menu, showToast } from "@webpack/common";

const settings = definePluginSettings({
    tag: { type: OptionType.STRING, description: "Added to messages starting with vc:", default: " [vc]" },
    clicked: { type: OptionType.BOOLEAN, description: "The chat bar button was clicked", default: false },
});

export default definePlugin({
    name: "FcE2e",
    description: "Fastcord's end-to-end check of the Vencord host",
    authors: [{ name: "fastcord", id: 0n }],
    settings,
    commands: [{ name: "fce2e", description: "Answers in the channel", execute: () => ({ content: "command ran" }) }],
    onBeforeMessageSend(_, msg) {
        if (msg.content.startsWith("vc:")) msg.content += settings.store.tag;
    },
    contextMenus: {
        message: (children, { message }) => children.push(<Menu.MenuItem id="fc-e2e" label="E2E Quote" action={() => showToast(`quoted ${message.author.username}`)} />),
    },
    chatBarButton: {
        icon: () => null,
        render: () => (
            <ChatBarButton tooltip="E2E" onClick={() => { settings.store.clicked = true; }}>
                <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="8" fill="currentColor" /></svg>
            </ChatBarButton>
        ),
    },
    flux: {
        MESSAGE_CREATE({ message }) {
            if (message.content === "command ran") showToast("flux saw it");
        },
    },
});
