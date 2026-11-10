def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    shifts = [r.get("rects", {}).get("history_anchor_shift") for r in runs.values()]
    paths = [r.get("path", "") for r in reqs if r.get("method") == "GET"]
    return {
        "regressions: a search result for an individual decoration variant opened its preview": both("decoration_variant_search"),
        "regressions: fullscreen filled its monitor and every border stayed non-resizable": both("fullscreen_locked"),
        "regressions: the animated WebP emoji changed pixels inside its image embed": both("webp_embed_animates")
            and all(r.get("rects", {}).get("webp_pixels_changed", 0) > 0 for r in runs.values()),
        "regressions: decoded WebP stayed available after its downloaded bytes were evicted": both("webp_survives_byte_eviction"),
        "regressions: a native paste command attached the clipboard image once": both("native_image_paste"),
        "regressions: incoming and outgoing friend-request rows open their profiles": both("incoming_request_profile") and both("outgoing_request_profile"),
        "regressions: Add Friend text is centered beside its send button": both("add_friend_aligned")
            and all(r.get("rects", {}).get("add_friend_text_shift", 999) <= 1 for r in runs.values()),
        "regressions: pinning and unpinning a channel through its menu updated the pinned section": both("channel_pinned_by_click") and both("channel_unpinned_by_click"),
        "regressions: a notification-only settings update preserved the channel pin": both("channel_pin_survives_update"),
        "regressions: loading older history kept the reading anchor within two pixels": both("history_kept_reading_anchor")
            and all(isinstance(shift, (int, float)) and abs(shift) <= 2 for shift in shifts),
        "regressions: selecting an Orb decoration variant requested its animated image": both("orb_decoration_variant_selected")
            and all(any(f"/avatar-decoration-presets/{asset}.png?" in p and "passthrough=true" in p for p in paths)
                    for asset in ("a_mockpanda", "a_mockpanda_blue")),
    }
