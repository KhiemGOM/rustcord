# Fastcord

A native Discord client in Rust (egui + glow). Same layout and colours as the desktop app, without Electron.

```
cargo run --release
```

Log in with email/password (2FA supported) or paste a token. The token and a snapshot of your servers, channels and recent messages live in `%LOCALAPPDATA%\Fastcord`, which is why the second launch paints your servers before the gateway has even connected.

## What works

- servers rail (unread pills, mention badges), channel list with categories, permissions, collapsed categories, voice occupants
- DMs and group DMs
- messages: grouping, replies, markdown (bold/italic/underline/strike/spoilers/code blocks/quotes/headers/lists), mentions, custom + unicode emoji (twemoji), timestamps, attachments, image/video/rich embeds, stickers, reactions
- send, reply, edit (↑ on an empty composer), delete, react, typing indicators, file upload (button or drag & drop)
- member list with role groups and presence
- read state / acks, history paging with scroll anchoring

Not yet: voice/video, threads, forum channels, settings UI, notifications.

## Tests

`python e2e/run.py` builds the release binary and a fake Discord (`examples/mockcord.rs`), then launches the real client twice (cold, then warm with cache) and drives it through egui's input pipeline: click server, click channel, type and send, scroll history, open DMs. It checks timings and what the fake server received, and writes `e2e/artifacts/report.md`, `report.json`, screenshots and the request log.

Env overrides used by the harness: `FASTCORD_API`, `FASTCORD_CDN`, `FASTCORD_EMOJI_CDN`, `FASTCORD_GATEWAY`, `FASTCORD_TOKEN`, `FASTCORD_DATA`, `FASTCORD_E2E`.
