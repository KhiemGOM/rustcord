/*
 * A Vencord userplugin the e2e run installs, to check Rustcord's Vencord host end to end:
 * settings, a slash command, a send listener, menu items, a chat bar button, flux, and React UI
 * drawn natively (an accessory, a decoration, a member decorator, a badge, a modal, a setting).
 */

import { ChatBarButton } from "@api/ChatButtons";
import { showNotification } from "@api/Notifications";
import { definePluginSettings } from "@api/Settings";
import { ModalContent, ModalFooter, ModalHeader, ModalRoot, openModal } from "@utils/modal";
import definePlugin, { OptionType } from "@utils/types";
import { Button, Menu, showToast, Switch, Text, TextInput, UserStore, useState } from "@webpack/common";

const settings = definePluginSettings({
    tag: { type: OptionType.STRING, description: "Added to messages starting with vc:", default: " [vc]" },
    clicked: { type: OptionType.BOOLEAN, description: "The chat bar button was clicked", default: false },
    loud: {
        type: OptionType.COMPONENT,
        component: ({ setValue }) => {
            const [on, setOn] = useState(false);
            return <Switch value={on} onChange={v => { setOn(v); setValue(v); }} note="Drawn by the plugin">Loud</Switch>;
        },
    },
});

function Bump({ label }) {
    const [n, setN] = useState(0);
    return (
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
            <Text variant="text-sm/semibold">{`${label} ${n}`}</Text>
            <Button size={Button.Sizes.SMALL} onClick={() => setN(n + 1)}>Bump</Button>
        </div>
    );
}

function Ask({ onClose }) {
    const [text, setText] = useState("");
    return (
        <ModalRoot size="small">
            <ModalHeader><Text variant="heading-lg/semibold">E2E modal</Text></ModalHeader>
            <ModalContent><TextInput value={text} onChange={setText} placeholder="Say something" /></ModalContent>
            <ModalFooter><Button onClick={() => { showToast("modal done"); showNotification({ title: "E2E notice", body: "from the modal" }); onClose(); }}>Done</Button></ModalFooter>
        </ModalRoot>
    );
}

export default definePlugin({
    name: "FcE2e",
    description: "Rustcord's end-to-end check of the Vencord host",
    authors: [{ name: "rustcord", id: 0n }],
    settings,
    commands: [
        { name: "fce2e", description: "Answers in the channel", execute: () => ({ content: "command ran" }) },
        { name: "fcmodal", description: "Opens a modal", execute: () => { openModal(props => <Ask onClose={props.onClose} />); } },
    ],
    onBeforeMessageSend(_, msg) {
        if (msg.content.startsWith("vc:")) msg.content += settings.store.tag;
    },
    contextMenus: {
        message: (children, { message }) => children.push(<Menu.MenuItem id="fc-e2e" label="E2E Quote" action={() => showToast(`quoted ${message.author.username}`)} />),
        "channel-context": (children, { channel }) => children.push(<Menu.MenuItem id="fc-e2e-channel" label="E2E Channel" action={() => showToast(`channel ${channel.name}`)} />),
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
    renderMessageAccessory: ({ message }) => message.content.startsWith("vc:") ? <Bump label="accessory" /> : null,
    renderMessageDecoration: ({ message }) => message.author.id === UserStore.getCurrentUser()?.id ? <span style={{ color: "var(--text-positive)" }}>VC</span> : null,
    renderMemberListDecorator: () => <span>★</span>,
    userProfileBadge: { id: "fc-e2e", description: "E2E badge", iconSrc: "https://cdn.discordapp.com/embed/avatars/0.png", shouldShow: () => true },
});
