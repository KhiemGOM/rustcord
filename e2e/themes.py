"""Checks for themes_steps in src/ui/e2e/themes.rs: Appearance's theme preview for a free account, the
custom theme editor, a server's own theme and the default for servers, and the app icon. What the
client saves is checked on the mock's settings-proto requests."""

import base64
import json


def sent_blobs(reqs):
    out = []
    for r in reqs:
        if r.get("method") != "PATCH" or not r.get("path", "").endswith("/settings-proto/1"):
            continue
        try:
            out.append(base64.b64decode(json.loads(r.get("body") or "{}").get("settings", "")))
        except ValueError:
            pass
    return out


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    blobs = sent_blobs(reqs)
    has = lambda part: any(part in b for b in blobs)
    return {
        "themes: Appearance draws Preview Theme, the Custom Theme tile and the preset swatches": both("themes_settings_rows"),
        "themes: a free account's click on a preset closed settings and opened the preview panel with it on": both("themes_preview_opens") and both("themes_preview_applies") and both("themes_preview_unsaved"),
        "themes: a swatch in the panel previewed another theme, and Unlock with Nitro dropped the preview and opened Nitro": both("themes_swatch_previews") and both("themes_unlock_to_nitro") and both("themes_preview_reverts"),
        "themes: Preview Theme opened the panel with nothing picked and Exit Preview went back to Appearance": both("themes_exit_returns"),
        "themes: a preview is never sent to the account": not has(b"\x12\x02\x08\x03"),
        "themes: Try it out opened the editor with Discord's blurple, and Add Color, Light Mode, a typed hex, direction and intensity each moved it": both("themes_editor_opens")
            and both("themes_editor_add_color") and both("themes_editor_light") and both("themes_editor_hex") and both("themes_editor_angle") and both("themes_editor_intensity"),
        "themes: Surprise Me changed the colours and Reset put the opening ones back": both("themes_editor_surprise") and both("themes_editor_reset"),
        "themes: a free account's editor has no Apply and saved nothing": both("themes_free_has_no_apply") and both("themes_free_cannot_apply"),
        "themes: a server with the theme feature showed its own theme while open and let go when left": both("server_theme_parsed") and both("server_theme_shown") and both("server_theme_left"),
        "themes: Use my theme in Default theme in servers stopped a server's theme and went to the account": both("server_theme_default_mine") and both("server_theme_skipped_when_mine") and has(b"p\x01"),
        "themes: with Nitro a preset applied from Appearance without leaving it, and went to the account": both("themes_nitro_applies") and has(b"\x12\x02\x08\x15"),
        "themes: Apply in the editor saved the custom theme, with its colours, angle and intensity on the account": both("themes_custom_applied")
            and any(b"#ff0066" in b and b"\x18\x5a" in b and b"\x20\x28" in b for b in blobs),
        "themes: with Nitro an app icon was chosen and put on the window": both("app_icon_chosen") and both("app_icon_on_window"),
        "themes: a free account's Preview Icon opened the panel, previewed an icon without choosing it and Exit Preview went back": both("app_icon_previews") and both("app_icon_exit"),
    }
