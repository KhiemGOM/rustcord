//! Rustcord's logo, drawn from assets/rustcord.svg at whatever size is asked for: the window, the
//! toasts, the tray and its voice faces. build.rs draws the exe's icon from the same file.

use resvg::{tiny_skia, usvg};

pub const SVG: &str = include_str!("../assets/rustcord.svg");

/// The logo with `over` drawn on top, its own shapes cut away under a `hole` (x, y, r) so a
/// status dot stays readable on the orange.
pub fn with(over: &str, hole: Option<(f32, f32, f32)>) -> String {
    let body = SVG.trim_end().strip_suffix("</svg>").unwrap_or(SVG);
    let Some((head, rest)) = body.split_once('>') else { return SVG.into() };
    match hole {
        Some((x, y, r)) => format!(
            r##"{head}><mask id="hole"><rect width="256" height="256" fill="#fff"/><circle cx="{x}" cy="{y}" r="{r}"/></mask><g mask="url(#hole)">{rest}</g>{over}</svg>"##
        ),
        None => format!("{body}{over}</svg>"),
    }
}

/// `svg` drawn `px` pixels square, premultiplied RGBA.
pub fn pixmap(svg: &str, px: u32) -> Option<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pix = tiny_skia::Pixmap::new(px, px)?;
    let s = px as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pix.as_mut());
    Some(pix)
}

/// Straight RGBA, which is what egui's window icon wants.
pub fn rgba(px: u32) -> Option<egui::IconData> {
    let pix = pixmap(SVG, px)?;
    let rgba = pix.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Some(egui::IconData { width: px, height: px, rgba: rgba.collect() })
}

pub fn png(svg: &str, px: u32) -> Option<Vec<u8>> {
    pixmap(svg, px)?.encode_png().ok()
}

/// `svg` as an icon `px` pixels across.
pub fn hicon(svg: &str, px: i32) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, LR_DEFAULTCOLOR};
    let png = png(svg, px as u32)?;
    let h = unsafe { CreateIconFromResourceEx(png.as_ptr(), png.len() as u32, 1, 0x00030000, px, px, LR_DEFAULTCOLOR) };
    (!h.is_null()).then_some(h)
}
