"""Checks for the chat cards and popouts in the mock's chat-extras channel, driven by chatx_steps
in src/ui/e2e/chatx.rs: what each drew, and what it asked the mock for."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    posts = [body(r) for r in sent("POST", "/channels/2085/messages")]
    asked = [body(r) for r in sent("POST", "/channels/2085/attachments")]
    big = [p for p in posts if p.get("content") == "big upload"]
    return {
        "chat: invite links drew a card each, valid, expired and one for a server you're in": both("chatx_invite_cards")
            and len(sent("GET", "/invites/e2eChat1?with_counts=true")) >= 2 and len(sent("GET", "/invites/e2eGone1")) >= 2,
        "chat: Join on an invite card joined, and the card then said Joined and opened the server": both("chatx_invite_join")
            and both("chatx_invite_joined_opens") and len(sent("POST", "/invites/e2eChat1")) >= 2,
        "chat: runs from blocked and ignored people folded into one row, Show opened them and Hide folded them again": both("chatx_blocked_collapsed")
            and both("chatx_blocked_shown") and both("chatx_blocked_folded_again"),
        "chat: a closed poll's votes button opened View Votes with the voters, fetched per answer": both("chatx_poll_voters") and both("chatx_poll_voters_tab")
            and len(sent("GET", "/channels/2085/polls/")) >= 4,
        "chat: Copy Image put the picture's pixels on the clipboard": both("chatx_copy_image"),
        "chat: a sticker opened its popout with the server it's from, asked /stickers/{id}/guild, and Join Server joined": both("chatx_sticker_popout")
            and len(sent("GET", "/stickers/7777/guild")) >= 2 and len(sent("PUT", "/guilds/3000/members/@me")) >= 2,
        "chat: a custom emoji from another server opened its popout, asked /emojis/{id}/source": both("chatx_emoji_popout")
            and len(sent("GET", "/emojis/9999/source")) >= 2,
        "chat: a @silent message showed its bell": both("chatx_silent"),
        "chat: Create Reminder > Custom... took a time in words and saved it with a due time": both("chatx_remind_custom")
            and any("due_at" in body(r) for r in sent("PUT", "/users/@me/saved-messages/")),
        "chat: the picker's Top Emoji came from /guilds/{id}/top-emojis in rank order": both("chatx_top_emojis")
            and len(sent("GET", "/guilds/1000/top-emojis")) >= 2,
        "forum: the post's author wore the Original Poster chip in their own post": both("chatx_op_chip"),
        "upload: a file past the account's limit got \"Your files are too powerful\" and never reached the composer": both("chatx_too_powerful")
            and not any(f.get("file_size") == 12 << 20 for a in asked for f in a.get("files", [])),
        "upload: the card showed real progress and Cancel stopped it before any message was sent": both("chatx_upload_progress")
            and both("chatx_upload_cancelled") and not any(p.get("content") == "slow upload" for p in posts),
        "upload: a 3 MB file went POST /attachments, PUT to the upload url, then a message with uploaded_filename": both("chatx_upload_done")
            and any(f.get("file_size") == 3 << 20 for a in asked for f in a.get("files", []))
            and len(big) >= 2 and all(a.get("uploaded_filename") for p in big for a in p.get("attachments", []))
            and len([r for r in reqs if r.get("method") == "PUT" and "big-3mb.bin" in r.get("path", "") and r.get("body") == str(3 << 20) + " bytes"]) >= 2,
    }
