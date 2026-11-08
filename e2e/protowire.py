"""Reads the status settings out of the PATCHes the client sent to the account's settings blob."""

import base64
import json
import struct


def varint(b, i):
    out = shift = 0
    while True:
        byte = b[i]
        i += 1
        out |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            return out, i


def fields(b):
    out = []
    i = 0
    while i < len(b):
        key, i = varint(b, i)
        wire = key & 7
        if wire == 0:
            v, i = varint(b, i)
        elif wire == 1:
            v = int.from_bytes(b[i:i + 8], "little")
            i += 8
        elif wire == 2:
            n, i = varint(b, i)
            v = b[i:i + n]
            i += n
        elif wire == 5:
            v = struct.unpack("<f", b[i:i + 4])[0]
            i += 4
        else:
            break
        out.append((key >> 3, v))
    return out


def first(b, n, default=None):
    return next((v for f, v in fields(b) if f == n), default)


def status_writes(reqs):
    out = []
    for r in reqs:
        if r.get("method") != "PATCH" or not r.get("path", "").endswith("/users/@me/settings-proto/1"):
            continue
        try:
            blob = base64.b64decode(json.loads(r.get("body") or "{}").get("settings", ""))
        except ValueError:
            continue
        for n, group in fields(blob):
            if n != 11 or not isinstance(group, bytes):
                continue
            d = {}
            for f, v in fields(group):
                if f == 1:
                    d["status"] = first(v, 1, b"").decode()
                elif f == 2:
                    d["custom"] = {"text": first(v, 1, b"").decode(), "expires": first(v, 4, 0), "created": first(v, 5, 0)}
                elif f == 4:
                    d["expires"] = v
                elif f == 5:
                    d["created"] = first(v, 1, 0)
            out.append(d)
    return out
