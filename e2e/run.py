"""End-to-end run: real fastcord binary against the mock Discord in examples/mockcord.rs.

Does a cold start (empty data dir) and a warm start (cache from the cold run),
drives the UI (click guild, click channel, type + send, scroll history, open DMs,
join a voice channel, start a DM call), then checks timings and what the mock
actually received. Both runs use a synthetic mic, camera and screen and keep the
speakers closed, so a test run never records you or makes noise.

    python e2e/run.py                  # builds release first
    python e2e/run.py --no-build
    python e2e/run.py --real-devices   # warm run uses your real mic, speakers and desktop
    python e2e/run.py --real-camera    # ...and your webcam

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
from pathlib import Path

import look

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "e2e" / "artifacts"
def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# random ports so parallel runs (other worktrees, other agents) can't cross wires
HTTP, WS, VOICE = free_port(), free_port(), free_port()
BUDGET_MS = 1000


def build():
    subprocess.run(["cargo", "build", "--release", "--bin", "fastcord", "--example", "mockcord"], cwd=ROOT, check=True)


def prime():
    """First launch of a freshly linked exe spends ~450ms in the OS (Defender scan, cold
    pages) before main runs. Pay that here so the timed cold start measures our code."""
    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ, FASTCORD_DATA=tmp + "/data", FASTCORD_E2E=tmp, FASTCORD_FAKE_MIC="1", FASTCORD_FAKE_CAM="1", FASTCORD_SILENT="1")
        p = subprocess.Popen([str(ROOT / "target" / "release" / "fastcord.exe")], env=env)
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
        "FASTCORD_API": f"http://127.0.0.1:{HTTP}/api/v9",
        "FASTCORD_CDN": f"http://127.0.0.1:{HTTP}/cdn",
        "FASTCORD_EMOJI_CDN": f"http://127.0.0.1:{HTTP}/cdn/emoji",
        "FASTCORD_FONTS": f"http://127.0.0.1:{HTTP}",
        "FASTCORD_GATEWAY": f"ws://127.0.0.1:{WS}",
        "FASTCORD_TOKEN": "mock-token",
        "FASTCORD_DATA": str(data_dir),
        "FASTCORD_E2E": str(out),
    })
    env.update(extra or {})
    exe = ROOT / "target" / "release" / "fastcord.exe"
    spawned = time.time() * 1000
    p = subprocess.Popen([str(exe)], env=env)
    try:
        code = p.wait(timeout=300)
    except subprocess.TimeoutExpired:
        p.kill()
        code = "timeout"
    res = json.loads((out / "results.json").read_text()) if (out / "results.json").exists() else {"ok": False, "error": "no results.json"}
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
    mock = subprocess.Popen([str(ROOT / "target" / "release" / "examples" / "mockcord.exe"), str(HTTP), str(WS), str(log), str(VOICE)])
    time.sleep(0.5)
    try:
        # real mic, speakers, desktop and webcam are all opt-in
        fake = {"FASTCORD_FAKE_MIC": "1", "FASTCORD_FAKE_VIDEO": "1", "FASTCORD_SILENT": "1"}
        cam = {} if "--real-camera" in sys.argv else {"FASTCORD_FAKE_CAM": "1"}
        cold = run_client("cold", data, {**fake, **cam})
        warm = run_client("warm", data, cam if "--real-devices" in sys.argv else {**fake, **cam})
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
            and near(g("chan:2303")[3], 32)
            and near(g("chanicon:2303")[2], 20)
            # discord draws emoji in names at about the text size, not 1.25x of it
            and 14 <= (g("chanemoji:2303")[3] or 0) <= 18
            and near(g("userpanel")[3], 56, 8.5)
            and near(g("member:101")[3], 42)
            and g("voicebadge:1008")[2] is not None and g("voicebadge:1004")[2] is None
            and rc.get("side_unread_1007") is True
            and not any(rc.get(f"side_unread_{i}") for i in (1003, 1004, 1005, 1006, 1008))
            and rc.get("side_locked_2302") is True and rc.get("side_locked_2303") is False
            and rc.get("side_locked_2309") is True and rc.get("side_locked_2310") is False
        )

    checks = {
        "side: rail icons 40pt at a 48pt pitch, emoji at text size, unread pills only where discord shows them": side_ok(cold) and side_ok(warm),
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
                              ("light theme applied", "theme_light"), ("compact mode applied", "compact"), ("streamer mode toggled", "streamer"),
                              ("display name changed through the modal", "display_name")]},
        "settings: profile pronouns PATCHed": sum('"pronouns":"they/them"' in r.get("body", "") for r in reqs if r.get("path", "").endswith("/users/@me/profile")) == 2,
        "settings: content filter PATCHed to the legacy settings": sum('"explicit_content_filter":2' in r.get("body", "") for r in reqs if r.get("method") == "PATCH") == 2,
        "settings: language switch reached the locale header (hypesquad join sent as de)": sum(1 for r in reqs if r.get("path", "").endswith("/hypesquad/online") and r.get("locale") == "de" and '"house_id":1' in r.get("body", "")) == 2,
        "settings: display name PATCHed": sum('"global_name":"You!"' in r.get("body", "") for r in reqs) == 2,
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
        "older history was paged in via ?before=": len(history) >= 2,
        "read state was acked": len(acks) >= 1,
        "member list subscription (op 37) was sent": len(subs) >= 1,
        "message authors were requested via op 8": any(r.get("gateway_op") == 8 for r in reqs),
        **{f"{name}: {what}": all(what in r.get("marks_ms", {}) for r in (cold, warm))
           for name, what in [("quick switcher (ctrl+k) jumped to #help", "switcher"), ("forum posts loaded", "forum"),
                              ("thread opened from sidebar", "thread"), ("profile popout loaded", "profile"),
                              ("search returned results", "search"), ("pins loaded", "pins"), ("settings opened", "settings"),
                              ("dm from someone else raised a notification", "notified")]},
        "forum, pins, search and profile hit the api": all(any(k in r.get("path", "") for r in reqs) for k in ("/threads/search", "/pins", "/messages/search", "/profile")),
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
        "layout: dragging the channel list edge widened it by ~60 and the chat column reflowed (both runs)":
            all(lay(r, "side_dragged") - lay(r, "side_start") > 50 and lay(r, "chat_after_side") < lay(r, "chat_start") - 50 for r in (cold, warm)),
        "layout: dragging the member list edge left widened it and the chat column reflowed again (both runs)":
            all(lay(r, "members_dragged") - lay(r, "members_start") > 30 and lay(r, "chat_after_members") < lay(r, "chat_after_side") - 30 for r in (cold, warm)),
        "layout: dragging the channel list far past its limit stops at 480 and leaves the chat column >= 400 (both runs)":
            all(lay(r, "side_max") <= 480.5 and lay(r, "chat_min") >= 400 for r in (cold, warm)),
        "layout: warm run starts with the panel widths the cold run left":
            abs(lay(warm, "side_start") - lay(cold, "side_end")) < 1 and abs(lay(warm, "members_start") - lay(cold, "members_dragged")) < 1,
        "frame cpu p99 < 8ms (cold)": cold.get("frame_cpu_ms", {}).get("p99", 1e9) < 8.0,
        "frame cpu p99 < 8ms (warm)": warm.get("frame_cpu_ms", {}).get("p99", 1e9) < 8.0,
        **look.checks({"cold": cold, "warm": warm}, reqs, data),
    }
    report = {"checks": checks, "cold": cold, "warm": warm, "requests_seen": len(reqs), "sent_bodies": sent_bodies}
    (ART / "report.json").write_text(json.dumps(report, indent=2))

    md = ["# fastcord e2e report", ""]
    md += [f"- [{'x' if ok else ' '}] {name}" for name, ok in checks.items()]
    for name, r in (("cold", cold), ("warm", warm)):
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
