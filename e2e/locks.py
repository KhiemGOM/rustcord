"""Fails when code reaches back into egui's context from inside one of its lock closures.

Every Context method (input, data, memory, request_repaint, even ctx.style) takes the one
RwLock inside it, and ctx.input takes it for writing, so a closure given to ctx.data_mut,
ctx.input, ctx.memory and the rest hangs the thread for good if it touches the context again:
directly, through ui.ctx(), or in a helper function it calls (which is followed, by name, as far
as it goes). Nothing reports it: the window just stops answering.

    python e2e/locks.py            # scans src/, exit code 1 and a line per offence
    python e2e/locks.py --self-test

`cargo test --bin rustcord` runs it too (src/ui/locks.rs).
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LOCKS = (
    "input", "input_mut", "input_for", "input_mut_for", "data", "data_mut", "memory", "memory_mut", "options", "options_mut",
    "fonts", "fonts_mut", "style_mut", "tessellation_options_mut", "graphics", "graphics_mut", "viewport", "viewport_for",
    "with_plugin", "plugin", "output", "output_mut", "write", "read",
)
CALL = re.compile(r"(?P<recv>[\w.()\[\]&*]*?)\.(?P<m>" + "|".join(LOCKS) + r")\(\s*(?:move\s*)?\|")
CTXLIKE = re.compile(r"(?:^|\.)(?:\w*ctx\w*|\w*ui|ui\w*)(?:\(\))?$")
BODY_USES = re.compile(r"\.(?:request_repaint\w*|request_discard|set_cursor_icon|send_viewport_cmd|copy_text|layer_painter|animate_\w+|pixels_per_point|set_pixels_per_point|content_rect|screen_rect|repaint_causes|set_visuals|set_style|set_fonts|is_context_menu_open|open_url)\(|(?<![\w])\w*ctx\w*\b|\.ctx\(\)|\b\w*ui\b\s*\.\s*(?:input|input_mut|data|data_mut|memory|memory_mut|painter|style_mut|ctx|request_repaint\w*)\b")
FN = re.compile(r"\bfn\s+(\w+)")
# names std, egui and every type share: a call to one says nothing about the helper of that name
COMMON = set(
    "new default from into get set insert remove push pop take any all map filter find first last len iter next collect contains clone min max abs "
    "key step load show add text label clear reset update draw paint run start stop open close send recv lock read write as_str to_string unwrap".split()
)


def blank(src):
    """Comments and strings turned to spaces, so braces and names inside them don't count."""
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            out[i:j] = " " * (j - i)
            i = j
        elif src.startswith("/*", i):
            j = src.find("*/", i)
            j = n if j < 0 else j + 2
            out[i:j] = [" " if ch != "\n" else "\n" for ch in src[i:j]]
            i = j
        elif c == '"' or (c == "r" and re.match(r'r#*"', src[i:])):
            m = re.match(r'r(#*)"', src[i:])
            if m:
                end = src.find('"' + m.group(1), i + m.end())
                j = n if end < 0 else end + 1 + len(m.group(1))
            else:
                j = i + 1
                while j < n and src[j] != '"':
                    j += 2 if src[j] == "\\" else 1
                j += 1
            out[i:j] = [" " if ch != "\n" else "\n" for ch in src[i:j]]
            i = j
        elif c == "'" and re.match(r"'(?:\\.|[^\\'])'", src[i:]):
            out[i:i + 3] = "   "
            i += 3
        else:
            i += 1
    return "".join(out)


def match_close(s, open_at):
    depth = 0
    for k in range(open_at, len(s)):
        if s[k] in "([{":
            depth += 1
        elif s[k] in ")]}":
            depth -= 1
            if depth == 0:
                return k
    return len(s)


def functions(files):
    """name -> True when its one definition uses the context in its body; names defined twice are left out."""
    touches, seen = {}, {}
    for _, clean in files:
        for m in FN.finditer(clean):
            seen[m.group(1)] = seen.get(m.group(1), 0) + 1
            brace = clean.find("{", m.end())
            semi = clean.find(";", m.end())
            if brace < 0 or (0 <= semi < brace):
                continue
            body = clean[brace:match_close(clean, brace)]
            touches[m.group(1)] = bool(BODY_USES.search(body))
    return {n: t for n, t in touches.items() if seen[n] == 1 and n not in COMMON}


def called(body):
    """Names called as plain functions or methods; `Type::name(` is left to the type."""
    return {m.group(2) for m in re.finditer(r"(?<![\w:])((?:[a-z_]\w*::)*)(\w+)\s*\(", body)} | set(re.findall(r"\.(\w+)\s*\(", body))


def reach(touches, files):
    """Close `touches` over calls: a function that calls one that touches the context touches it."""
    calls = {}
    for _, clean in files:
        for m in FN.finditer(clean):
            brace = clean.find("{", m.end())
            semi = clean.find(";", m.end())
            if brace < 0 or (0 <= semi < brace):
                continue
            body = clean[brace:match_close(clean, brace)]
            calls.setdefault(m.group(1), set()).update(re.findall(r"\b(\w+)\s*\(", body))
    changed = True
    while changed:
        changed = False
        for name, cs in calls.items():
            if name in touches and not touches.get(name) and any(touches.get(c) for c in cs if c != name):
                touches[name] = True
                changed = True
    return touches


def scan(files):
    touches = reach(functions(files), files)
    found = []
    for path, clean in files:
        for m in CALL.finditer(clean):
            if not CTXLIKE.search(m.group("recv")):
                continue
            paren = clean.rfind("(", 0, m.end() - 1)
            end = match_close(clean, paren)
            body = clean[m.end():end]
            line = clean.count("\n", 0, m.start()) + 1
            why = BODY_USES.search(body)
            if why:
                found.append((path, line, f"{m.group('m')} closure uses {why.group(0).strip()!r}"))
                continue
            for name in sorted(called(body)):
                if touches.get(name) and name not in LOCKS:
                    found.append((path, line, f"{m.group('m')} closure calls {name}(), which reaches the context"))
                    break
    return found


def load(root):
    files = []
    for p in sorted((root / "src").rglob("*.rs")):
        files.append((p.relative_to(root).as_posix(), blank(p.read_text(encoding="utf-8"))))
    return files


SAMPLES = {
    "nested input": ("ctx.data_mut(|d| d.insert_temp(key, ctx.input(|i| i.time)));", True),
    "through ui.ctx()": ("ui.ctx().memory_mut(|m| { let t = ui.ctx().input(|i| i.time); });", True),
    "through a helper": ("fn peek(ctx: &Context) -> f64 { ctx.input(|i| i.time) }\nfn go(ctx: &Context) { ctx.data_mut(|d| d.insert_temp(k, peek(ctx))); }", True),
    "two helpers deep": ("fn a(ctx: &Context) { ctx.request_repaint(); }\nfn b(ctx: &Context) { a(ctx) }\nfn go(ctx: &Context) { ctx.memory_mut(|m| b(x)); }", True),
    "context under another name": ("fn go(c: &Context) { c.data_mut(|d| d.insert_temp(k, 1)); }\nfn go2(c: &Context, ui: &Ui) { ui.ctx().memory_mut(|m| c.request_repaint()); }", True),
    "fonts and options": ("ctx.fonts_mut(|f| ctx.set_fonts(x)); ctx.options_mut(|o| ui.ctx().style());", True),
    "read first": ("let t = ctx.input(|i| i.time); ctx.data_mut(|d| d.insert_temp(key, t));", False),
    "plain closure": ("ctx.data(|d| d.get_temp::<f32>(id).unwrap_or(0.0));", False),
    "in a string": ('ctx.data(|d| d.get_temp::<f32>(id)); let s = "ctx.data(|d| ctx.input(|i| 1))";', False),
}


def self_test():
    bad = 0
    for name, (code, expect) in SAMPLES.items():
        got = bool(scan([("sample.rs", blank(code))]))
        if got != expect:
            print(f"self-test: {name}: expected {'a finding' if expect else 'none'}, got {'one' if got else 'none'}")
            bad += 1
    return bad


def main():
    if "--self-test" in sys.argv:
        return 1 if self_test() else 0
    bad = self_test()
    found = scan(load(ROOT))
    for path, line, why in found:
        print(f"{path}:{line}: {why}")
    if found:
        print("egui's context lock is not reentrant: read what the closure needs before taking the lock, or after.")
    return 1 if found or bad else 0


if __name__ == "__main__":
    sys.exit(main())
