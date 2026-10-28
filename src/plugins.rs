//! Vencord's plugins, done natively. Which are on and their options live in `plugins.json`
//! in Vencord's own shape (`{"Name": {"enabled": true, "key": value}}`), so a Vencord
//! install's choices import as they are.

use serde_json::{Map, Value};
use std::sync::{LazyLock, RwLock};

pub enum Kind {
    Bool(bool),
    Num(f64),
    Text(&'static str),
    // (value, label); the first is the default
    Pick(&'static [(&'static str, &'static str)]),
    // lo, hi, default
    Slider(f64, f64, f64),
}

pub struct Opt {
    pub key: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
}

pub struct Plugin {
    pub name: &'static str,
    pub desc: &'static str,
    pub opts: &'static [Opt],
}

const fn b(key: &'static str, desc: &'static str, v: bool) -> Opt {
    Opt { key, desc, kind: Kind::Bool(v) }
}

const fn n(key: &'static str, desc: &'static str, v: f64) -> Opt {
    Opt { key, desc, kind: Kind::Num(v) }
}

const fn t(key: &'static str, desc: &'static str, v: &'static str) -> Opt {
    Opt { key, desc, kind: Kind::Text(v) }
}

const fn pick(key: &'static str, desc: &'static str, v: &'static [(&'static str, &'static str)]) -> Opt {
    Opt { key, desc, kind: Kind::Pick(v) }
}

const fn sl(key: &'static str, desc: &'static str, lo: f64, hi: f64, v: f64) -> Opt {
    Opt { key, desc, kind: Kind::Slider(lo, hi, v) }
}

const fn p(name: &'static str, desc: &'static str, opts: &'static [Opt]) -> Plugin {
    Plugin { name, desc, opts }
}

pub const ALL: &[Plugin] = &[
    p("AccountPanelServerProfile", "Right click your account panel in the bottom left to view your profile in the current server", &[
        b("prioritizeServerProfile", "Prioritize Server Profile when left clicking your account panel", false),
    ]),
    p("AddAttachments", "Allows you to add new attachments while editing messages", &[]),
    p("AlwaysAnimate", "Animates anything that can be animated", &[]),
    p("AlwaysExpandRoles", "Always expands the role list in profile popouts", &[]),
    p("AlwaysTrust", "Removes the annoying untrusted domain and suspicious file popup", &[
        b("domain", "Remove the untrusted domain popup when opening links", true),
        b("file", "Remove the 'Potentially Dangerous Download' popup when opening links", true),
    ]),
    p("AnonymiseFileNames", "Anonymise uploaded file names", &[
        b("anonymiseByDefault", "Whether to anonymise file names by default", true),
        pick("method", "Anonymising method", &[("0", "Random Characters"), ("1", "Consistent"), ("2", "Timestamp")]),
        n("randomisedLength", "Random characters length", 7.0),
        t("consistent", "Consistent filename", "image"),
    ]),
    p("BetterGifPicker", "Makes the gif picker open the favourite category by default", &[]),
    p("BetterRoleContext", "Adds options to copy role color / edit role / view role icon when right clicking roles in the user profile or in the member list", &[
        pick("roleIconFileFormat", "File format to use when viewing role icons", &[("png", "png"), ("webp", "webp"), ("jpg", "jpg")]),
    ]),
    p("BetterRoleDot", "Copy role colour on RoleDot (accessibility setting) click. Also allows using both RoleDot and coloured names simultaneously", &[
        b("bothStyles", "Show both role dot and coloured names", false),
        b("copyRoleColorInProfilePopout", "Allow click on role dot in profile popout to copy role color", false),
    ]),
    p("BetterSessions", "Enhances the sessions (devices) menu. Allows you to view exact timestamps, give each session a custom name, and receive notifications about new sessions.", &[
        b("backgroundCheck", "Check for new sessions in the background, and display notifications when they are detected", false),
        n("checkInterval", "How often to check for new sessions in the background (if enabled), in minutes", 20.0),
    ]),
    p("BetterUploadButton", "Upload with a single click, open menu with right click", &[]),
    p("BlurNSFW", "Blur attachments in NSFW channels until hovered", &[n("blurAmount", "Blur Amount (in pixels)", 10.0)]),
    p("CallTimer", "Adds a timer to vcs", &[pick("format", "The timer format", &[("stopwatch", "30d 23:00:42"), ("human", "30d 23h 00m 42s")])]),
    p("CharacterCounter", "Adds a character counter to the chat input", &[
        b("colorEffects", "Enable yellow/red colouring as you get closer to the character limit", true),
    ]),
    p("ClearURLs", "Automatically removes tracking elements from URLs you send", &[]),
    p("ClientTheme", "Recreation of the old client theme experiment. Add a color to your Discord client theme", &[
        t("color", "Theme colour, as hex", "313338"),
    ]),
    p("ColorSighted", "Removes the colorblind-friendly icons from statuses, just like 2015-2017 Discord", &[]),
    p("CopyEmojiMarkdown", "Allows you to copy emojis as formatted string (<:blobcatcozy:1026533070955872337>)", &[
        b("copyUnicode", "Copy the raw unicode character instead of :name: for default emojis", true),
    ]),
    p("CopyFileContents", "Adds a button to text file attachments to copy their contents", &[]),
    p("CopyStickerLinks", "Adds the ability to copy & open Sticker links", &[]),
    p("CopyUserURLs", "Adds a 'Copy User URL' option to the user context menu.", &[]),
    p("CustomIdle", "Allows you to set the time before Discord goes idle (or disable auto-idle)", &[
        sl("idleTimeout", "Minutes before Discord goes idle (0 to disable auto-idle)", 0.0, 60.0, 10.0),
    ]),
    p("CustomRPC", "Add a fully customisable Rich Presence (Game status) to your Discord profile", &[
        b("enablePresence", "Whether to show the presence", true),
        t("appID", "Application ID (required)", ""),
        t("appName", "Application name (required)", ""),
        t("details", "Details (line 1)", ""),
        t("detailsURL", "Details URL", ""),
        t("state", "State (line 2)", ""),
        t("stateURL", "State URL", ""),
        pick("type", "Activity type", &[("0", "Playing"), ("1", "Streaming"), ("2", "Listening"), ("3", "Watching"), ("5", "Competing")]),
        t("streamLink", "Twitch.tv or Youtube.com link (only for Streaming activity type)", ""),
        pick("timestampMode", "Timestamp mode", &[("0", "None"), ("1", "Since discord open"), ("2", "Same as your current time"), ("3", "Custom")]),
        n("startTime", "Start timestamp in milliseconds (only for custom timestamp mode)", 0.0),
        n("endTime", "End timestamp in milliseconds (only for custom timestamp mode)", 0.0),
        t("imageBig", "Big image key/link", ""),
        t("imageBigURL", "Big image URL", ""),
        t("imageBigTooltip", "Big image tooltip", ""),
        t("imageSmall", "Small image key/link", ""),
        t("imageSmallURL", "Small image URL", ""),
        t("imageSmallTooltip", "Small image tooltip", ""),
        t("buttonOneText", "Button 1 text", ""),
        t("buttonOneURL", "Button 1 URL", ""),
        t("buttonTwoText", "Button 2 text", ""),
        t("buttonTwoURL", "Button 2 URL", ""),
        n("partySize", "Party size", 0.0),
        n("partyMaxSize", "Maximum party size", 0.0),
    ]),
    p("Dearrow", "Makes YouTube embed titles and thumbnails less sensationalist, powered by Dearrow", &[
        b("hideButton", "Hides the Dearrow button from YouTube embeds", false),
        pick("replaceElements", "Choose which elements of the embed will be replaced", &[("0", "Everything (Titles & Thumbnails)"), ("1", "Titles"), ("2", "Thumbnails")]),
        b("dearrowByDefault", "Dearrow videos automatically", true),
    ]),
    p("ExpressionCloner", "Allows you to clone Emotes & Stickers to your own server (right click them)", &[]),
    p("FakeNitro", "Allows you to send fake emojis/stickers, use nitro themes, and stream in nitro quality", &[
        b("enableEmojiBypass", "Allows sending fake emojis (also bypasses missing permission to use custom emojis)", true),
        pick("emojiSize", "Size of the emojis when sending", &[("48", "48"), ("32", "32"), ("64", "64"), ("128", "128"), ("160", "160"), ("256", "256"), ("512", "512")]),
        b("transformEmojis", "Whether to transform fake emojis into real ones", true),
        b("enableStickerBypass", "Allows sending fake stickers (also bypasses missing permission to use stickers)", true),
        pick("stickerSize", "Size of the stickers when sending", &[("160", "160"), ("32", "32"), ("64", "64"), ("128", "128"), ("256", "256"), ("512", "512")]),
        b("useHyperLinks", "Whether to use hyperlinks when sending fake emojis and stickers", true),
        t("hyperLinkText", "What text the hyperlink should use. {{NAME}} will be replaced with the emoji/sticker name.", "{{NAME}}"),
    ]),
    p("FavoriteEmojiFirst", "Puts your favorite emoji first in the emoji autocomplete.", &[]),
    p("FixImagesQuality", "Improves quality of images by loading them at their original resolution", &[
        b("originalImagesInChat", "Also load the original image in Chat", false),
    ]),
    p("ForceOwnerCrown", "Force the owner crown next to usernames even if the server is large.", &[]),
    p("FriendInvites", "Create and manage friend invite links via slash commands (/create friend invite, /view friend invites, /revoke friend invites).", &[]),
    p("GameActivityToggle", "Adds a button next to the mic and deafen button to toggle game activity.", &[]),
    p("GifPaste", "Makes picking a gif in the gif picker insert a link into the chatbox instead of instantly sending it", &[]),
    p("HideMedia", "Hide attachments and embeds for individual messages via hover button", &[]),
    p("IgnoreActivities", "Ignore activities from showing up on your status ONLY.", &[
        pick("listMode", "Change the mode of the filter list", &[("1", "Blacklist"), ("0", "Whitelist")]),
        t("idsList", "Comma separated list of game names to filter", ""),
        b("ignorePlaying", "Ignore all playing activities (These are usually game and RPC activities)", false),
        b("ignoreStreaming", "Ignore all streaming activities", false),
        b("ignoreListening", "Ignore all listening activities (These are usually Spotify activities)", false),
        b("ignoreWatching", "Ignore all watching activities", false),
        b("ignoreCompeting", "Ignore all competing activities (These are normally special game activities)", false),
    ]),
    p("ImageFilename", "Display the file name of images & GIFs as a tooltip when hovering over them", &[]),
    p("ImageLink", "Never hide image links in messages, even if it's the only content", &[]),
    p("ImageZoom", "Lets you zoom in to images and gifs. Use scroll wheel to zoom in and shift + scroll wheel to increase lens radius / size", &[
        b("invertScroll", "Invert scroll", true),
        b("nearestNeighbour", "Use Nearest Neighbour Interpolation when scaling images", false),
        b("square", "Make the lens square", false),
    ]),
    p("ImplicitRelationships", "Shows your implicit relationships in the Friends tab.", &[
        b("sortByAffinity", "Whether to sort implicit relationships by their affinity to you.", true),
    ]),
    p("IrcColors", "Makes username colors in chat unique, like in IRC clients", &[
        n("lightness", "Lightness, in %. Change if the colors are too light or too dark", 70.0),
        b("memberListColors", "Replace role colors in the member list", true),
        b("applyColorOnlyToUsersWithoutColor", "Only apply colors to users who don't have a predefined color", false),
        b("applyColorOnlyInDms", "Only apply colors in DMs", false),
    ]),
    p("MemberCount", "Shows the number of online members, total members, and users in voice channels on the server — in the member list and tooltip.", &[
        b("toolTip", "Show member count on the server tooltip", true),
        b("memberList", "Show member count in the member list", true),
        b("voiceActivity", "Show voice activity with member count in the member list", true),
    ]),
    p("MentionAvatars", "Shows user avatars and role icons inside mentions", &[
        b("showAtSymbol", "Whether the the @ symbol should be displayed on user mentions", true),
    ]),
    p("MessageClickActions", "Hold Backspace and click to delete, double click to edit/reply", &[
        b("enableDeleteOnClick", "Enable delete on click while holding backspace", true),
        b("enableDoubleClickToEdit", "Enable double click to edit", true),
        b("enableDoubleClickToReply", "Enable double click to reply", true),
        b("requireModifier", "Only do double click actions when shift/ctrl is held", false),
    ]),
    p("MessageLatency", "Displays an indicator for messages that took ≥n seconds to send", &[
        n("latency", "Threshold in seconds for latency indicator", 2.0),
        b("showMillis", "Show milliseconds", false),
        b("ignoreSelf", "Don't add indicator to your own messages", false),
    ]),
    p("MessageLinkEmbeds", "Adds a preview to messages that link another message", &[
        pick("listMode", "Whether to show embeds for every channel, or only some", &[("blacklist", "Blacklist"), ("whitelist", "Whitelist")]),
        t("idList", "Guild/channel/user IDs to blacklist or whitelist (separate with comma)", ""),
    ]),
    p("MessageLogger", "Temporarily logs deleted and edited messages.", &[
        pick("deleteStyle", "The style of deleted messages", &[("text", "Red text"), ("overlay", "Red overlay")]),
        b("logDeletes", "Whether to log deleted messages", true),
        b("logEdits", "Whether to log edited messages", true),
        b("logDeletedAttachments", "Whether to log deleted attachments", true),
        b("inlineEdits", "Whether to display edit history as part of message content", true),
        b("ignoreBots", "Whether to ignore messages by bots", true),
        b("ignoreSelf", "Whether to ignore messages by yourself", false),
        t("ignoreUsers", "Comma-separated list of user IDs to ignore", ""),
        t("ignoreChannels", "Comma-separated list of channel IDs to ignore", ""),
        t("ignoreGuilds", "Comma-separated list of guild IDs to ignore", ""),
    ]),
    p("MoreQuickReactions", "Increases the number of reactions available in the Quick React hover menu", &[
        n("reactionCount", "Number of reactions (0-42)", 5.0),
    ]),
    p("MutualGroupDMs", "Shows mutual group dms in profiles", &[]),
    p("NewGuildSettings", "Automatically mute new servers and change various other settings upon joining", &[
        b("guild", "Mute Guild automatically", true),
        pick("messages", "Server Notification Settings", &[("3", "Server default"), ("0", "All messages"), ("1", "Only @mentions"), ("2", "Nothing")]),
        b("everyone", "Suppress @everyone and @here", true),
        b("role", "Suppress All Role @mentions", true),
        b("highlights", "Suppress Highlights automatically", true),
        b("events", "Mute New Events automatically", true),
    ]),
    p("NoBlockedMessages", "Hides all blocked/ignored messages from chat completely", &[
        b("ignoreMessages", "Completely ignores incoming messages from blocked and ignored (if enabled) users", false),
    ]),
    p("NoMosaic", "Removes Discord image mosaic", &[]),
    p("NoPendingCount", "Removes the ping count of incoming friend requests, message requests, and nitro offers.", &[
        b("hideFriendRequestsCount", "Hide incoming friend requests count", true),
    ]),
    p("NoProfileThemes", "Completely removes Nitro profile themes from everyone but yourself", &[]),
    p("NoReplyMention", "Disables reply pings by default", &[
        t("userList", "List of users to allow or exempt pings for (separated by commas or spaces)", ""),
        t("roleList", "List of roles to allow or exempt pings for (separated by commas or spaces)", ""),
        pick("shouldPingListed", "Behaviour", &[("true", "Do not ping unless listed"), ("false", "Ping unless listed")]),
        b("inverseShiftReply", "Invert Discord's shift replying behaviour (enable to make shift reply mention user)", false),
    ]),
    p("NoServerEmojis", "Do not show server emojis in the autocomplete menu.", &[
        pick("shownEmojis", "The types of emojis to show in the autocomplete menu.", &[("onlyUnicode", "Only unicode emojis"), ("currentServer", "Unicode emojis and server emojis from current server"), ("all", "Show all emojis (same as disabling this plugin)")]),
    ]),
    p("NotificationVolume", "Save your ears and set a separate volume for notifications and in-app sounds", &[
        sl("notificationVolume", "Notification volume", 0.0, 100.0, 100.0),
    ]),
    p("NoTypingAnimation", "Disables the CPU-intensive typing dots animation", &[]),
    p("oneko", "cat follow mouse (real)", &[]),
    p("OnePingPerDM", "If unread messages are sent by a user in DMs multiple times, you'll only receive one audio ping. Read the messages to reset the limit", &[
        pick("channelToAffect", "Select the type of DM for the plugin to affect", &[("both_dms", "Both"), ("user_dm", "User DMs"), ("group_dm", "Group DMs")]),
        b("allowMentions", "Receive audio pings for @mentions", false),
        b("allowEveryone", "Receive audio pings for @everyone and @here in group DMs", false),
    ]),
    p("OpenInApp", "Open links in their respective apps instead of your browser", &[
        b("spotify", "Open Spotify links in the Spotify app", true),
        b("steam", "Open Steam links in the Steam app", true),
        b("epic", "Open Epic Games links in the Epic Games Launcher", true),
        b("tidal", "Open Tidal links in the Tidal app", true),
        b("itunes", "Open Apple Music links in the iTunes app", true),
    ]),
    p("OverrideForumDefaults", "Allows you to override default forum layout/sort order. you can still change it on a per-channel basis", &[
        pick("defaultLayout", "Which layout to use as default", &[("1", "List"), ("2", "Gallery")]),
        pick("defaultSortOrder", "Which sort order to use as default", &[("0", "Recently Active"), ("1", "Date Posted")]),
    ]),
    p("PauseInvitesForever", "Brings back the option to pause invites indefinitely that stupit Discord removed.", &[]),
    p("PermissionsViewer", "View the permissions a user or channel has, and the roles of a server", &[
        pick("permissionsSortOrder", "The sort method used for defining which role grants an user a certain permission", &[("0", "Highest Role"), ("1", "Lowest Role")]),
    ]),
    p("petpet", "Adds a /petpet slash command to create headpet gifs from any image", &[]),
    p("PinDMs", "Allows you to pin private channels to the top of your DM list. To pin/unpin or re-order pins, right click DMs", &[
        pick("pinOrder", "Which order should pinned DMs be displayed in?", &[("0", "Most recent message"), ("1", "Custom (right click channels to reorder)")]),
    ]),
    p("PlainFolderIcon", "Dont show the small guild icons in folders", &[]),
    p("PlatformIndicators", "Adds platform indicators (Desktop, Mobile, Web...) to users", &[
        b("list", "Show indicators in the member list", true),
        b("badges", "Show indicators in user profiles, as badges", true),
        b("messages", "Show indicators inside messages", true),
        b("colorMobileIndicator", "Whether to make the mobile indicator match the color of the user status.", true),
    ]),
    p("PreviewMessage", "Lets you preview your message before sending it.", &[]),
    p("QuickMention", "Adds a quick mention button to the message actions bar", &[]),
    p("QuickReply", "Reply to (ctrl + up/down) and edit (ctrl + shift + up/down) messages via keybinds", &[
        pick("shouldMention", "Ping reply by default", &[("2", "Follow NoReplyMention plugin (if enabled)"), ("1", "Enabled"), ("0", "Disabled")]),
        b("ignoreBlockedAndIgnored", "Ignore messages by blocked/ignored users when navigating", true),
    ]),
    p("ReadAllNotificationsButton", "Read all server notifications with a single button click!", &[]),
    p("RelationshipNotifier", "Notifies you when a friend, group chat, or server removes you.", &[
        b("offlineRemovals", "Notify you when starting discord if you were removed while offline.", true),
        b("friends", "Notify when a friend removes you", true),
        b("friendRequestCancels", "Notify when a friend request is cancelled", true),
        b("servers", "Notify when removed from a server", true),
        b("groups", "Notify when removed from a group chat", true),
    ]),
    p("ReplyTimestamp", "Shows a timestamp on replied-message previews", &[]),
    p("RevealAllSpoilers", "Reveal all spoilers in a message by Ctrl-clicking a spoiler, or in the chat with Ctrl+Shift-click", &[]),
    p("ReverseImageSearch", "Adds ImageSearch to image context menus", &[]),
    p("RoleColorEverywhere", "Adds the top role color anywhere possible", &[
        b("chatMentions", "Show role colors in chat mentions (including in the message box)", true),
        b("memberList", "Show role colors in member list role headers", true),
        b("voiceUsers", "Show role colors in the voice chat user list", true),
        b("colorChatMessages", "Color chat messages based on the author's role color", false),
        sl("messageSaturation", "Intensity of message coloring.", 0.0, 100.0, 30.0),
    ]),
    p("SecretRingToneEnabler", "Always play the secret version of the discord ringtone (except during special ringtone events)", &[]),
    p("SendTimestamps", "Send timestamps easily via chat box button & text shortcuts.", &[
        b("replaceMessageContents", "Replace timestamps in message contents", true),
    ]),
    p("ServerInfo", "Allows you to view info about a server", &[]),
    p("ServerListIndicators", "Add online friend count or server count in the server list", &[
        pick("mode", "mode", &[("1", "Only online friend count"), ("2", "Only server count"), ("3", "Both server and online friend counts")]),
    ]),
    p("ShowAllMessageButtons", "Always show all message buttons no matter if you are holding the shift key or not.", &[]),
    p("ShowConnections", "Show connected accounts in user popouts", &[
        n("iconSize", "Icon size (px)", 32.0),
        pick("iconSpacing", "Icon margin", &[("1", "Cozy"), ("0", "Compact"), ("2", "Roomy")]),
    ]),
    p("ShowHiddenChannels", "Show channels that you do not have access to view.", &[
        b("hideUnreads", "Hide Unreads", true),
    ]),
    p("ShowMeYourName", "Display usernames next to nicks, or no nicks at all", &[
        pick("mode", "How to display usernames and nicks", &[("user-nick", "Username then nickname"), ("nick-user", "Nickname then username"), ("user", "Username only")]),
        b("displayNames", "Use display names in place of usernames", false),
        b("inReplies", "Also apply functionality to reply previews", false),
    ]),
    p("SilentMessageToggle", "Adds a button to the chat bar to toggle sending a silent message.", &[
        b("persistState", "Whether to persist the state of the silent message toggle when changing channels", false),
        b("autoDisable", "Automatically disable the silent message toggle again after sending one", true),
    ]),
    p("SilentTyping", "Hide that you are typing", &[
        b("showIcon", "Show an icon for toggling the plugin", false),
        b("isEnabled", "Toggle functionality", true),
    ]),
    p("SortFriendRequests", "Sorts friend requests by date of receipt", &[b("showDates", "Show dates on friend requests", false)]),
    p("SpotifyShareCommands", "Share your current Spotify track, album or artist via slash command (/track, /album, /artist)", &[]),
    p("StickerPaste", "Makes picking a sticker in the sticker picker insert it into the chatbox instead of instantly sending", &[]),
    p("StreamerModeOnStream", "Automatically enables streamer mode when you start streaming in Discord", &[]),
    p("TextReplace", "Replace text in your messages.", &[]),
    p("Translate", "Translate messages with Google Translate.", &[
        t("receivedInput", "Language incoming messages are translated from", "auto"),
        t("receivedOutput", "Language incoming messages are translated to", "en"),
        t("sentInput", "Language your messages are translated from", "auto"),
        t("sentOutput", "Language your messages are translated to", "en"),
        b("autoTranslate", "Automatically translate your messages before sending. You can also Shift+click or right-click the translate button to toggle this", false),
    ]),
    p("TypingIndicator", "Adds an indicator if someone is typing on a channel.", &[
        b("includeCurrentChannel", "Whether to show the typing indicator for the currently selected channel", true),
        b("includeMutedChannels", "Whether to show the typing indicator for muted channels.", false),
        b("includeBlockedUsers", "Whether to show the typing indicator for blocked users.", false),
        pick("indicatorMode", "How should the indicator be displayed?", &[("3", "Avatars and animated dots"), ("1", "Animated dots"), ("2", "Avatars")]),
    ]),
    p("TypingTweaks", "Show avatars and role colours in the typing indicator", &[
        b("showAvatars", "Show avatars in the typing indicator", true),
        b("showRoleColors", "Show role colors in the typing indicator", true),
        b("alternativeFormatting", "Show a more useful message when several users are typing", true),
    ]),
    p("Unindent", "Trims leading indentation from codeblocks", &[]),
    p("UnsuppressEmbeds", "Allows you to unsuppress embeds in messages", &[]),
    p("UserMessagesPronouns", "Adds pronouns to chat user messages", &[
        pick("pronounsFormat", "The format for pronouns to appear in chat", &[("LOWERCASE", "Lowercase"), ("CAPITALIZED", "Capitalized")]),
        b("showSelf", "Enable or disable showing pronouns for yourself", true),
    ]),
    p("UserVoiceShow", "Shows an indicator when a user is in a Voice Channel", &[
        b("showInUserProfileModal", "Show a user's Voice Channel indicator in their profile next to the name", true),
        b("showInMemberList", "Show a user's Voice Channel indicator in the member and DMs list", true),
        b("showInMessages", "Show a user's Voice Channel indicator in messages", true),
    ]),
    p("USRBG", "Displays user banners from USRBG, allowing anyone to get a banner without Nitro", &[
        b("nitroFirst", "Prefer Nitro banners over USRBG banners", true),
        b("voiceBackground", "Use USRBG banners as voice chat backgrounds", true),
    ]),
    p("ValidUser", "Fix mentions for unknown users showing up as '@unknown-user' (hover over a mention to fix it)", &[]),
    p("VcNarrator", "Announces when users join, leave, or move voice channels via narrator", &[
        sl("volume", "Narrator Volume", 0.0, 1.0, 1.0),
        sl("rate", "Narrator Speed", 0.1, 10.0, 1.0),
        b("sayOwnName", "Say own name", false),
        b("latinOnly", "Strip non latin characters from names before saying them", false),
        t("joinMessage", "Join Message", "{{USER}} joined"),
        t("leaveMessage", "Leave Message", "{{USER}} left"),
        t("moveMessage", "Move Message", "{{USER}} moved to {{CHANNEL}}"),
    ]),
    p("ViewIcons", "Makes avatars and banners in user profiles clickable, adds View Icon/Banner entries in the user, server and group channel context menu.", &[
        pick("format", "Choose the image format to use for non-animated images. Animated images will always use .webp", &[("webp", "webp"), ("png", "png"), ("jpg", "jpg")]),
        pick("imgSize", "The image size to use", &[("1024", "1024"), ("128", "128"), ("256", "256"), ("512", "512"), ("2048", "2048"), ("4096", "4096")]),
    ]),
    p("ViewRaw", "Copy and view the raw content/data of any message, channel or guild", &[]),
    p("VoiceChatDoubleClick", "Join voice chats via double click instead of single click", &[]),
    p("VoiceDownload", "Adds a download to voice messages.", &[]),
    p("WhoReacted", "Renders the avatars of users who reacted to a message", &[]),
    p("XSOverlay", "Forwards discord notifications to XSOverlay, for easy viewing in VR", &[
        n("webSocketPort", "Websocket port", 42070.0),
        b("botNotifications", "Allow bot notifications", false),
        b("serverNotifications", "Allow server notifications", true),
        b("dmNotifications", "Allow Direct Message notifications", true),
        b("groupDmNotifications", "Allow Group DM notifications", true),
        b("callNotifications", "Allow call notifications", true),
        n("timeout", "Notification duration (secs)", 3.0),
        sl("opacity", "Notif opacity", 0.0, 1.0, 1.0),
        sl("volume", "Volume", 0.0, 1.0, 0.2),
    ]),
];

static CFG: LazyLock<RwLock<Map<String, Value>>> = LazyLock::new(|| {
    let m = std::fs::read(path())
        .ok()
        .and_then(|b| serde_json::from_slice::<Map<String, Value>>(&b).ok())
        .unwrap_or_default();
    RwLock::new(m)
});

fn path() -> std::path::PathBuf {
    crate::cache::dir().join("plugins.json")
}

pub fn find(name: &str) -> Option<&'static Plugin> {
    ALL.iter().find(|p| p.name == name)
}

pub fn on(name: &str) -> bool {
    CFG.read().unwrap().get(name).and_then(|c| c["enabled"].as_bool()).unwrap_or(false)
}

/// A plugin's option, or its default. Plugins keep their own data here too (rules, pins,
/// names), the way Vencord does.
pub fn get(name: &str, key: &str) -> Value {
    if let Some(v) = CFG.read().unwrap().get(name).and_then(|c| c.get(key)) {
        return v.clone();
    }
    let Some(o) = find(name).and_then(|p| p.opts.iter().find(|o| o.key == key)) else { return Value::Null };
    match o.kind {
        Kind::Bool(v) => v.into(),
        Kind::Num(v) | Kind::Slider(_, _, v) => v.into(),
        Kind::Text(v) => v.into(),
        Kind::Pick(v) => v[0].0.into(),
    }
}

/// On, and its bool option set.
pub fn flag(name: &str, key: &str) -> bool {
    on(name) && get(name, key).as_bool().unwrap_or(false)
}

pub fn num(name: &str, key: &str) -> f64 {
    let v = get(name, key);
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0)
}

pub fn text(name: &str, key: &str) -> String {
    match get(name, key) {
        Value::String(s) => s,
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// A comma or space separated id list option.
pub fn ids(name: &str, key: &str) -> Vec<u64> {
    text(name, key).split([',', ' ']).filter_map(|s| s.trim().parse().ok()).collect()
}

pub fn set(name: &str, key: &str, v: Value) {
    let mut m = CFG.write().unwrap();
    if let Value::Object(c) = m.entry(name).or_insert_with(|| Value::Object(Map::new())) {
        c.insert(key.into(), v);
    }
    save(&m);
}

pub fn enable(name: &str, on: bool) {
    set(name, "enabled", on.into());
}

fn save(m: &Map<String, Value>) {
    if let Ok(b) = serde_json::to_vec_pretty(m) {
        let _ = std::fs::write(path(), b);
    }
}

pub fn vencord_settings() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Vencord").join("settings").join("settings.json")
}

/// Copies Vencord's choices for every plugin Fastcord has. Returns how many were turned on.
pub fn import_vencord() -> usize {
    let Some(theirs) = std::fs::read(vencord_settings())
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["plugins"].as_object().cloned())
    else {
        return 0;
    };
    let mut m = CFG.write().unwrap();
    let mut count = 0;
    for p in ALL {
        if let Some(c) = theirs.get(p.name).filter(|c| c.is_object()) {
            count += c["enabled"].as_bool().unwrap_or(false) as usize;
            m.insert(p.name.into(), c.clone());
        }
    }
    save(&m);
    count
}
