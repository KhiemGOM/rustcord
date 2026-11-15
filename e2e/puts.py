"""Fails when a `ui.put` leaves the layout cursor inside the box it was put in.

egui's `put(rect, widget)` moves the cursor to just after `rect`. Placing a text field's inner
rect inside a bigger box that way leaves the cursor in the box, so whatever the ui lays out next
starts too high (or too far left in a row) and overlaps it. Each put has to be followed, before
its block ends, by `ui.advance_cursor_after_rect(<the box>)`, or say why it needn't with a
`// put: <reason>` directive on the line before it.

    python e2e/puts.py            # scans src/, exit code 1 and a line per offence
    python e2e/puts.py --self-test

`cargo test --bin rustcord` runs it too (src/ui/locks.rs).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PUT = re.compile(r"\bui\.put\(")


def offences(text, name):
    lines = text.splitlines()
    found = []
    for i, line in enumerate(lines):
        if not PUT.search(line) or line.lstrip().startswith("//"):
            continue
        if i > 0 and lines[i - 1].strip().startswith("// put:"):
            continue
        depth = len(line) - len(line.lstrip())
        ok = False
        for later in lines[i + 1:]:
            if later.strip() and len(later) - len(later.lstrip()) < depth:
                break
            if "advance_cursor_after_rect" in later:
                ok = True
                break
        if not ok:
            found.append(f"{name}:{i + 1}: ui.put without advance_cursor_after_rect or a `// put:` reason")
    return found


def self_test():
    bad = "    let e = ui.put(inner, edit);\n    ui.add_space(8.0);\n}\n"
    good = "    let e = ui.put(inner, edit);\n    ui.painter().rect_stroke(r);\n    ui.advance_cursor_after_rect(r);\n}\n"
    said = "    // put: the last thing in its ui\n    ui.put(inner, edit);\n}\n"
    closed = "        ui.put(inner, edit);\n    }\n    ui.advance_cursor_after_rect(r);\n"
    assert offences(bad, "x") and not offences(good, "x") and not offences(said, "x") and offences(closed, "x")
    print("ok")


def main():
    if "--self-test" in sys.argv:
        return self_test()
    found = []
    for path in sorted((ROOT / "src").rglob("*.rs")):
        rel = path.relative_to(ROOT).as_posix()
        if "/e2e" in rel:
            continue
        found += offences(path.read_text(encoding="utf-8"), rel)
    print("\n".join(found) or "ok")
    sys.exit(1 if found else 0)


if __name__ == "__main__":
    main()
