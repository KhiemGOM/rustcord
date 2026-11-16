use resvg::{tiny_skia, usvg};

/// assets/rustcord.svg as an .ico with a PNG at each size Windows asks for.
fn ico(svg: &str) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("assets/rustcord.svg doesn't parse");
    let pngs: Vec<(u32, Vec<u8>)> = [16, 20, 24, 32, 40, 48, 64, 256]
        .into_iter()
        .map(|px| {
            let mut pix = tiny_skia::Pixmap::new(px, px).unwrap();
            let s = px as f32 / tree.size().width();
            resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pix.as_mut());
            (px, pix.encode_png().unwrap())
        })
        .collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend((pngs.len() as u16).to_le_bytes());
    let mut at = 6 + 16 * pngs.len();
    for (px, png) in &pngs {
        let side = if *px >= 256 { 0 } else { *px as u8 };
        out.extend([side, side, 0, 0, 1, 0, 32, 0]);
        out.extend((png.len() as u32).to_le_bytes());
        out.extend((at as u32).to_le_bytes());
        at += png.len();
    }
    for (_, png) in pngs {
        out.extend(png);
    }
    out
}

fn main() {
    println!("cargo:rerun-if-changed=assets/rustcord.svg");
    println!("cargo:rerun-if-changed=assets/rustcord.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("rustcord.ico");
        std::fs::write(&icon, ico(&std::fs::read_to_string("assets/rustcord.svg").unwrap())).unwrap();
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon.to_str().unwrap());
        // the segment heap gives memory back after READY's burst; the old heap kept ~100 MB of it
        res.set_manifest_file("assets/rustcord.manifest");
        res.compile().expect("couldn't embed the icon");
    }
}
