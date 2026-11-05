"""Checks for Discord's little behaviours (ref/QOL.md), driven by qol_steps in src/ui/e2e.rs:
what each did on screen, and what it asked the mock to do."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    sent = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    posts = [body(r).get("content", "") for r in sent("POST", "/channels/2011/messages")]
    return {
        "qol: +:fire: reacted to the newest message instead of sending it": both("qol_react")
            and len([r for r in reqs if r.get("method") == "PUT" and "/reactions/" in r.get("path", "")]) >= 2 and "+:fire:" not in posts,
        "qol: s/typo/fixed edited the last message, nothing sent": both("qol_sub")
            and sum(body(r).get("content") == "fixed here" for r in sent("PATCH", "/channels/2011/messages/")) == 2 and not any(p.startswith("s/") for p in posts),
        "qol: /shrug hi sent discord's shrug": both("qol_shrug") and posts.count("hi ¯\\_(ツ)_/¯") == 2,
        "qol: text pasted past the limit became message.txt, and Escape took the upload card off": both("qol_paste_txt") and both("qol_esc_uploads"),
        "qol: ctrl+b wrapped the selection in **": both("qol_ctrl_b"),
        "qol: Enter inside an open ``` block made a new line": both("qol_code_enter") and not any(p.startswith("```rs") for p in posts),
        "qol: shift+tab put focus on the newest message and R replied to it": both("qol_msg_reply"),
        "qol: holding shift turned the hover bar into discord's long one, and its delete didn't ask": both("qol_shift_bar") and both("qol_shift_delete")
            and len(sent("DELETE", "/channels/2011/messages/")) >= 2,
        "qol: a masked link asked \"Leaving Discord\", and a trusted site stopped asking": both("qol_leaving") and both("qol_trusted"),
        "qol: P on a dm message asked \"Pin It. Pin It Good.\" and Enter pinned it": both("qol_pin") and len(sent("PUT", "/channels/3000/messages/pins/")) >= 2,
        "qol: S read the message out": both("qol_speak"),
        "qol: Bookmark Message saved it, and the inbox's Bookmarks tab lists it": both("qol_bookmark") and both("qol_bookmarks_tab")
            and len(sent("PUT", "/users/@me/saved-messages/3000/")) >= 2,
        "qol: inbox Mark All as Read asked first": both("qol_read_all"),
        "qol: an incoming call's popup, and Escape declined it": both("qol_decline") and len(sent("POST", "/channels/3000/call/stop-ringing")) >= 2,
        "qol: Enter saved a note on alice": both("qol_note") and sum(body(r).get("note") == "likes rust" for r in sent("PUT", "/users/@me/notes/101")) >= 1,
        "qol: ctrl+alt+right went back to the server, ctrl+alt+down to the next": both("qol_server_keys"),
        "qol: ! in the quick switcher found the voice channel": both("qol_switcher_voice"),
        "qol: Create Thread from a message made the thread and opened it": both("qol_thread")
            and sum(body(r).get("name") == "e2e thread" for r in sent("POST", "/threads")) == 2,
        "qol: alt+click starred an emoji in the picker": both("qol_fav_emoji"),
        "qol: ctrl+/ opened the sheet, its arrows lit, and H H right N K played the Raging Demon": both("qol_combos") and both("qol_demon"),
        "qol: Escape well back in the history jumped to the present": both("qol_esc_present"),
        "feel: one wheel notch moved the chat by Windows' lines setting times 100/3": both("feel_wheel_notch"),
        "feel: PageUp with the composer focused scrolled the chat most of a screen": both("feel_page_key"),
        "qol: clicking the dark round any dialog closed it": both("qol_backdrops"),
        "qol: @time 5pm listed the timestamp styles and Enter wrote the first": both("qol_time_list") and both("qol_time"),
        "qol: an error toast opened its details on shift+click": both("qol_error_details"),
        "qol: Enter on Leaving Discord visited the site": both("qol_enter_dialog"),
        "qol: a select jumped to a row by its letter and Enter picked it": both("qol_select_keys"),
        "qol: the image cropper took keys and a drag, and Apply handed back a png": both("qol_crop"),
        "qol: E on an upload card opened Modify Attachment and Save renamed the file": both("qol_modify"),
        "qol: D on an upload card took it off": both("qol_card_keys"),
        "qol: the custom status ignored Shift+Enter, saved on Enter and went to discord": both("qol_status_shift_enter") and both("qol_custom_status")
            and sum('"e2e status"' in r.get("body", "") for r in sent("PATCH", "/users/@me/settings")) == 2,
        "qol: Select Friends picked two, Backspace took the last off, and Create Group DM made the group": both("qol_newdm_pick") and both("qol_newdm_backspace")
            and both("qol_group_dm") and sum(len(body(r).get("recipients", [])) == 2 for r in sent("POST", "/users/@me/channels")) == 2,
    }
