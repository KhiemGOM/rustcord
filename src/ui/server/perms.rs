pub type Perm = (u64, &'static str, &'static str);

const GENERAL: &[Perm] = &[
    (1 << 10, "View Channels", "Allows members to view channels by default (excluding private channels)."),
    (1 << 4, "Manage Channels", "Allows members to create, edit, or delete channels."),
    (1 << 28, "Manage Roles", "Allows members to create new roles and edit or delete roles lower than their highest role. Also allows members to change permissions of individual channels that they have access to."),
    (1 << 43, "Create Expressions", "Allows members to add custom emoji, stickers, and sounds in this server."),
    (1 << 30, "Manage Expressions", "Allows members to edit or remove custom emoji, stickers, and sounds in this server."),
    (1 << 7, "View Audit Log", "Allows members to view a record of who made which changes in this server."),
    (1 << 29, "Manage Webhooks", "Allows members to create, edit, or delete webhooks, which can post messages from other apps or sites into this server."),
    (1 << 5, "Manage Server", "Allows members to change this server\u{2019}s name, switch regions, view all invites, add apps to this server and create and update AutoMod rules."),
];
const MEMBERSHIP: &[Perm] = &[
    (1, "Create Invite", "Allows members to invite new people to this server."),
    (1 << 26, "Change Nickname", "Allows members to change their own nickname, a custom name for just this server."),
    (1 << 27, "Manage Nicknames", "Allows members to change the nicknames of other members."),
    (1 << 1, "Kick, Approve, and Reject Members", "Allows members to remove other members from this server. Kicked members will be able to rejoin if they have another invite."),
    (1 << 2, "Ban Members", "Allows members to permanently ban and delete the message history of other members from this server."),
    (1 << 40, "Timeout Members", "When you put a user in timeout they will not be able to send messages in chat, reply within threads, react to messages, or speak in voice or Stage channels."),
];
const TEXT: &[Perm] = &[
    (1 << 11, "Send Messages and Create Posts", "Allow members to send messages in text channels and create posts in forum channels."),
    (1 << 38, "Send Messages in Threads and Posts", "Allow members to send messages in threads and in posts on forum channels."),
    (1 << 35, "Create Public Threads", "Allow members to create threads that everyone in a channel can view."),
    (1 << 36, "Create Private Threads", "Allow members to create invite-only threads."),
    (1 << 14, "Embed Links", "Allows links that members share to show embedded content in text channels."),
    (1 << 15, "Attach Files", "Allows members to upload files or media in text channels."),
    (1 << 6, "Add Reactions", "Allows members to add new emoji reactions to a message. If this permission is disabled, members can still react using any existing reactions on a message."),
    (1 << 18, "Use External Emoji", "Allows members to use emoji from other servers, if they\u{2019}re a Discord Nitro member."),
    (1 << 37, "Use External Stickers", "Allows members to use stickers from other servers, if they\u{2019}re a Discord Nitro member."),
    (1 << 17, "Mention @everyone, @here, and All Roles", "Allows members to use @everyone (everyone in the server) or @here (only online members in that channel). They can also @mention all roles, even if the role\u{2019}s \u{201c}Allow anyone to @mention this role\u{201d} permission is disabled."),
    (1 << 13, "Manage Messages", "Allows members to delete messages by other members or remove embeds from them."),
    (1 << 51, "Pin Messages", "Allows members to pin or unpin any message."),
    (1 << 52, "Bypass Slowmode", "Allows members to send messages without being affected by slowmode."),
    (1 << 34, "Manage Threads and Posts", "Allows members to rename, delete, close, and turn on slow mode for threads and posts. They can also view private threads."),
    (1 << 16, "Read Message History", "Allows members to read previous messages sent in channels. If this permission is disabled, members only see messages sent when they are online. This does not fully apply to threads and forum posts."),
    (1 << 12, "Send Text-to-Speech Messages", "Allows members to send text-to-speech messages by starting a message with /tts. These messages can be heard by anyone focused on the channel."),
    (1 << 46, "Send Voice Messages", "Allows members to send voice messages."),
    (1 << 49, "Create Polls", "Allows members to create polls."),
];
const VOICE: &[Perm] = &[
    (1 << 20, "Connect", "Allows members to join voice channels and hear others."),
    (1 << 21, "Speak", "Allows members to talk in voice channels. If this permission is disabled, members are default muted until somebody with the \u{201c}Mute Members\u{201d} permission un-mutes them."),
    (1 << 9, "Video", "Allows members to share their video, screen share, or stream a game in this server."),
    (1 << 42, "Use Soundboard", "Allows members to send sounds from server soundboard."),
    (1 << 45, "Use External Sounds", "Allows members to use sounds from other servers, if they\u{2019}re a Discord Nitro member."),
    (1 << 25, "Use Voice Activity", "Allows members to speak in voice channels by simply talking. If this permission is disabled, members are required to use Push-to-talk. Good for controlling background noise or noisy members."),
    (1 << 8, "Priority Speaker", "Allows members to be more easily heard in voice channels. When activated, the volume of others without this permission will be automatically lowered."),
    (1 << 22, "Mute Members", "Allows members to mute other members in voice channels for everyone."),
    (1 << 23, "Deafen Members", "Allows members to deafen other members in voice channels, which means they won\u{2019}t be able to speak or hear others."),
    (1 << 24, "Move Members", "Allows members to disconnect or move other members between voice channels that the member with this permission has access to."),
    (1 << 48, "Set Voice Channel Status", "Allows members to create and edit voice channel status."),
];
const STAGE: &[Perm] = &[(1 << 32, "Request to Speak", "Allow members to request to speak in Stage channels. Stage moderators can approve or deny each request.")];
const APPS: &[Perm] = &[
    (1 << 31, "Use Application Commands", "Allows members to use commands from applications, including slash commands and context menu commands."),
    (1 << 39, "Use Activities", "Allows members to use Activities."),
    (1 << 50, "Use External Apps", "Allows apps that members have added to their account to post messages in channels. When disabled, the messages will be private."),
];
const EVENTS: &[Perm] = &[(1 << 44, "Create Events", "Allows members to create events."), (1 << 33, "Manage Events", "Allows members to edit and cancel events.")];
const ADVANCED: &[Perm] = &[
    (1 << 3, "Administrator", "Members with this permission have every permission and also bypass channel specific permissions. This is a dangerous permission to grant."),
    (1 << 19, "View Server Insights", "Allows members to view Server Insights, which shows data on community growth, engagement, and more. This will allow them to see certain data about channel activity, even for channels they cannot access."),
    (1 << 41, "View Creator Analytics", "Allows members to view Server Subscription Insights, which shows data on revenue, subscribers, and free trials."),
];

pub const SECTIONS: &[(&str, &[Perm])] = &[
    ("General Server Permissions", GENERAL),
    ("Membership Permissions", MEMBERSHIP),
    ("Text Channel Permissions", TEXT),
    ("Voice Channel Permissions", VOICE),
    ("Stage Channel Permissions", STAGE),
    ("Apps Permissions", APPS),
    ("Events Permissions", EVENTS),
    ("Advanced Permissions", ADVANCED),
];
