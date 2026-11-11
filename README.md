# Rustcord

A native Discord client in Rust (egui + wgpu). Same layout and colours as the desktop app, without Electron. Windows 10/11 only.

> [!WARNING]
> **Unofficial.** Rustcord is not made by, affiliated with or endorsed by Discord Inc. "Discord" and its logos are Discord's trademarks.
> Third-party clients are against [Discord's Terms of Service](https://discord.com/terms), and Discord can suspend or ban accounts that use one. Rustcord identifies itself to Discord's servers as the official desktop client so features work. Use it at your own risk, and consider a secondary account.

## Install

Download `rustcord.exe` from [Releases](https://github.com/WRChip/rustcord/releases) and run it. It needs WebView2 (built into Windows 11 and current Windows 10) for activities and video embeds.

Or build it yourself with a recent stable Rust (edition 2024):

```
cargo run --release
```

Log in with email/password (2FA supported) or paste a token. The token and a snapshot of your servers, channels and recent messages live in `%LOCALAPPDATA%\Rustcord`, which is why the second launch paints your servers before the gateway has even connected.

## What works

- servers rail (unread pills, mention badges), channel list with categories, permissions, collapsed categories, voice occupants
- DMs and group DMs
- messages: grouping, replies, markdown (bold/italic/underline/strike/spoilers/code blocks/quotes/headers/lists), mentions, custom + unicode emoji (twemoji), timestamps, attachments, image/video/rich embeds, stickers, reactions
- send, reply, edit (↑ on an empty composer), delete, react, typing indicators, file upload (button or drag & drop)
- member list with role groups and presence
- read state / acks, history paging with scroll anchoring
- server folders, threads (sidebar + header list + "N Messages" previews), forum channels (post cards, tags, new post)
- friends page (online / all / pending / blocked, accept/ignore, add by username), status picker
- profile popouts, pinned messages, message search, Ctrl+K quick switcher, join server by invite
- right-click menus: mark as read, mute, copy link, pin, mark unread, copy IDs (developer mode)
- desktop notifications for mentions and DMs, taskbar flash, unread count in the window title
- settings, laid out like Discord's (searchable sidebar, 24 pages):
  - User: My Account (display name, username, email, password, account removal), Profiles (live preview, per-server nickname, unsaved-changes bar), Content & Social, Data & Privacy, Family Center, Authorized Apps, Devices, Connections, Nitro/Subscriptions/Gift Inventory/Billing (read-only)
  - App: Appearance (Light/Ash/Dark/Onyx/Sync themes, cozy/compact, font scaling, group spacing, zoom), Accessibility, Voice & Video (devices, volumes, voice activity or push to talk), Chat, Notifications (with sounds), Keybinds (global), Language, Windows Settings (run on startup), Streamer Mode (incl. hiding the window from capture), Advanced
  - Activity: Activity Privacy, HypeSquad
- emoji render as twemoji everywhere (messages, channel names, nicknames, server names), CJK fallback fonts
- composer: Ctrl+V images/files, upload cards with spoiler toggle, `@` `#` `:` `/` autocomplete, slash commands, GIF picker, reply ping toggle
- bot buttons and select menus, jump to replied/pinned/searched messages, "new messages" and "jump to present" bars, image viewer
- shortcuts: Ctrl+K, Alt+↑/↓, Alt+Shift+↑/↓ (unread), Ctrl+E, Ctrl+Shift+U, Esc marks read, ↑ edits, Alt+click marks unread
- voice channels and DM / group DM calls: join from the sidebar or the call button, mute / deafen, voice activity detection, speaking rings, the "Voice Connected" panel
- video: camera, screen share (Go Live) and watching other people's cameras and streams, H264 via openh264
- DAVE end-to-end encryption (the `davey` crate) for audio and video, on top of the transport encryption

Voice is our own implementation of the voice gateway (v8) and RTP; opus is `opus-rs` (pure Rust, so the build needs no cmake), audio I/O is `cpal` over WASAPI.

## Tests

`python e2e/run.py` builds the release binary and a fake Discord (`examples/mockcord.rs`), then launches the real client twice (cold, then warm with cache) and drives it through egui's input pipeline: click server, click channel, type and send, scroll history, open DMs, friends, Ctrl+K, forum, thread, profile, search, pins, settings, then a voice channel (mute, camera, watching a stream, going live) and a DM call. The fake voice server does real DAVE: openmls plays the MLS external sender and a second `davey` member talks and streams, so the client's commit and welcome paths and its E2EE media are all checked against something independent. It checks timings and what the fake server received, and writes `e2e/artifacts/report.md`, `report.json`, screenshots and the request log.

Env overrides used by the harness: `RUSTCORD_API`, `RUSTCORD_CDN`, `RUSTCORD_EMOJI_CDN`, `RUSTCORD_GATEWAY`, `RUSTCORD_TOKEN`, `RUSTCORD_DATA`, `RUSTCORD_E2E`, and for media `RUSTCORD_FAKE_MIC`, `RUSTCORD_FAKE_CAM`, `RUSTCORD_FAKE_VIDEO` (synthetic sources instead of devices). `python e2e/run.py --real-camera` uses the real webcam on the warm run.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option. Unless you state otherwise, any contribution you submit for inclusion in this project is dual licensed as above, without any additional terms or conditions.

Bundled third-party files keep their own licenses: `assets/selfie_segmentation.onnx` is MediaPipe's model under Apache-2.0 (see `assets/selfie_segmentation.NOTICE`). Discord's names, logos and artwork belong to Discord Inc. and are not covered by this project's license.
