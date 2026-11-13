"""Checks for theme colours, the gg sans download and message headers.

The client samples its own screenshots (see look_steps in src/ui/e2e.rs) and drops
the colours into results.json under rects["look:..."]; here they're compared with
what discord's web css resolves for each theme. Expected values are those tokens:
theme-midnight (Onyx), theme-darker (Dark), theme-dark (Ash), theme-light.
"""

OVERLAY = (0x97, 0x97, 0x9F)
HOVER_ALPHA = 0.122  # --message-background-hover

# rail = --background-base-lowest; channel list (.sidebar), chat, members and the channel
# header (.headerBar) = base-lower; input = --chat-background-default
SURFACES = {
    "Onyx": dict(rail="000000", side="000000", chat="000000", members="000000", header="000000", input="101013"),
    "Dark": dict(rail="121214", side="1a1a1e", chat="1a1a1e", members="1a1a1e", header="1a1a1e", input="222327"),
    "Ash": dict(rail="2c2d32", side="323339", chat="323339", members="323339", header="323339", input="393a41"),
    "Light": dict(rail="f3f3f4", side="fbfbfb", chat="fbfbfb", members="fbfbfb", header="fbfbfb", input="ffffff"),
}
FONTS = ("ggsans-400", "ggsans-500", "ggsans-600", "ggsans-700", "ggsans-400i", "ggsans-600i", "ggsans-700i", "ggmono-400", "ggmono-700", "ginto-nord-700")
# the nitro display name fonts' asset names (src/fonts.rs)
NAME_FONTS = {
    "33d4f12a85e1f736", "e6f5f44abb520735", "c560709c3470bb66", "52b541f86401a5b6", "69c735ca5c604de7", "8f20cb550d739cea",
    "27b34a77ba5d693e", "45ef50dce38a931a", "4841dace333e9054", "c00df4f25667809b", "aa59ae54ce7e540a",
}


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def near(a, b, tol=2):
    return all(abs(x - y) <= tol for x, y in zip(rgb(a), rgb(b)))


def over(chat):
    return tuple(round(o * HOVER_ALPHA + c * (1 - HOVER_ALPHA)) for o, c in zip(OVERLAY, rgb(chat)))


def sfnt(path):
    try:
        return path.read_bytes()[:4] in (b"\x00\x01\x00\x00", b"OTTO", b"true")
    except OSError:
        return False


def checks(runs, reqs, data):
    out = {}
    for tag, r in runs.items():
        look = r.get("rects", {})
        for theme, want in SURFACES.items():
            got = {k: look.get(f"look:look_{theme}:{k}", "#?") for k in want}
            bad = [k for k, v in want.items() if not (got[k].startswith("#") and "?" not in got[k] and near(got[k], v))]
            out[f"look: {tag}: {theme} surfaces match discord's tokens"] = not bad
            if bad:
                print(f"{tag} {theme}: " + ", ".join(f"{k} {got[k]} != #{want[k]}" for k in bad))
            hov = look.get(f"look:look_{theme}_hover:msg_hover", "#?")
            out[f"look: {tag}: {theme} hovered message is the 0.122 overlay"] = "?" not in hov and near(hov, "#%02x%02x%02x" % over(want["chat"]))

        out[f"look: {tag}: gg sans is the proportional face (and its medium cut), italics gg sans italic, code gg mono"] = (
            [look.get(f"look:font{k}") for k in ("", "_medium", "_italic", "_mono")] == ["ggsans-400", "ggsans-500", "ggsans-700i", "ggmono-400"])

        c = lambda k: rgb(look[f"look:look_Ash:{k}"]) if f"look:look_Ash:{k}" in look else (0, 0, 0)
        # carol: two-tone role, tiled every 100px; eve: holographic role; frank: a nitro glow style,
        # which in a server gives way to his role's colour like discord's
        out[f"look: {tag}: duo role gradient runs magenta to cyan across the name"] = (
            c("name103_l")[0] - c("name103_r")[0] >= 30 and c("name103_r")[1] - c("name103_l")[1] >= 25)
        out[f"look: {tag}: holographic role gradient goes from blue to pink"] = c("name105_r")[0] - c("name105_l")[0] >= 25
        out[f"look: {tag}: in a server a nitro style gives way to the role colour, flat and without a halo"] = (
            max(abs(a - b) for a, b in zip(c("name106_l"), c("name106_r"))) <= 12
            and c("name106_l")[2] - c("name106_l")[0] >= 100
            and max(abs(a - b) for a, b in zip(c("halo106_near"), c("halo106_far"))) <= 1)
        chat = rgb("#" + SURFACES["Ash"]["chat"])
        out[f"look: {tag}: role icon (image) and role emoji drew something"] = all(
            max(abs(a - b) for a, b in zip(c(k), chat)) >= 40 for k in ("icon105", "icon103"))

        def box(k):
            return look.get(f"look:Ash:{k}") or [0, 0, 0, 0]

        for who in (103, 105, 106):
            av, nm, st = box(f"avatar:{who}"), box(f"name:{who}"), box(f"stamp:{who}")
            icon = box(f"role_icon:{who}")
            after = icon[0] + icon[2] if icon[2] else nm[0] + nm[2] + 4
            out[f"look: {tag}: header {who}: 40px avatar, name 56px right of it, centred 9px below the avatar top"] = (
                av[2] == 40 and abs(nm[0] - av[0] - 56) <= 1 and abs(nm[1] + nm[3] / 2 - av[1] - 9) <= 2)
            out[f"look: {tag}: header {who}: timestamp 4px after the {'role icon' if icon[2] else 'name + margin'}, time only"] = (
                abs(st[0] - after - 4) <= 1 and 0 < st[2] < 60)
        icon = box("role_icon:105")
        nm = box("name:105")
        out[f"look: {tag}: role icon is 20px, 4px after the name"] = icon[2] == 20 and abs(icon[0] - nm[0] - nm[2] - 4) <= 1

    cached = [data / "fonts" / f"{f}.ttf" for f in FONTS]
    out["look: gg sans and gg mono were decoded to ttf in the cache dir"] = all(sfnt(p) for p in cached)
    paths = [r.get("path", "") for r in reqs]
    woff = [p for p in paths if p.endswith(".woff2")]
    # the nitro display name fonts come by their real names; everything else is gg sans or gg mono
    names = [p for p in woff if p.split("/")[-1][:-6] in NAME_FONTS]
    gg = [p for p in woff if p not in names]
    out["look: font download read the new names off /app after the built-in one 404ed, and the warm run reused the cache"] = (
        "/app" in paths and "/assets/fonts.css" in paths
        and sum("/assets/mock" in p for p in gg) == 2 * len(FONTS) and len(gg) == 2 * (len(FONTS) + 1))
    out["look: the eleven nitro name fonts were each fetched once, the warm run reused them"] = (
        len(names) == 22 and len(set(names)) == 11 and all(names.count(n) == 2 for n in set(names)))
    return out
