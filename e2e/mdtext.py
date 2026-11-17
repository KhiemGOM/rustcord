"""Checks for mdtext_steps in src/ui/e2e/mdtext.rs: how message text is laid out."""


def checks(runs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    return {
        "text: a bulleted list's items sit 16 in from the text, and a list inside an item 16 in from that": both("text_list_bullets"),
        "text: a numbered list counts on from its first number, sits in by its digits, and leaves 4 under each item": both("text_list_numbers"),
        "text: inline code sits in its 20px box and a covered spoiler in its 21px one": both("text_boxes"),
        "text: a quote keeps to 90% of the text's width": both("text_quote_width"),
        "text: a click opens the spoiler under it and leaves the other covered": both("text_spoiler_one"),
    }
