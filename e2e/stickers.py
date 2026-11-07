"""Checks for Lottie stickers, driven by sticker_steps in src/ui/e2e/stickers.rs: drawn
and animated in chat and the picker, the pack popout, and no drawing once off screen."""


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    idle = [r.get("rects", {}).get("lottie_idle_wakes") for r in runs.values()]
    asked = [r for r in reqs if r.get("method") == "GET" and r.get("path", "").startswith("/cdn/stickers/8103.json")]
    return {
        "stickers: a Lottie sticker was fetched as json and drew four or more distinct frames in chat": both("lottie_chat_plays") and len(asked) >= 1,
        "stickers: a click on a default-pack sticker opened the pack popout and View More Stickers opened the picker": both("lottie_pack_popout") and both("lottie_picker"),
        "stickers: the sticker pack's Lottie art animated in the picker, no text standing in for it": both("lottie_picker"),
        "stickers: with the sticker off screen no more of its frames were drawn and nothing in gif.rs or lottie.rs asked for a repaint for 1.5s": both("lottie_idle_off_screen")
            and all(n == 0 for n in idle),
    }
