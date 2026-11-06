"""Checks for profed_steps in src/ui/e2e/profed.rs: the profile editor's layout and every picker,
the card edited in place, widgets, and what Save and the pickers send to the mock."""

import json


def body(reqs, method, path):
    out = []
    for r in reqs:
        if r.get("method") == method and r.get("path", "").split("?")[0] == path:
            try:
                out.append(json.loads(r.get("body") or "{}"))
            except ValueError:
                pass
    return out


def checks(runs, reqs, data):
    cold = runs["cold"].get("marks_ms", {})
    has = lambda *names: all(n in cold for n in names)
    profile = body(reqs, "PATCH", "/api/v9/users/@me/profile")
    account = body(reqs, "PATCH", "/api/v9/users/@me")
    widgets = body(reqs, "PUT", "/api/v9/users/@me/widgets")
    deleted = [r for r in reqs if r.get("method") == "DELETE" and r.get("path", "").endswith("/users/@me/avatars/1")]
    fetched = lambda frag: any(frag in r.get("path", "") for r in reqs)
    last = profile[-1] if profile else {}
    acct = {}
    for b in account:
        acct.update(b)
    return {
        "profed: the editor opens with the styles panel, the card, the board and every tile": has("profed_layout"),
        "profed: each picker lists Your items, Exclusive to Nitro and the shop, and Apply puts the choice in the draft":
            has("profed_pick_sections", "profed_decoration_applied", "profed_effect_applied", "profed_frame_applied", "profed_nameplate_applied"),
        "profed: an effect plays its layers in the editor and its products were fetched": has("profed_effect_preview_plays") and fetched("/collectibles-categories/v2"),
        "profed: Recent Avatars lists /users/@me/avatars and picking one sets the draft; the x asks before deleting": has("profed_recent_avatars", "profed_recent_avatar_picked", "profed_recent_delete_asks") and fetched("/users/@me/avatars") and bool(deleted),
        "profed: the display name style dialog applies a font, effect and colours, and Surprise Me re-rolls": has("profed_style_applied", "profed_surprise_me"),
        "profed: the colour dialog takes a hex colour for the theme": has("profed_theme_colour"),
        "profed: the banner pencil opens its menu, and Change Profile Effect opens that picker": has("profed_banner_menu", "profed_banner_menu_opens_effects"),
        "profed: the server tag menu lists tags and picks one": has("profed_tag_menu", "profed_tag_picked"),
        "profed: pronouns and bio edit on the card, and Add custom status opens the status dialog": has("profed_card_edits", "profed_status_opens"),
        "profed: widgets are added from their dialogs and Browse All Widgets opens": has("profed_widget_added", "profed_widget_games_added", "profed_browse_widgets") and fetched("/widget-configs/featured") and fetched("/users/@me/widgets/suggested-games"),
        "profed: the Board, Activity and Wishlist tabs switch": has("profed_tabs"),
        "profed: changes raise the save bar and Save writes the profile, account and widgets in discord's bodies": has("profed_save_bar", "profed_saved") and bool(widgets)
            and last.get("bio") == "hello from the editor" and last.get("pronouns") == "they/them" and last.get("theme_colors", [None])[0] == 0x00B89A
            and last.get("profile_effect_id") == "7003" and last.get("profile_frame_id") == "7005"
            and acct.get("avatar_decoration_sku_id") == "7001" and acct.get("nameplate_sku_id") == "7002" and acct.get("avatar_id") == "2"
            and acct.get("display_name_font_id") == 7 and acct.get("display_name_effect_id") == 2 and len(acct.get("display_name_colors", [])) == 2 and bool(acct.get("primary_guild_id"))
            and widgets and {w["data"]["type"] for w in widgets[-1]["widgets"]} == {"favorite_games", "played_games"},
        "profed: Hide Styles collapses the panel": has("profed_hide_styles"),
        "profed: someone's profile resolves their effect and frame from /collectibles-products and plays them": has("profile_effect_resolved") and fetched("/collectibles-products/7003"),
    }
