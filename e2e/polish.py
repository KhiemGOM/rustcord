"""Checks for the palette sweep, the voice panel inside the account card, channel
marks in the header and code blocks (see polish_steps in src/ui/e2e.rs)."""

import look
from look import rgb


def near(a, b, tol=2):
    return "?" not in a and look.near(a, b, tol)


def over(fg, a, bg):
    return "#%02x%02x%02x" % tuple(round(f * a + b * (1 - a)) for f, b in zip(rgb(fg), rgb(bg)))


def checks(runs, reqs, data):
    out = {}
    for tag, r in runs.items():
        rc = r.get("rects", {})
        g = lambda k: rc.get(f"polish:{k}") or [0, 0, 0, 0]
        px = lambda k: rc.get(f"look:{k}", "#?")

        lock, bare = px("polish_private:lock"), px("polish_public:lock")
        out[f"polish: {tag}: private channel header draws the lock over the glyph's corner, public one leaves it empty"] = (
            "?" not in lock and "?" not in bare and near(lock, "#999aa1", 24) and near(bare, "#393a41", 6))

        cam, scr, off, user = g("voice:panel_camera"), g("voice:panel_screen"), g("voice:disconnect"), g("userpanel")
        out[f"polish: {tag}: voice panel buttons are 32 high, equal width, 8 apart, 12 above the user card"] = (
            cam[3] == 32 and scr[3] == 32 and abs(cam[2] - scr[2]) <= 0.6 and abs(scr[0] - cam[0] - cam[2] - 8) <= 0.6
            and abs(user[1] - (cam[1] + cam[3]) - 13) <= 0.6)
        out[f"polish: {tag}: disconnect sits on the status row, flush with the buttons' right edge"] = (
            abs(off[1] - (cam[1] - 12 - 32)) <= 0.6 and abs(scr[0] + scr[2] - (off[0] + off[2])) <= 0.6)
        out[f"polish: {tag}: voice panel floats on the user card's colour, buttons are the 0.122 overlay"] = (
            near(px("polish_voice:gap"), "#36373e", 3) and near(px("polish_voice:button"), over("97979f", 0.122, "36373e"), 4))

        code = g("codeblock")
        fill, edge, chat = px("polish_code:fill"), px("polish_code:edge"), px("polish_code:chat")
        out[f"polish: {tag}: code block is --background-code over the chat with a --border-normal outline"] = (
            code[2] > 100 and near(chat, "#323339", 2) and near(fill, over("5865f2", 0.078, "323339"), 4)
            and near(edge, over("97979f", 0.2, over("5865f2", 0.078, "323339")), 14))
    return out
