"""Builds assets/art.tsv, the list of Discord's icons the client fetches at runtime.

Each row is name, seed chunk, and an svg template whose path data is a fingerprint of the path
in that chunk of discord.com/assets, so the repo carries the shapes' layout but not Discord's
drawings. Paths that aren't in any chunk stay literal.

    python tools/art_table.py <dir of discord chunks> <svg>... > assets/art.tsv
"""

import os
import re
import sys

SEEDS = ["web.843cc7edc28c426c", "1a1518aa502a9482"]


def fnv(s):
    h = 0xcbf29ce484222325
    for b in s.encode():
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def name(path):
    stem = os.path.splitext(os.path.basename(path))[0]
    return "tag_" + stem if os.path.basename(os.path.dirname(path)) == "tag_badges" else stem


def main():
    chunks = {s: set(re.findall(r'd:"([^"]+)"', open(os.path.join(sys.argv[1], s + ".js"), encoding="utf-8").read())) for s in SEEDS}
    literal = []
    for path in sys.argv[2:]:
        svg = " ".join(open(path, encoding="utf-8").read().split())
        ds = re.findall(r'\sd="([^"]+)"', svg)
        seed = next((s for s in SEEDS if any(d in chunks[s] for d in ds)), "")
        for d in ds:
            if seed and d in chunks[seed]:
                svg = svg.replace(f'd="{d}"', f'd="{{{fnv(d)}}}"')
        if not seed:
            literal.append(name(path))
        print(f"{name(path)}\t{seed}\t{svg}")
    print("literal:", " ".join(literal), file=sys.stderr)


main()
