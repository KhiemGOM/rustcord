"""Fails when a text box doesn't say which font it types in, or types in `reg`.

Discord draws text boxes and their placeholders at medium weight on the light theme on a 1x
screen, so a text box takes `field_font(ui.ctx(), size)` (src/theme.rs), which knows. An egui
`TextEdit` with no `.font(` falls back to the body style and one with `.font(reg(..))` is regular
everywhere; both miss it. A box that is meant to have another font just says so with its own
`.font(`.

    python e2e/fields.py            # scans src/ui, exit code 1 and a line per offence
    python e2e/fields.py --self-test

`cargo test --bin rustcord` runs it too (src/ui/locks.rs).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
START = re.compile(r"TextEdit::(singleline|multiline)\(")


def chain(text, at):
    """The builder chain that starts at `at`: up to the bracket, comma or semicolon that ends it."""
    depth = 0
    for i in range(at, len(text)):
        c = text[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                return text[at:i]
            depth -= 1
        elif c in ";," and depth == 0:
            return text[at:i]
    return text[at:]


def offences(text, name):
    found = []
    for m in START.finditer(text):
        built = chain(text, m.start())
        line = text.count("\n", 0, m.start()) + 1
        if ".font(" not in built:
            found.append(f"{name}:{line}: text box with no .font(: give it field_font(ui.ctx(), size)")
        elif ".font(reg(" in built:
            found.append(f"{name}:{line}: text box typed in reg(: use field_font(ui.ctx(), size)")
    return found


def self_test():
    bad = "ui.add(egui::TextEdit::singleline(&mut s).desired_width(f(a, b)));\nlet t = TextEdit::multiline(x).font(reg(16.0));"
    good = "ui.add(egui::TextEdit::singleline(&mut s).font(field_font(ui.ctx(), 16.0)).hint_text(h(1, 2)));\nlet t = TextEdit::multiline(x)\n    .font(mono(14.0));"
    assert len(offences(bad, "bad")) == 2, offences(bad, "bad")
    assert not offences(good, "good"), offences(good, "good")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
        sys.exit(0)
    found = []
    for path in sorted((ROOT / "src" / "ui").rglob("*.rs")):
        if "e2e" in path.relative_to(ROOT).parts or path.name == "e2e.rs":
            continue
        found += offences(path.read_text(encoding="utf-8"), path.relative_to(ROOT).as_posix())
    print("\n".join(found))
    sys.exit(1 if found else 0)
