"""End-to-end run: real rustcord binary against the mock Discord in examples/mockcord.rs.

Does a cold start (empty data dir) and a warm start (cache from the cold run),
drives the UI (click guild, click channel, type + send, scroll history, open DMs,
join a voice channel, start a DM call), then checks timings and what the mock
actually received. Both runs use a synthetic mic, camera and screen and keep the
speakers closed, so a test run never records you or makes noise.

    python e2e/run.py                  # builds release first
    python e2e/run.py --no-build
    python e2e/run.py --real-devices   # warm run uses your real mic, speakers and desktop
    python e2e/run.py --real-camera    # ...and your webcam
    python e2e/run.py --real-video     # warm run streams a real screen, mic and speakers stay fake

Artifacts land in e2e/artifacts/: report.json, report.md, screenshots, request log.
"""

import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from base64 import b64decode
from pathlib import Path

import auth
import look
import polish
import call
import discover
import motion
import home
import composer
import messages
import rail
import people
import players
import voice_settings
import rpc
import qol
import web
import sound
import activity
import clips

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "e2e" / "artifacts"
def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def free_both():
    """The voice port takes tcp and udp. Windows reserves udp ranges inside the ephemeral
    ports a tcp probe hands out, so try until one binds both ways."""
    while True:
        p = free_port()
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as u:
            try:
                u.bind(("127.0.0.1", p))
                return p
            except OSError:
                pass


# random ports so parallel runs (other worktrees, other agents) can't cross wires
HTTP, WS, VOICE, REMOTE = free_port(), free_port(), free_both(), free_port()
BUDGET_MS = 1000


def build():
    subprocess.run(["cargo", "build", "--release", "--bin", "rustcord", "--example", "mockcord"], cwd=ROOT, check=True)


def prime():
    """First launch of a freshly linked exe spends ~450ms in the OS (Defender scan, cold
    pages) before main runs. Pay that here so the timed cold start measures our code."""
    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ, RUSTCORD_DATA=tmp + "/data", RUSTCORD_E2E=tmp, RUSTCORD_FAKE_MIC="1", RUSTCORD_FAKE_CAM="1", RUSTCORD_SILENT="1")
        p = subprocess.Popen([str(ROOT / "target" / "release" / "rustcord.exe")], env=env)
        end = time.time() + 30
        while not (Path(tmp) / "first_frame_epoch_ms").exists() and time.time() < end:
            time.sleep(0.01)
        p.kill()
        p.wait()


def run_client(name, data_dir, extra=None):
    out = ART / name
    out.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.update({
        "RUSTCORD_API": f"http://127.0.0.1:{HTTP}/api/v9",
        "RUSTCORD_CDN": f"http://127.0.0.1:{HTTP}/cdn",
        "RUSTCORD_EMOJI_CDN": f"http://127.0.0.1:{HTTP}/cdn/emoji",
        "RUSTCORD_FONTS": f"http://127.0.0.1:{HTTP}",
        "RUSTCORD_WEB": f"http://127.0.0.1:{HTTP}",
        "RUSTCORD_SOUNDS": f"http://127.0.0.1:{HTTP}",
        "RUSTCORD_ACTIVITY": f"http://127.0.0.1:{HTTP}/activity/{{app}}",
        "RUSTCORD_GATEWAY": f"ws://127.0.0.1:{WS}",
        "RUSTCORD_REMOTE_AUTH": f"ws://127.0.0.1:{REMOTE}",
        "RUSTCORD_TOKEN": "mock-token",
        "RUSTCORD_DATA": str(data_dir),
        "RUSTCORD_E2E": str(out),
        "RUSTCORD_SPELL_WORDS": str(ROOT / "e2e" / "spell_words.txt"),
    })
    env.update(extra or {})
    env = {k: v for k, v in env.items() if v is not None}
    exe = ROOT / "target" / "release" / "rustcord.exe"
    spawned = time.time() * 1000
    p = subprocess.Popen([str(exe)], env=env)
    try:
        code = p.wait(timeout=1500)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
        code = "timeout"
    res = json.loads((out / "results.json").read_text(encoding="utf-8")) if (out / "results.json").exists() else {"ok": False, "error": "no results.json"}
    ff = out / "first_frame_epoch_ms"
    res["spawn_to_first_frame_ms"] = round(float(ff.read_text()) - spawned, 1) if ff.exists() else None
    res["exit_code"] = code
    return res


def main():
    if "--no-build" not in sys.argv:
        build()
    if ART.exists():
        shutil.rmtree(ART)
    ART.mkdir(parents=True)
    data = ART / "data"
    log = ART / "mock_requests.jsonl"

    prime()
    served = sound.make(ART / "sounds")
    mock = subprocess.Popen([str(ROOT / "target" / "release" / "examples" / "mockcord.exe"), str(HTTP), str(WS), str(log), str(VOICE), str(REMOTE)],
                            env=dict(os.environ, MOCK_SOUNDS=str(served or "")))
    time.sleep(0.5)
    try:
        # real mic, speakers, desktop and webcam are all opt-in
        fake = {"RUSTCORD_FAKE_MIC": "1", "RUSTCORD_FAKE_VIDEO": "1", "RUSTCORD_SILENT": "1"}
        cam = {} if "--real-camera" in sys.argv else {"RUSTCORD_FAKE_CAM": "1"}
        cold = run_client("cold", data, {**fake, **cam})
        # --real-video: the warm run captures a real screen for go live, mic and speakers stay fake
        real_video = {"RUSTCORD_FAKE_MIC": "1", "RUSTCORD_SILENT": "1", **cam}
        warm = run_client("warm", data, cam if "--real-devices" in sys.argv else real_video if "--real-video" in sys.argv else {**fake, **cam})
        signin = run_client("login", ART / "login_data", {"RUSTCORD_TOKEN": None, "RUSTCORD_E2E_LOGIN": "1", **fake})
    finally:
        mock.kill()

    reqs = [json.loads(l) for l in log.read_text(encoding="utf-8").splitlines() if l.strip()]
    sends = [r for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith("/channels/2011/messages")]
    def content(body):
        try:
            return json.loads(body).get("content", "")
        except ValueError:  # multipart upload
            m = re.search(r'"content":"((?:[^"\\]|\\.)*)"', body)
            return m.group(1) if m else ""

    def top_type(body):
        try:
            return json.loads(body[body.index("{"):body.rindex("}") + 1]).get("type")
        except ValueError:
            return None

    sent_bodies = [content(r["body"]) for r in sends]
    interactions = [r for r in reqs if r.get("path", "").endswith("/interactions")]
    uploads = [r for r in sends if 'filename="image.png"' in r.get("body", "")]
    acks = [r for r in reqs if "/ack" in r.get("path", "")]
    history = [r for r in reqs if "before=" in r.get("path", "")]
    subs = [r for r in reqs if r.get("gateway_op") == 37]
    vops = [r for r in reqs if "voice_op" in r]
    idents = [r["d"] for r in vops if r["voice_op"] == 0]
    selects = [r["d"] for r in vops if r["voice_op"] == 1]
    beats = [r for r in vops if r["voice_op"] == 3]
    speaking = [r.get("speaking") for r in vops if r["voice_op"] == 5]
    rtp = [r for r in reqs if "voice_rtp" in r and r.get("ssrc") == 1001]
    op4 = [r.get("d", {}) for r in reqs if r.get("gateway_op") == 4]
    voice_marks = ("voice_connected", "voice_heard", "voice_left", "dm_call", "dm_left")
    vbin = [r for r in reqs if "voice_bin" in r]
    vframes = [r for r in reqs if r.get("video_frame") and r.get("e2ee")]
    gw = lambda op: [r.get("d", {}) for r in reqs if r.get("gateway_op") == op]
    video_marks = ("video_in", "camera_on", "stream_in", "stream_out")
    e2ee = [r for r in rtp if r.get("e2ee")]
    op3 = gw(3)
    played = [i for i, d in enumerate(op3) if any(a.get("application_id") == "7100" for a in d.get("activities") or [])]

    lay = lambda r, k: r.get("rects", {}).get("layout_" + k, 0)
    def near(v, want, tol=0.6):
        return v is not None and abs(v - want) <= tol

    def side_ok(r):
        rc = r.get("rects", {})
        g = lambda k: rc.get(k) or [None] * 4
        pitch = (g("guildicon:1008")[1] or 0) - (g("guildicon:1007")[1] or 0)
        return (
            "side" in r.get("marks_ms", {})
            and all(near(g(f"guildicon:{i}")[2], 40) and near(g(f"guildicon:{i}")[3], 40) for i in (1003, 1007, 1008))
            and near(pitch, 48)
            and near(g("folderbg:78")[2], 48)
            # discord's open folder: a 48 header, the first server 4 under it, the background
            # 4 past the last one and 8 over whatever follows
            and near((g("guildicon:1003")[1] or 0) - (g("folderbg:78")[1] or 0), 52)
            and near((g("guildicon:1007")[1] or 0) - (g("folderbg:78")[1] or 0) - (g("folderbg:78")[3] or 0), 8)
            and near(g("voicebadge:1008")[2], 16)
            and near((g("guildicon:1008")[0] or 0) + 40 - (g("voicebadge:1008")[0] or 0), 16)
            and near(g("chan:2303")[3], 32)
            and near(g("chanicon:2303")[2], 20)
            # discord draws emoji in names at about the text size, not 1.25x of it
            and 14 <= (g("chanemoji:2303")[3] or 0) <= 18
            and near(g("userpanel")[3], 56, 8.5)
            and near(g("member:101")[3], 42)
            and g("voicebadge:1008")[2] is not None and g("voicebadge:1004")[2] is None
            # muting hides unreads; an "only @mentions" notification level doesn't (1006)
            and rc.get("side_unread_1007") is True and rc.get("side_unread_1006") is True
            and not any(rc.get(f"side_unread_{i}") for i in (1003, 1004, 1005, 1008))
            and rc.get("side_locked_2302") is True and rc.get("side_locked_2303") is False
            and rc.get("side_locked_2309") is True and rc.get("side_locked_2310") is False
        )

    checks = {
        "side: rail icons 40pt at a 48pt pitch, emoji at text size, unread pills where discord shows them (mutes hide them, notification levels don't)": side_ok(cold) and side_ok(warm),
        "cold run finished its script": cold.get("ok") is True,
        "warm run finished its script": warm.get("ok") is True,
        f"cold: spawn -> first frame < {BUDGET_MS}ms": (cold.get("spawn_to_first_frame_ms") or 1e9) < BUDGET_MS,
        f"warm: spawn -> first frame < {BUDGET_MS}ms": (warm.get("spawn_to_first_frame_ms") or 1e9) < BUDGET_MS,
        f"warm: cached servers on screen < {BUDGET_MS}ms": warm.get("marks_ms", {}).get("content", 1e9) < BUDGET_MS,
        "two messages typed through the composer reached the API": sum("hello from the e2e run" in b for b in sent_bodies) == 2,
        "@al + Tab became a real mention": sum("<@101> ping from autocomplete" in b for b in sent_bodies) == 2,
        "slash command and button click went out as interactions": [top_type(r["body"]) for r in interactions].count(2) == 2
            and [top_type(r["body"]) for r in interactions].count(3) == 2,
        "ctrl+v image was uploaded as a file": len(uploads) == 2,
        **{f"{name}: {what}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("mention autocomplete", "mention_ac"), ("slash command answered", "slash"),
                              ("bot button pressed", "component"), ("clipboard image attached", "paste"),
                              ("reply click jumped to the original", "jump"),
                              ("image opened in the in-app viewer", "viewer"),
                              ("unread dm shows as a bubble in the rail", "dm_bubble"), ("alt+arrow channel navigation", "alt_nav"),
                              ("gif picked from the picker was sent", "gif"), ("reply sent with ping off", "reply_quiet")]},
        "reply with @ OFF told discord not to ping": sum('"replied_user":false' in r.get("body", "") for r in sends) == 2,
        "shift+enter made a newline instead of sending": sum("quiet reply\nsecond line" in b for b in sent_bodies) == 2,
        **{f"settings: {name}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("every page opened and was captured", "settings_tour"), ("profile edit saved via the unsaved-changes bar", "profile_saved"),
                              ("sidebar sub-section scrolled its section into view", "settings_scrolled_to"), ("light theme applied", "theme_light"), ("compact mode applied", "compact"), ("streamer mode toggled", "streamer"),
                              ("username changed through the modal with the password", "display_name"),
                              ("authenticator enabled through a sudo ticket", "mfa_on"), ("device logged out once the second factor was given", "sessions_logout"),
                              ("message requests and spam filter written to the settings blob", "messaging_privacy"), ("activity sharing switched off for every server", "activity_servers"),
                              ("who can join my games written", "activity_join"), ("authorized app and connection removed", "connected_apps"),
                              ("avatar decoration picked and saved", "profile_decoration"), ("disable account says you own servers", "owner_modal"),
                              ("hardware acceleration switched off", "hw_accel_off"), ("notifications, system, language and developer pages were driven", "system_pages"),
                              ("settings search found a control by its label", "settings_search"), ("a keybind was added", "keybind_added")]},
        "settings: profile pronouns PATCHed": sum('"pronouns":"they/them"' in r.get("body", "") for r in reqs if r.get("path", "").endswith("/users/@me/profile")) == 2,
        "settings: privacy switches PATCHed to the settings blob": sum(1 for r in reqs if r.get("method") == "PATCH" and r.get("path", "").endswith("/users/@me/settings-proto/1")) >= 2 * 4,
        "settings: language switch reached the locale header (hypesquad join sent as de)": sum(1 for r in reqs if r.get("path", "").endswith("/hypesquad/online") and r.get("locale") == "de" and '"house_id":1' in r.get("body", "")) == 2,
        "settings: username PATCHed with the password": sum('"username":"you!"' in r.get("body", "") and '"password":"hunter2"' in r.get("body", "") for r in reqs) == 2,
        "settings: sudo ticket finished and the token sent on the retry": sum(1 for r in reqs if r.get("path", "").endswith("/mfa/finish")) == 4,
        "settings: data request, connection sign-in, app and connection removal, integration join reached discord": all(
            sum(1 for r in reqs if r.get("method") == m and p in r.get("path", "")) == 2
            for m, p in [("POST", "/users/@me/harvest"), ("GET", "/connections/github/authorize"), ("DELETE", "/oauth2/tokens/"), ("DELETE", "/users/@me/connections/github/1"), ("POST", "/integrations/7001/join")]),
        "settings: decoration sku sent with the profile": sum('"avatar_decoration_sku_id":"7001"' in r.get("body", "") for r in reqs) == 2,
        **{f"billing: {name}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("server boost page showed the boosts, perks and faq", "boost_page"), ("boost any server opened the server picker", "boost_picker"),
                              ("gift code redeemed natively", "gift_redeemed"), ("payment method edit modal opened", "billing_edit"),
                              ("payment method made default, or delete asked first", "billing_manage"), ("transactions paged", "billing_tx")]},
        "billing: gift code redeem POSTed": sum(r.get("method") == "POST" and r.get("path", "").endswith("/entitlements/gift-codes/WUMP-AAAAA-BBBBB-CCCCC/redeem") for r in reqs) == 2,
        "billing: default payment method PATCHed": any(r.get("method") == "PATCH" and "/billing/payment-sources/" in r.get("path", "") and '"default":true' in r.get("body", "") for r in reqs),
        "billing: nothing was bought or deleted": not any(
            (r.get("method") != "GET" and "/billing/" in r.get("path", "") and "/payment-sources/" not in r.get("path", "")) or (r.get("method") == "DELETE" and "/payment-sources/" in r.get("path", ""))
            or any(k in r.get("path", "") for k in ("/billing/premium", "guild-subscriptions/purchase", "/purchase", "/subscribe")) for r in reqs),
        **{f"settings: {name} PATCHed to the account's settings proto": sum(b64decode(json.loads(r["body"])["settings"]) == want for r in reqs
                                                                            if r.get("method") == "PATCH" and "/settings-proto/" in r.get("path", "") and r.get("body")) == 2
           for name, want in [("friends online off", bytes([0x3a, 2, 0x62, 0])), ("tts command off", bytes([0x32, 3, 0x82, 1, 0])),
                              ("24-hour clock", bytes([0x6a, 2, 0x48, 2])), ("developer mode", bytes([0x6a, 2, 0x10, 1]))]},
        "settings: email category PATCHed natively": sum('"social":false' in r.get("body", "") for r in reqs if r.get("path", "").endswith("/users/@me/email-settings") and r.get("method") == "PATCH") == 2,
        "settings: devices, apps and connections were fetched": all(any(r.get("path", "").endswith(p) for r in reqs) for p in ("/auth/sessions", "/oauth2/tokens", "/users/@me/connections")),
        **{f"media: {name}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("animated emoji, mp4 gifv, gif upload and ?query gif each cycled through 3+ distinct frames", "media_animates"),
                              ("rendered pixels of the gifv and the gif upload differ between two screenshots 150ms apart", "media_pixels_change"),
                              ("animations hold still while the window is unfocused", "media_pauses_unfocused"),
                              ("animations pick up again when focus returns", "media_resumes_focused"),
                              ("gif autoplay off holds gifs and clips on frame 0", "media_autoplay_off"),
                              ("hovering a gif with autoplay off plays it", "media_hover_plays")]},
        "media: gifv titled by its provider drew as bare 160x120 media, not a card": all(
            (r.get("rects", {}).get("media_gifv") or [0, 0, 0, 0])[2:] == [160, 120] for r in (cold, warm)),
        **{f"title bar: {name}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("drawn with back, forward, inbox, help and the window buttons", "title_bar"),
                              ("back and forward buttons walked the history", "nav_buttons"),
                              ("alt+left and the mouse side button walked it too", "nav_keys"),
                              ("the friends page is a history entry", "nav_friends"),
                              ("inbox listed the mentions", "inbox_listed"), ("clicking a mention jumped to it", "inbox_jump"),
                              ("double click on the bar maximized the window", "maximize"), ("the maximize button restored it", "restore")]},
        "title bar: 32pt tall like discord's top bar, flush with the top of the window": all(
            r.get("rects", {}).get("title_bar", [0, 1, 0, 0])[1] == 0 and abs(r.get("rects", {}).get("title_bar", [0, 0, 0, 0])[3] - 32) < 0.5 for r in (cold, warm)),
        "title bar: every edge and corner answers WM_NCHITTEST with its resize zone": all(
            r.get("rects", {}).get("edge_hits") == [10, 11, 12, 15, 13, 14, 16, 17, 1] for r in (cold, warm)),
        "title bar: inbox fetched /users/@me/mentions": sum("/users/@me/mentions?limit=25&roles=true&everyone=true" in r.get("path", "") for r in reqs) >= 2,
        **{f"emoji: {what}": sent_bodies.count(body) == 2
           for what, body in [("a locked emoji picked from the popup stays literal text", ":omgParty: locked"),
                              ("typed :ferris: became the current server's emoji without nitro", "<:ferris:5000> static"),
                              ("typed :smile: became the glyph on the closing colon", "\U0001F604 hi"),
                              ("unknown names and code spans stayed as typed", ":nosuchemoji: `:ferris:` `:smile:`"),
                              ("another server's emoji stayed literal without nitro", ":omgBruh: far"),
                              ("nitro: animated emoji picked from the popup", "<a:omgParty:5101> anim"),
                              ("nitro: other server's emoji picked from the popup", "<:omgBruh:5200> picked"),
                              ("nitro: other server's emoji typed in full", "<:omgBruh:5200> typed"),
                              ("role-locked and unavailable emojis stayed literal even with nitro", ":omgMods: :omgLapsed: no")]},
        "emoji: popup opened for both the locked and the nitro run": all(m in r.get("marks_ms", {}) for r in (cold, warm) for m in ("emoji_locked_ac", "emoji")),
        "menus: channel, server dropdown, search filters and person menu all opened, and Server Boost opened its page": all("menus" in r.get("marks_ms", {}) for r in (cold, warm)),
        "person menu: a voice user's menu had volume, mute and soundboard rows, and their volume reached the mixer": all("person_menu" in r.get("marks_ms", {}) for r in (cold, warm)),
        "friend menu: right-clicking a friend gave the whole menu": all("friend_menu" in r.get("marks_ms", {}) for r in (cold, warm)),
        "pinned dm: Pin sent the dm's override flags": all("pin_dm" in r.get("marks_ms", {}) for r in (cold, warm)) and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/users/@me/guilds/@me/settings") and "2048" in r.get("body", "") for r in reqs),
        "accounts: the signed-in account is in the Switch Accounts list, and a custom status went to the server": all("accounts" in r.get("marks_ms", {}) for r in (cold, warm)) and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/users/@me/settings") and "mock status" in r.get("body", "") for r in reqs),
        "quick reactions: right-clicking one in the message menu and picking an emoji replaced it": all("quick_reactions" in r.get("marks_ms", {}) for r in (cold, warm)),
        "new messages bar: a wheel down at the bottom of the channel takes it away": all("bar_read" in r.get("marks_ms", {}) for r in (cold, warm)),
        "autoscroll: a middle click then moving up scrolled the chat up, a click ended it": all("autoscroll" in r.get("marks_ms", {}) for r in (cold, warm)),
        "fullscreen: F11 over a maximised window filled the monitor and came back maximised": all("fullscreen" in r.get("marks_ms", {}) for r in (cold, warm)),
        "zoom: a ctrl+wheel notch went up one level (100% to 110%), ctrl+0 came back": all("zoom" in r.get("marks_ms", {}) for r in (cold, warm)),
        "name fonts: a nitro display name font was fetched and the user's name uses it": all("name_font" in r.get("marks_ms", {}) for r in (cold, warm)),
        "profile: the avatar on a server profile opens the main one, and its \"...\" opens the menu with the profile still up": all("profile_more" in r.get("marks_ms", {}) for r in (cold, warm)),
        "opt-in: an onboarding server lists only picked channels, Add to Channel List brings one back": all("opt_in" in r.get("marks_ms", {}) for r in (cold, warm))
            and sum('"flags":4096' in r.get("body", "") and '"2101"' in r.get("body", "") for r in reqs if r.get("path", "").endswith("/users/@me/guilds/1001/settings")) >= 2,
        "poll: rendered in #general, an answer picked and Vote pressed": all("home_poll" in r.get("marks_ms", {}) for r in (cold, warm)),
        "poll: the vote was PUT to discord's poll answers endpoint": any(
            r.get("method") == "PUT" and "/polls/" in r.get("path", "") and r.get("path", "").endswith("/answers/@me") for r in reqs),
        "soundboard: opened in the guild call and the server's sounds loaded": all("soundboard" in r.get("marks_ms", {}) for r in (cold, warm))
            and any("/soundboard-sounds" in r.get("path", "") for r in reqs),
        "soundboard: clicking a sound fetched it to play": any("/soundboard-sounds/5001" in r.get("path", "") for r in reqs),
        "older history was paged in via ?before=": len(history) >= 2,
        "read state was acked": len(acks) >= 1,
        "member list subscription (op 37) was sent": len(subs) >= 1,
        "message authors were requested via op 8": any(r.get("gateway_op") == 8 for r in reqs),
        **{f"{name}: {what}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("quick switcher (ctrl+k) jumped to #help", "switcher"), ("forum posts loaded", "forum"),
                              ("thread opened from sidebar", "thread"), ("profile popout loaded", "profile"),
                              ("search returned results", "search"), ("pins loaded", "pins"), ("settings opened", "settings"),
                              ("clicking outside settings closes it", "settings_backdrop"),
                              ("dm from someone else raised a notification", "notified"),
                              ("a muted dm's message neither pinged nor popped up in the rail", "muted_dm"),
                              ("toast: a dm's ping is the sender's name over the text, with their avatar, silent", "toast"),
                              ("toast: clicking one opens the dm at that message", "toast_click"),
                              ("taskbar: discord's red badge shows the mention count", "taskbar_badge"),
                              ("minimized: a dm still raised its toast and moved the taskbar badge and the title's count", "minimized_ping"),
                              ("tray: with something waiting the icon is discord's unread face", "tray_unread"),
                              ("tray: in a call the icon is discord's voice face", "tray_voice"),
                              ("tray: closing the window with Minimize to Tray on puts it in the tray instead of quitting", "tray_close"),
                              ("pins: something pinned since you looked puts discord's dot on the pin button", "pins_dot"),
                              ("pins: opening them takes the dot away", "pins_dot_gone"),
                              ("activity: the member list leads with three cards from this server's people", "activity"),
                              ("member list: a gradient role's name is drawn as its gradient", "member_grad"),
                              ("boosts: Server Boost opens the boosts page in the app, levels and perks from the catalogue", "boosts"),
                              ("create channel: typed into discord's modal, sent, and the new channel opened", "create_channel"),
                              ("active threads: the modal lists the server's threads and a card opens one", "active_threads"),
                              ("app directory: discover opens in the app with the featured collections", "directory"),
                              ("server settings: the layer opens on Roles with discord's member counts", "server_settings"),
                              ("server settings: the role editor, members, emoji, stickers, invites, integrations, safety, audit log, bans and AutoMod each drew with their data", "server_pages"),
                              ("events: the window lists the server's event and Interested marks it", "events"),
                              ("threads: Open in Split View's twin, Shift on the channel list, puts the thread beside its parent", "thread_split"),
                              ("threads: the member list asks for the thread's members and shows them", "thread_members"),
                              ("threads: Join Thread from the header's menu joined it", "thread_join"),
                              ("threads: Close Thread archived it", "thread_close"),
                              ("threads: Lock Thread locked it", "thread_lock"),
                              ("events: a card opens Event Details with who is interested", "event_detail"),
                              ("events: Start Event asks first and starts it", "event_start"),
                              ("events: Cancel Event asks first and takes it off the list", "event_cancel"),
                              ("events: Edit Event fills the wizard from the event", "event_edit"),
                              ("events: Mute New Events from the Events row's menu", "events_mute"),
                              ("announcements: Follow opens the dialog and sends the follow", "follow"),
                              ("quests: the hub lists the quests with the Orbs balance", "quests"),
                              ("quests: Accept Quest enrolled one", "quest_accept"),
                              ("quests: Claim Reward claimed the finished one", "quest_claim"),
                              ("escape cancels a reply even with the composer unfocused", "reply_escape"),
                              ("game: the running mockcord.exe got discord's panel over the account card", "game_panel"),
                              ("game: with activity sharing off the panel says Not Sharing", "game_hidden"),
                              ("invite: discord's dialog lists dms and friends, and Invite on a friend opened a dm", "invite_friends"),
                              ("voice: the call's timer and status prompt, another channel's status, a tile per person", "voice_channel_info"),
                              ("go live: the share button opens discord's picker and a screen from it goes live", "share_picker"),
                              ("rail: a server's tooltip shows who's in voice beside a speaker", "guild_tip_voice"),
                              ("do not disturb: a dm arriving raised no notification", "dnd_quiet"),
                              ("voice: Voice Connected opens the connection popout with pings graphed", "rtc_popout"),
                              ("camera settings: Test Video shows the live camera in its box", "camera_test_video"),
                              ("camera settings: Blur went behind the person in the preview", "camera_background"),
                              ("camera settings: Always preview video switched on", "camera_always_preview"),
                              ("camera settings: a blur picked with the camera off loaded no model and ran no thread", "camera_idle_no_load"),
                              ("camera settings: leaving the page stopped the preview's camera", "camera_preview_stops"),
                              ("camera settings: a custom upload showed in the preview", "camera_bg_custom"),
                              ("camera settings: a preset fetched from the cdn showed in the preview", "camera_bg_preset"),
                              ("camera: with it on, the camera button opens the preview and Turn On Camera starts it", "camera_preview_modal"),
                              ("camera: the device chosen in settings is the one the call opened", "camera_device"),
                              ("camera: the background went into the stream that was sent", "camera_stream_bg"),
                              ("camera: the call bar's menu switched the live camera to another device", "camera_callbar_menu")]},
        "voice: channel info was asked for with op 43": any(r.get("gateway_op") == 43 for r in reqs),
        "invite: the link was made for 30 days and sent to the friend's new dm": any(
            r.get("method") == "POST" and r.get("path", "").endswith("/channels/2400/invites") and '"max_age":2592000' in r.get("body", "") for r in reqs)
            and any(r.get("method") == "POST" and r.get("path", "").endswith("/channels/3010/messages") and "discord.gg/e2eInv1" in r.get("body", "") for r in reqs),
        "game: the gateway heard the game while shared and an empty activity list once sharing was off": bool(played)
            and op3[played[0]]["activities"][0].get("name") == "Mockcord Quest"
            and op3[played[0]]["activities"][0].get("type") == 0
            and any(not d.get("activities") for d in op3[played[0] + 1:]),
        "events: Interested was sent to discord": any(r.get("method") == "PUT" and r.get("path", "").endswith("/guilds/1000/scheduled-events/6100/users/@me") for r in reqs),
        "server settings: editing the profile raised the unsaved bar, and Save sent the name to the profile": all(m in r.get("marks_ms", {}) for r in (cold, warm) for m in ("server_bar", "server_profile_saved"))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001/profile") and ' X"' in r.get("body", "") for r in reqs),
        "server settings: Server Tag saved the tag and badge to the profile": all("server_tag" in r.get("marks_ms", {}) for r in (cold, warm))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001/profile") and '"tag":"RUST"' in r.get("body", "") and '"badge":0' in r.get("body", "") for r in reqs),
        "server settings: Engagement saved the system message flags": all("server_engagement" in r.get("marks_ms", {}) for r in (cold, warm))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001") and '"system_channel_flags":4' in r.get("body", "") for r in reqs),
        "server settings: Boost Perks toggled the progress bar": all("server_perks" in r.get("marks_ms", {}) for r in (cold, warm))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001") and "premium_progress_bar_enabled" in r.get("body", "") for r in reqs),
        "server settings: Soundboard renamed a sound through its edit modal": all("server_soundboard" in r.get("marks_ms", {}) for r in (cold, warm))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001/soundboard-sounds/5001") and "airhorn2" in r.get("body", "") for r in reqs),
        "server settings: Access sent the join questions to the membership screening form": all("server_access" in r.get("marks_ms", {}) for r in (cold, warm))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001/member-verification") and "Why do you want to join?" in r.get("body", "") and '"enabled":true' in r.get("body", "") for r in reqs),
        "server settings: Enable Community walked its three steps and sent the community features": all("server_community" in r.get("marks_ms", {}) for r in (cold, warm))
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1002") and '"COMMUNITY"' in r.get("body", "") and '"explicit_content_filter":2' in r.get("body", "") for r in reqs),
        "server settings: Server Template generated a template, and Delete Server asked first": all(m in r.get("marks_ms", {}) for r in (cold, warm) for m in ("server_template", "server_delete"))
            and any(r.get("method") == "POST" and r.get("path", "").endswith("/guilds/1002/templates") and "Design fans" in r.get("body", "") for r in reqs),
        "threads: Join Thread was sent": any(r.get("method") == "PUT" and r.get("path", "").endswith("/channels/2160/thread-members/@me") for r in reqs),
        "threads: Close and Lock Thread were patched": any(r.get("method") == "PATCH" and r.get("path", "").endswith("/channels/2160") and '"archived":true' in r.get("body", "") for r in reqs)
            and any(r.get("method") == "PATCH" and r.get("path", "").endswith("/channels/2160") and '"locked":true' in r.get("body", "") for r in reqs),
        "threads: the thread's members were asked for over the gateway": any(d.get("subscriptions", {}).get("1001", {}).get("thread_member_lists") == ["2160"] for d in gw(37)),
        "events: opening the window read the event notifications": any(r.get("method") == "POST" and r.get("path", "").endswith("/read-states/ack-bulk") and '"read_state_type":1' in r.get("body", "") for r in reqs),
        "events: the interested users were fetched": any("/guilds/1001/scheduled-events/6200/users" in r.get("path", "") for r in reqs),
        "events: start and cancel went to discord": any(r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1001/scheduled-events/6201") and '"status":2' in r.get("body", "") for r in reqs)
            and any(r.get("method") == "DELETE" and r.get("path", "").endswith("/guilds/1001/scheduled-events/6200") for r in reqs),
        "events: muting new events was saved to the server's settings": any(r.get("method") == "PATCH" and r.get("path", "").endswith("/users/@me/guilds/1001/settings") and '"mute_scheduled_events":true' in r.get("body", "") for r in reqs),
        "announcements: the follow went to the channel's followers with the picked channel": any(r.get("method") == "POST" and r.get("path", "").endswith("/channels/2002/followers") and '"webhook_channel_id":"2100"' in r.get("body", "") for r in reqs),
        "announcements: Publish crossposted the message": any(r.get("method") == "POST" and r.get("path", "").endswith("/channels/2002/messages/1/crosspost") for r in reqs),
        "quests: accept and claim went to discord": any(r.get("method") == "POST" and r.get("path", "").endswith("/quests/9002/enroll") for r in reqs)
            and any(r.get("method") == "POST" and r.get("path", "").endswith("/quests/9003/claim-reward") for r in reqs),
        "create channel: posted the lowercased name to the server": any(
            r.get("method") == "POST" and r.get("path", "").endswith("/guilds/1000/channels") and '"e2e-room"' in r.get("body", "") for r in reqs),
        "boosts: the page fetched the powerups catalogue, what's applied and the server's boosts": all(
            any(k in r.get("path", "") for r in reqs) for k in ("/store/published-listings/skus", "/guilds/1000/powerups", "/guilds/1000/premium/subscriptions")),
        "server settings: the role editor's save sent only the changed name, hoist and permissions": any(
            r.get("method") == "PATCH" and r.get("path", "").endswith("/guilds/1000/roles/1100") and '"name":"Admins"' in r.get("body", "")
            and '"hoist"' in r.get("body", "") and '"permissions"' in r.get("body", "") and '"mentionable"' not in r.get("body", "") for r in reqs),
        "server settings: the pages fetched the role's members, emoji, stickers, invites, integrations, webhooks, bans, audit log and AutoMod rules": all(
            any(r.get("method") == "GET" and r.get("path", "").endswith(p) for r in reqs)
            for p in ("/roles/1100/member-ids", "/guilds/1000/emojis", "/guilds/1000/stickers", "/guilds/1000/invites", "/guilds/1000/webhooks",
                      "/guilds/1000/bans?limit=1000", "/guilds/1000/auto-moderation/rules")) and any("/guilds/1000/audit-logs" in r.get("path", "") for r in reqs)
            and any("/guilds/1000/integrations" in r.get("path", "") for r in reqs),
        "pins: opening them acked them": any(r.get("method") == "POST" and r.get("path", "").endswith("/channels/2050/pins/ack") for r in reqs),
        "activity: the feed and its apps' icons were fetched": any("/content-inventory/users/@me" in r.get("path", "") for r in reqs)
            and any("/applications/public" in r.get("path", "") for r in reqs),
        "voice: op 4 joined the guild voice channel": any(d.get("guild_id") == "1000" and d.get("channel_id") == "2021" for d in op4),
        "voice: identify carried session, token and max_dave_protocol_version": len(idents) >= 4 and all(
            i.get("session_id") == "mock-voice-session" and (i.get("max_dave_protocol_version") or 0) >= 1
            and i.get("token") == ("mock-stream-token" if i.get("server_id") in ("5000", "5001") else "mock-voice-token") for i in idents),
        "voice: dm call identified with the dm channel as server_id": any(i.get("server_id") == "3000" for i in idents),
        "voice: select protocol picked aead_aes256_gcm_rtpsize with the discovered address": len(selects) >= 4 and all(
            s["data"]["mode"] == "aead_aes256_gcm_rtpsize" and s["data"]["address"] == "127.0.0.1" and s["data"]["port"] > 0 for s in selects),
        "voice: heartbeats carry seq_ack (v8)": len(beats) >= 2 and all(isinstance(b.get("seq_ack"), int) and b["seq_ack"] >= 1 for b in beats),
        "voice: client sent speaking (op 5), and speaking=1 from the fake mic": 0 in speaking and 1 in speaking,
        "voice: every client rtp packet decrypted with the session key": len(rtp) >= 20 and all(r["decrypted"] for r in rtp),
        "voice: fake mic audio arrived as voiced opus": sum(1 for r in rtp if (r.get("level") or 0) > 0.05) >= 20,
        "dave: client sent a key package (op 26) on every session": sum(1 for r in vbin if r["voice_bin"] == 26) >= 4,
        "dave: guild call - client committed the external add proposal, the other member joined from its welcome":
            sum(1 for r in vbin if r.get("alice_welcomed") is True) >= 2,
        "dave: dm call - client joined the group from the other member's welcome": sum(1 for r in vbin if r.get("alice_committed") is True) >= 2,
        "dave: client acked every transition (op 23), never flagged a bad commit (op 31)":
            sum(1 for r in vops if r["voice_op"] == 23 and r.get("alice_ready")) >= 4 and not any(r["voice_op"] == 31 for r in vops),
        "dave: client mic audio arrived end-to-end encrypted and decrypted by the other member":
            sum(1 for r in e2ee if (r.get("level") or 0) > 0.05) >= 20,
        "dave: client decrypted the other member's E2EE audio (both runs)": all(r.get("voice", {}).get("e2ee_in", 0) > 20 for r in (cold, warm)),
        "voice: client decoded the fake speaker and lit their speaking ring (both runs)": all(
            all(m in r.get("marks_ms", {}) for m in voice_marks) and r.get("voice", {}).get("frames_decoded", 0) > 20 for r in (cold, warm)),
        "voice: mute and deafen state reached the gateway": any(d.get("self_mute") is True and d.get("channel_id") for d in op4),
        "voice: disconnect sent op 4 with channel_id null": sum(1 for d in op4 if d.get("channel_id") is None) >= 4,
        "voice: dm call joined with guild_id null and rang the recipient": any(d.get("guild_id") is None and d.get("channel_id") == "3000" for d in op4)
            and any(r.get("path", "").endswith("/channels/3000/call/ring") for r in reqs),
        "video: client announced its camera (op 12 with an active stream) and asked for video (op 15)":
            any(r["voice_op"] == 12 and r["d"].get("video_ssrc") and r["d"]["streams"][0]["active"] for r in vops)
            and any(r["voice_op"] == 15 for r in vops) and any(d.get("self_video") is True for d in op4),
        "video: client camera arrived as E2EE H264 and decoded in the mock": sum(1 for r in vframes if r["server"] == "voice") >= 10,
        "video: client decoded the other member's camera, watched their stream, went live (both runs)":
            all(all(m in r.get("marks_ms", {}) for m in video_marks) and r.get("voice", {}).get("video_frames_in", 0) > 10 for r in (cold, warm)),
        "go live: STREAM_CREATE + SET_PAUSED, own stream connection on the rtc server, STREAM_DELETE after":
            bool(gw(18)) and any(d.get("paused") is False for d in gw(22)) and len(gw(19)) >= 2
            and any(i.get("server_id") == "5000" for i in idents),
        "go live: client screen arrived as E2EE H264 and decoded in the mock (warm run = real desktop with --real-devices)":
            sum(1 for r in vframes if r["server"] == "own_stream") >= 10,
        "watch: STREAM_WATCH for the other member's stream, separate connection with its own DAVE group":
            bool(gw(20)) and any(i.get("server_id") == "5001" for i in idents)
            and sum(1 for r in vbin if r.get("server") == "watch_stream" and r.get("alice_welcomed")) >= 2,
        "rtcp: client asked for keyframes with PLI": any(r.get("rtcp") == 206 and r.get("fmt") == 1 for r in reqs),
        "layout: dragging the channel list edge narrowed it by ~50 and the chat column reflowed (both runs)":
            all(lay(r, "side_start") - lay(r, "side_dragged") > 40 and lay(r, "chat_after_side") > lay(r, "chat_start") + 40 for r in (cold, warm)),
        "layout: dragging the member list edge left widened it and the chat column reflowed again (both runs)":
            all(lay(r, "members_dragged") - lay(r, "members_start") > 30 and lay(r, "chat_after_members") < lay(r, "chat_after_side") - 30 for r in (cold, warm)),
        "layout: dragging the channel list far past its limit stops at discord's 359 and leaves the chat column >= 400 (both runs)":
            all(lay(r, "side_max") <= 359.5 and lay(r, "chat_min") >= 400 for r in (cold, warm)),
        "layout: warm run starts with the panel widths the cold run left":
            abs(lay(warm, "side_start") - lay(cold, "side_end")) < 1 and abs(lay(warm, "members_start") - lay(cold, "members_dragged")) < 1,
        "frame cpu p99 < 8ms (cold)": cold.get("frame_cpu_ms", {}).get("p99", 1e9) < 8.0,
        "frame cpu p99 < 8ms (warm)": warm.get("frame_cpu_ms", {}).get("p99", 1e9) < 8.0,
        **{f"account sync: {name}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("READY's proto set the account's compact avatars at login", "sync_login"),
                              ("USER_SETTINGS_PROTO_UPDATE switched the theme to Onyx live", "sync_live_theme"),
                              ("an unchanged account value left the local edit alone", "sync_local_kept"),
                              ("a local theme pick survived the account repeating its value", "sync_override_kept"),
                              ("a second update switched back to Ash", "sync_back")]},
        "account sync: Onyx update painted the rail #000000 (both runs)": all(
            r.get("rects", {}).get("look:sync_onyx:rail") == "#000000" for r in (cold, warm)),
        **look.checks({"cold": cold, "warm": warm}, reqs, data),
        **polish.checks({"cold": cold, "warm": warm}, reqs, data),
        **qol.checks({"cold": cold, "warm": warm}, reqs, data),
        **composer.checks({"cold": cold, "warm": warm}, reqs, data),
        **people.checks({"cold": cold, "warm": warm}, reqs, data),
        **home.checks({"cold": cold, "warm": warm}, reqs, data),
        **web.checks({"cold": cold, "warm": warm}, reqs, data),
        **clips.checks({"cold": cold, "warm": warm}, reqs, data),
        **call.checks({"cold": cold, "warm": warm}, reqs, data),
        **messages.checks({"cold": cold, "warm": warm}, reqs, data),
        **rail.checks({"cold": cold, "warm": warm}, reqs, data),
        **discover.checks({"cold": cold, "warm": warm}, reqs, data),
        **motion.checks({"cold": cold, "warm": warm}, reqs, data),
        **auth.checks(signin, reqs),
        **players.checks({"cold": cold, "warm": warm}, reqs, data),
        **sound.checks({"cold": cold, "warm": warm}, reqs, data, served),
        **voice_settings.checks({"cold": cold, "warm": warm}, reqs, data),
        **rpc.checks({"cold": cold, "warm": warm}, reqs, data),
        **activity.checks({"cold": cold, "warm": warm}, reqs, data),
    }
    report = {"checks": checks, "cold": cold, "warm": warm, "login": signin, "requests_seen": len(reqs), "sent_bodies": sent_bodies}
    (ART / "report.json").write_text(json.dumps(report, indent=2))

    md = ["# rustcord e2e report", ""]
    md += [f"- [{'x' if ok else ' '}] {name}" for name, ok in checks.items()]
    for name, r in (("cold", cold), ("warm", warm), ("login", signin)):
        md += ["", f"## {name} start", "", f"- spawn -> first frame: {r.get('spawn_to_first_frame_ms')} ms"]
        md += [f"- {k}: {v} ms" for k, v in r.get("marks_ms", {}).items()]
        f = r.get("frame_cpu_ms", {})
        md += [f"- frame cpu: p50 {f.get('p50', 0):.2f} / p95 {f.get('p95', 0):.2f} / p99 {f.get('p99', 0):.2f} / max {f.get('max', 0):.2f} ms over {f.get('count')} frames"]
        md += [f"- voice: {json.dumps(r.get('voice'))}"]
        md += [f"- screenshot: {name}/{s}" for s in r.get("screenshots", [])]
        if r.get("error"):
            md += [f"- error: {r['error']}"]
    (ART / "report.md").write_text("\n".join(md) + "\n", encoding="utf-8")
    print("\n".join(md))
    sys.exit(0 if all(checks.values()) else 1)


if __name__ == "__main__":
    main()
