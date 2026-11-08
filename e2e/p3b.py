"""Checks for p3b_steps in src/ui/e2e/p3b.rs: the settings rows that do something, billing address editing
and the profile leftovers."""

import base64
import json
import re


def blobs(reqs):
    out = []
    for r in reqs:
        if r.get("method") == "PATCH" and r.get("path", "").endswith("/settings-proto/1"):
            try:
                out.append(base64.b64decode(json.loads(r.get("body") or "{}").get("settings", "")))
            except ValueError:
                pass
    return out


def edits(reqs):
    out = []
    for r in reqs:
        if r.get("method") == "PATCH" and "/billing/payment-sources/" in r.get("path", ""):
            try:
                out.append(json.loads(r.get("body") or "{}"))
            except ValueError:
                pass
    return out


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    return {
        "p3b: the shop's pointer was the hand over a tab and the arrow over bare shop, and the window answered with Discord's pictures": both("shop_cursor_hand") and both("shop_cursor_hand_applied") and both("shop_cursor_arrow") and both("shop_cursor_arrow_applied"),
        "p3b: editing a payment method sent the whole address and the new expiry in the shape Discord's client uses": both("billing_address_saved")
            and any(b.get("billing_address", {}).get("name") == "You Lovelace" and b["billing_address"].get("line_2") == "Apt 4" and b["billing_address"].get("state") == "HCM"
                    and b["billing_address"].get("country") == "VN" and b.get("expires_month") == 10 and b.get("default") is True for b in edits(reqs)),
        "p3b: with the legacy chat input on, typing /rem offered no app command, and with it off the command came back": both("legacy_no_app_commands") and both("legacy_off_app_commands"),
        "p3b: Choose GIF in the avatar dialog opened the GIF picker without tabs or stars, and the clicked GIF went to the cropper": both("avatar_gif_to_crop"),
        "p3b: the status dialog took an emoji from the picker, and Save sent its text and emoji name in the account's custom status": both("status_emoji_picked") and both("status_emoji_saved")
            and any(re.search(rb"emoji status(\x11.{8})?\x1a[\x01-\x40]", b, re.S) for b in blobs(reqs)),
        "p3b: the theme, custom theme and app icon panels opened at 940x500 and 150% (looked at in their screenshots)": both("themes_panels_small"),
        "p3b: with developer mode and Stream Info Overlay on, hovering a call tile showed the overlay": both("stream_info_overlay"),
        "p3b: turning Enable custom cursor in shop off gave the pointer back": both("shop_cursor_off"),
    }
