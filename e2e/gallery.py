"""Checks for the gallery channel and the account's favourites (steps 4000 and up in
src/ui/e2e/gallery.rs): what the mosaics, forwards and stickers drew, what went to
settings-proto/2, and that the mock's CDN refused nothing."""

import base64
import json


def varint(b, i):
    out = shift = 0
    while True:
        byte = b[i]
        i += 1
        out |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return out, i
        shift += 7


def fields(blob):
    """The top-level fields of a protobuf message as (number, payload bytes or int)."""
    out, i = [], 0
    while i < len(blob):
        key, i = varint(blob, i)
        wire = key & 7
        if wire == 0:
            v, i = varint(blob, i)
        elif wire == 2:
            n, i = varint(blob, i)
            v, i = blob[i:i + n], i + n
        elif wire == 1:
            v, i = blob[i:i + 8], i + 8
        elif wire == 5:
            v, i = blob[i:i + 4], i + 4
        else:
            break
        out.append((key >> 3, v))
    return out


def patches(reqs):
    out = []
    for r in reqs:
        if r.get("method") == "PATCH" and r.get("path", "").endswith("/settings-proto/2"):
            try:
                out.append(dict(fields(base64.b64decode(json.loads(r["body"])["settings"]))))
            except (ValueError, KeyError, IndexError):
                out.append({})
    return out


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    gets = [r for r in reqs if r.get("method") == "GET" and r.get("path", "").endswith("/settings-proto/2")]
    sent = patches(reqs)
    wrote_emoji = any(b"fire" in p.get(5, b"") for p in sent)
    cleared_gif = any(2 in p and b"e2e-favourite" not in p[2] for p in sent)
    refused = [r["cdn_refused"] for r in reqs if "cdn_refused" in r]
    return {
        "gallery: every attachment mosaic (2, 3, 4, 5, 7 and 10 tiles, and image with video) is tidy, 4 apart, inside 550 wide": all(both(f"mosaic_{i}") for i in range(8)),
        "gallery: forwards carry the forwarded message's content, attachments and embeds, stickers parse": both("forward_parsed") and both("stickers_parsed"),
        "favourites: the account's settings-proto/2 was read": len(gets) >= 2,
        "favourites: another client's favourites and frecency reached the pickers": both("favs_from_account"),
        "favourites: starring and using here went back as a settings-proto/2 patch": both("favs_written") and wrote_emoji and cleared_gif,
        "cdn: every url the client built has the shape discord's CDN accepts": not refused,
    }
