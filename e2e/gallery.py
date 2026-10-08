"""Shrink chosen screenshots into phone-sized JPEGs and write a gallery page.

    python e2e/gallery.py out_dir "caption|path" "caption|path" ...

The page references the JPEGs by absolute path; viewers that inline local files
(like T3's html_render) turn it into a single self-contained document.
"""

import html
import sys
from pathlib import Path

from PIL import Image


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    items = []
    for i, arg in enumerate(sys.argv[2:]):
        caption, path = arg.split("|", 1)
        src = Path(path)
        if not src.exists():
            print("missing", src)
            continue
        img = Image.open(src).convert("RGB")
        img.thumbnail((960, 960))
        dst = out / f"{i:02d}_{src.stem}.jpg"
        img.save(dst, quality=78, optimize=True)
        items.append((caption, dst.resolve().as_posix()))
    figs = "\n".join(
        f'<figure><img src="{p}" alt=""><figcaption>{html.escape(c)}</figcaption></figure>' for c, p in items
    )
    page = f"""<!doctype html><html><head><meta charset="utf-8"><style>
figure {{ margin: 0 0 18px 0; }}
img {{ width: 100%; height: auto; display: block; border-radius: 8px; border: 1px solid var(--border); }}
figcaption {{ font: 13px/1.4 var(--font-sans); color: var(--muted-foreground); margin-top: 6px; }}
</style></head><body>{figs}</body></html>"""
    (out / "gallery.html").write_text(page, encoding="utf-8")
    print(out / "gallery.html", len(items))


if __name__ == "__main__":
    main()
