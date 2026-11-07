"""Checks for bot_steps in src/ui/e2e/bots.rs: option pills and their pickers, autocomplete,
modals, the states between a command and its answer, and message components. The REST bodies are
read back from the mock's request log; the screenshots are in each run's folder (bot_*.png)."""

import json

ORDER = ["text", "choice", "count", "ratio", "flag", "person", "rank", "place", "either", "file", "city"]
TYPES = [3, 3, 4, 10, 5, 6, 8, 7, 9, 11, 3]


def body_of(r):
    b = r.get("body") or ""
    try:
        return json.loads(b[b.index("{"):b.rindex("}") + 1])
    except ValueError:
        return {}


def checks(runs, reqs, data):
    marks = {name: set(r.get("marks_ms", {})) for name, r in runs.items()}
    has = lambda *names: all(n in m for m in marks.values() for n in names)
    inter = [body_of(r) for r in reqs if r.get("path", "").endswith("/interactions")]
    of = lambda t: [b for b in inter if b.get("type") == t]
    named = lambda t, n: [b for b in of(t) if b.get("data", {}).get("name") == n]
    runs_n = len(runs)

    show = named(2, "showcase")
    options = show[0]["data"]["options"] if show else []
    got = {o["name"]: o for o in options}
    val = lambda k: got.get(k, {}).get("value")
    atts = show[0]["data"]["attachments"] if show else []
    showcase_ok = (
        len(show) == runs_n
        and show[0]["application_id"] == "104" and show[0]["channel_id"] == "2011" and bool(show[0].get("session_id"))
        and str(show[0].get("nonce", "")).isdigit() and show[0].get("analytics_location") == "slash_ui"
        and show[0]["data"]["id"] == "9003" and show[0]["data"]["type"] == 1 and show[0]["data"]["application_command"]["name"] == "showcase"
        and val("text") == "abc" and val("choice") == "g" and val("count") == 4 and isinstance(val("count"), int)
        and val("ratio") == 0.5 and val("flag") is True and val("person") == "101" and str(val("rank")).isdigit()
        and val("place") == "2011" and val("either") == "102" and val("file") == 0 and val("city") == "berlin"
        and [got[k]["type"] for k in ORDER] == TYPES
        and [o["name"] for o in options] == ORDER
        and len(atts) == 1 and atts[0].get("filename") == "bot_upload.txt" and bool(atts[0].get("uploaded_filename")) and "path" not in atts[0]
    )
    stage = [r for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith("/channels/2011/attachments") and "bot_upload.txt" in r.get("body", "")]
    puts = [r for r in reqs if r.get("method") == "PUT" and r.get("path", "").startswith("/upload/") and r.get("path", "").endswith("/bot_upload.txt")]

    auto = of(4)
    ber = [b for b in auto if any(o.get("focused") and o.get("value") == "ber" for o in b["data"]["options"])]
    auto_ok = (
        len(ber) == runs_n
        and all(b.get("session_id") and str(b.get("nonce", "")).isdigit() and "analytics_location" not in b
                and b["application_id"] == "104" and b["data"]["id"] == "9003" for b in ber)
        and all(any(o["name"] == "text" and o["value"] == "abc" for o in b["data"]["options"]) for b in ber)
    )
    lookup = [b for b in auto if b["data"]["name"] == "lookup"]

    modal = of(5)
    flat = {}

    def walk(c):
        if c.get("type") == 1:
            [walk(x) for x in c["components"]]
        elif c.get("type") == 18:
            walk(c["component"])
        elif "custom_id" in c:
            flat[c["custom_id"]] = c

    comps = modal[0]["data"]["components"] if modal else []
    [walk(c) for c in comps]
    modal_ok = (
        len(modal) == runs_n and modal[0]["data"]["custom_id"] == "feedback_modal" and bool(modal[0]["data"].get("id"))
        and modal[0]["application_id"] == "104" and bool(modal[0].get("session_id")) and str(modal[0].get("nonce", "")).isdigit()
        and comps[0]["type"] == 1 and comps[0]["components"][0]["type"] == 4 and comps[1]["type"] == 18 and comps[1]["component"]["type"] == 4
        and flat.get("subject", {}).get("value") == "Great app" and flat.get("details", {}).get("value") == "it opens forms now"
        and flat.get("mood", {}).get("type") == 3 and flat.get("mood", {}).get("values") == ["great"]
    )

    comp = of(3)
    pick = lambda cid: [b for b in comp if b["data"].get("custom_id") == cid]
    sel = lambda cid, kind, values: len(pick(cid)) == runs_n and all(
        b["data"]["component_type"] == kind and b["data"].get("type") == kind and b["data"]["values"] == values
        and b.get("message_id") and isinstance(b.get("message_flags"), int) and b["application_id"] == "104" and b["channel_id"] == "2011"
        and b.get("session_id") and str(b.get("nonce", "")).isdigit() for b in pick(cid))
    either = pick("pick_either")
    return {
        "bots: picking a command made option pills, only the required one up front, the rest listed to add, and a short string was refused with discord's message":
            has("bot_picker", "bot_pills", "bot_pill_validation", "bot_optional_list", "bot_all_options"),
        "bots: an integer outside min and max was refused with 'Enter a number between 1 and 10.'": has("bot_integer_range"),
        "bots: choices, user, role, channel, mentionable, boolean and attachment options each took a value from their picker": has("bot_pickers"),
        "bots: the slash command went out with every option typed, in order, its attachment staged and listed with uploaded_filename":
            showcase_ok and len(stage) == runs_n and len(puts) == runs_n,
        "bots: option autocomplete asked once after the typing stopped (type 4, the focused option, the others as context, no analytics_location) and listed what came back":
            auto_ok and has("bot_autocomplete"),
        "bots: an autocomplete that failed said 'Loading options failed', and the X in the bar dropped the command":
            has("bot_autocomplete_failed", "bot_slash_closed") and any(any(o.get("value") == "fail" for o in b["data"]["options"]) for b in lookup),
        "bots: a command's modal opened, refused empty required fields, and submitted as type 5 with the text inputs and the select":
            has("bot_modal_open", "bot_modal_validation", "bot_modal_submit") and modal_ok,
        "bots: 'Sending command...', then '{app} is thinking...', then the deferred message (flag 128), then its edit": has("bot_sending", "bot_thinking", "bot_deferred", "bot_deferred_edit"),
        "bots: a failed interaction said so, and after three seconds said the application did not respond": has("bot_failed", "bot_timed_out"),
        "bots: a rate-limited command failed with the cooldown message": has("bot_cooldown") and len(named(2, "cooldown")) == 2 * runs_n,
        "bots: select menus of every kind opened, a multi-select under its minimum sent nothing, and each answer went out with its component type and values":
            has("bot_menus", "bot_select_min", "bot_select_multi", "bot_select_user", "bot_select_role", "bot_select_channel", "bot_select_mentionable")
            and sel("pick_color", 3, ["r", "g"]) and sel("pick_user", 5, ["101"]) and sel("pick_role", 6, ["1100"]) and sel("pick_channel", 8, ["2011"])
            and len(either) == runs_n and all(b["data"]["component_type"] == 7 and sorted(b["data"]["values"]) == ["101", "102"] for b in either),
        "bots: a disabled menu sent nothing": has("bot_disabled") and not pick("pick_off"),
        "bots: a pressed button showed its loading state, a failed one its error":
            has("bot_button_loading", "bot_button_failed") and len(pick("slow_btn")) == runs_n and len(pick("fail_btn")) == runs_n,
        "bots: a link button asked 'Leaving Discord' first, and a premium button drew": has("bot_link_prompt", "bot_premium_button") and not pick("off_btn"),
        "bots: a reply to a bot's command answer without text said 'Click to see command'": has("bot_reply_header"),
    }
