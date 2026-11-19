"""Copies eframe from the cargo registry into vendor/eframe and adds the d3d11 renderer next to glow and wgpu.

    python tools/fork_eframe.py <eframe version>

The d3d11 integration is made from eframe's wgpu one with the painter swapped; the painter itself,
vendor/eframe/src/native/d3d11.rs and d3d11.hlsl, is ours and kept across a rerun.
"""
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = sys.argv[1]
SRC = next((Path.home() / ".cargo" / "registry" / "src").glob(f"*/eframe-{VERSION}"))
DST = ROOT / "vendor" / "eframe"
OURS = ["src/native/d3d11.rs", "src/native/d3d11.hlsl", "src/native/d3d11_vs.cso", "src/native/d3d11_ps.cso"]

kept = {p: (DST / p).read_bytes() for p in OURS if (DST / p).exists()}
shutil.rmtree(DST, ignore_errors=True)
shutil.copytree(SRC, DST, ignore=shutil.ignore_patterns(".cargo-ok", ".cargo_vcs_info.json", "Cargo.lock", "Cargo.toml.orig"), dirs_exist_ok=True)
for p, text in kept.items():
    (DST / p).write_bytes(text)


def edit(path, *pairs, count=1):
    f = DST / path
    s = f.read_text(encoding="utf-8")
    for a, b in pairs:
        n = s.count(a)
        if n != count and count != 0:
            raise SystemExit(f"{path}: expected {count} of {a!r}, found {n}")
        s = s.replace(a, b)
    f.write_text(s, encoding="utf-8")


ANY = 'any(feature = "glow", feature = "wgpu_no_default_features")'
for f in ["src/epi.rs", "src/lib.rs", "src/native/run.rs"]:
    edit(f, (ANY, 'any(feature = "glow", feature = "wgpu_no_default_features", feature = "d3d11")'), count=0)

edit("Cargo.toml", ("[features]\n", '[features]\nd3d11 = ["dep:windows"]\n'),
     ("[target.'cfg(any(target_os = \"windows\"))'.dependencies.windows-sys]",
      "[target.'cfg(any(target_os = \"windows\"))'.dependencies.windows]\n"
      'version = "0.62.2"\n'
      'features = ["Win32_Foundation", "Win32_Graphics_Direct3D", "Win32_Graphics_Direct3D11", "Win32_Graphics_Dxgi", "Win32_Graphics_Dxgi_Common", "Win32_System_Threading"]\n'
      "optional = true\n\n"
      "[target.'cfg(any(target_os = \"windows\"))'.dependencies.windows-sys]"))

edit("src/epi.rs",
     ("""    /// Use [`egui_wgpu`] renderer for [`wgpu`](https://github.com/gfx-rs/wgpu).
    #[cfg(feature = "wgpu_no_default_features")]
    Wgpu,
}""", """    /// Use [`egui_wgpu`] renderer for [`wgpu`](https://github.com/gfx-rs/wgpu).
    #[cfg(feature = "wgpu_no_default_features")]
    Wgpu,

    /// Direct3D 11, Windows only.
    #[cfg(feature = "d3d11")]
    D3d11,
}"""),
     ("""    fn default() -> Self {
        #[cfg(not(feature = "glow"))]
        #[cfg(not(feature = "wgpu_no_default_features"))]
        compile_error!""", """    fn default() -> Self {
        #[cfg(feature = "d3d11")]
        return Self::D3d11;

        #[cfg(not(feature = "glow"))]
        #[cfg(not(feature = "wgpu_no_default_features"))]
        #[cfg(not(feature = "d3d11"))]
        compile_error!"""),
     ("""        #[cfg(feature = "glow")]
        #[cfg(not(feature = "wgpu_no_default_features"))]
        return Self::Glow;

        #[cfg(not(feature = "glow"))]
        #[cfg(feature = "wgpu_no_default_features")]
        return Self::Wgpu;

        // It's weird that the user has enabled both glow and wgpu,
        // but let's pick the better of the two (wgpu):
        #[cfg(feature = "glow")]
        #[cfg(feature = "wgpu_no_default_features")]
        return Self::Wgpu;""", """        #[cfg(not(feature = "d3d11"))]
        #[cfg(feature = "glow")]
        #[cfg(not(feature = "wgpu_no_default_features"))]
        return Self::Glow;

        #[cfg(not(feature = "d3d11"))]
        #[cfg(not(feature = "glow"))]
        #[cfg(feature = "wgpu_no_default_features")]
        return Self::Wgpu;

        // It's weird that the user has enabled both glow and wgpu,
        // but let's pick the better of the two (wgpu):
        #[cfg(not(feature = "d3d11"))]
        #[cfg(feature = "glow")]
        #[cfg(feature = "wgpu_no_default_features")]
        return Self::Wgpu;"""),
     ("""            #[cfg(feature = "wgpu_no_default_features")]
            Self::Wgpu => "wgpu".fmt(f),
""", """            #[cfg(feature = "wgpu_no_default_features")]
            Self::Wgpu => "wgpu".fmt(f),

            #[cfg(feature = "d3d11")]
            Self::D3d11 => "d3d11".fmt(f),
"""),
     ("""            #[cfg(feature = "wgpu_no_default_features")]
            "wgpu" => Ok(Self::Wgpu),
""", """            #[cfg(feature = "wgpu_no_default_features")]
            "wgpu" => Ok(Self::Wgpu),

            #[cfg(feature = "d3d11")]
            "d3d11" => Ok(Self::D3d11),
"""))

edit("src/lib.rs",
     ("""        #[cfg(feature = "wgpu_no_default_features")]
        Renderer::Wgpu => {
            log::debug!("Using the wgpu renderer");
            native::run::run_wgpu(app_name, native_options, egui_ctx, app_creator)
        }""", """        #[cfg(feature = "wgpu_no_default_features")]
        Renderer::Wgpu => {
            log::debug!("Using the wgpu renderer");
            native::run::run_wgpu(app_name, native_options, egui_ctx, app_creator)
        }

        #[cfg(feature = "d3d11")]
        Renderer::D3d11 => native::run::run_d3d11(app_name, native_options, egui_ctx, app_creator),"""),
     ("""    /// An error from [`wgpu`].
    #[cfg(feature = "wgpu_no_default_features")]
    Wgpu(egui_wgpu::WgpuError),
}""", """    /// An error from [`wgpu`].
    #[cfg(feature = "wgpu_no_default_features")]
    Wgpu(egui_wgpu::WgpuError),

    /// An error from Direct3D 11.
    #[cfg(feature = "d3d11")]
    D3d11(windows::core::Error),
}"""),
     ("""            #[cfg(feature = "wgpu_no_default_features")]
            Self::Wgpu(err) => {
                write!(f, "WGPU error: {err}")
            }
""", """            #[cfg(feature = "wgpu_no_default_features")]
            Self::Wgpu(err) => {
                write!(f, "WGPU error: {err}")
            }

            #[cfg(feature = "d3d11")]
            Self::D3d11(err) => write!(f, "Direct3D 11 error: {err}"),
"""))

s = (DST / "src/lib.rs").read_text(encoding="utf-8")
m = re.search(r"\n        #\[cfg\(feature = \"wgpu_no_default_features\"\)\]\n        Renderer::Wgpu => \{\n            log::debug!\(\"Using the wgpu renderer\"\);\n            EframeWinitApplication::new\(native::run::create_wgpu\((.|\n)*?\)\)\n        \}", s)
if not m:
    raise SystemExit("lib.rs: create_wgpu arm not found")
arm = m.group(0)
d3d = arm.replace('feature = "wgpu_no_default_features"', 'feature = "d3d11"').replace("Renderer::Wgpu", "Renderer::D3d11").replace("create_wgpu", "create_d3d11").replace('            log::debug!("Using the wgpu renderer");\n', "")
(DST / "src/lib.rs").write_text(s.replace(arm, arm + "\n" + d3d), encoding="utf-8")

s = (DST / "src/native/run.rs").read_text(encoding="utf-8")
start = s.index('#[cfg(feature = "wgpu_no_default_features")]\npub fn run_wgpu(')
end = s.index("// ----", s.index("pub fn create_wgpu"))
block = s[start:end]
d3d = (block.replace('feature = "wgpu_no_default_features"', 'feature = "d3d11"').replace("run_wgpu", "run_d3d11").replace("create_wgpu", "create_d3d11")
       .replace("wgpu_integration::WgpuWinitApp", "d3d11_integration::D3d11WinitApp").replace("WgpuWinitApp", "D3d11WinitApp").replace("wgpu_eframe", "d3d11_eframe"))
(DST / "src/native/run.rs").write_text(s[:end] + d3d + s[end:], encoding="utf-8")

edit("src/native/mod.rs", ("""#[cfg(feature = "wgpu_no_default_features")]
mod wgpu_integration;""", """#[cfg(feature = "wgpu_no_default_features")]
mod wgpu_integration;

#[cfg(feature = "d3d11")]
pub(crate) mod d3d11;

#[cfg(feature = "d3d11")]
mod d3d11_integration;"""))

edit("src/lib.rs", ("""#[cfg(feature = "wgpu_no_default_features")]
pub use {egui_wgpu, egui_wgpu::SurfaceConfig, egui_wgpu::WgpuConfiguration, egui_wgpu::wgpu};""", """#[cfg(feature = "wgpu_no_default_features")]
pub use {egui_wgpu, egui_wgpu::SurfaceConfig, egui_wgpu::WgpuConfiguration, egui_wgpu::wgpu};

#[cfg(feature = "d3d11")]
pub use native::d3d11::{device_resets, lose_device};"""))
s = (DST / "src/native/wgpu_integration.rs").read_text(encoding="utf-8")
start = s.index("        // Inject the display handle into the wgpu setup")
end = s.index("        let mut viewport_info = ViewportInfo::default();")
s = s[:start] + "        let mut painter = super::d3d11::Painter::new(egui_ctx.clone(), self.native_options.dithering).map_err(crate::Error::D3d11)?;\n\n" + s[end:]
for a, b in [
    ("egui_wgpu::winit::Painter", "super::d3d11::Painter"),
    ("WgpuWinitApp", "D3d11WinitApp"),
    ("WgpuWinitRunning", "D3d11WinitRunning"),
    ("pollster::block_on(shared.painter.set_window(ViewportId::ROOT, None))?;", "shared.painter.set_window(ViewportId::ROOT, None).map_err(crate::Error::D3d11)?;"),
    ("pollster::block_on(painter.set_window(ViewportId::ROOT, Some(Arc::clone(&window))))?;", "painter.set_window(ViewportId::ROOT, Some(Arc::clone(&window))).map_err(crate::Error::D3d11)?;"),
    ("pollster::block_on(painter.set_window(viewport_id, Some(Arc::clone(window))))?;", "painter.set_window(viewport_id, Some(Arc::clone(window))).map_err(crate::Error::D3d11)?;"),
    ("pollster::block_on(painter.set_window(viewport_id, Some(Arc::clone(&window))))", "painter.set_window(viewport_id, Some(Arc::clone(&window)))"),
    ("pollster::block_on(painter.set_window(ids.this, Some(Arc::clone(window))))", "painter.set_window(ids.this, Some(Arc::clone(window)))"),
    ("pollster::block_on(painter.set_window(viewport.ids.this, None))", "painter.set_window(viewport.ids.this, None)"),
    ("        let wgpu_render_state = painter.render_state();\n", ""),
    ("            wgpu_render_state.clone(),\n", "            #[cfg(feature = \"wgpu_no_default_features\")]\n            None,\n"),
    ("            wgpu_render_state,\n", "            #[cfg(feature = \"wgpu_no_default_features\")]\n            wgpu_render_state: None,\n"),
    ("    fn drop_window(&mut self) -> Result<(), egui_wgpu::WgpuError> {", "    fn drop_window(&mut self) -> crate::Result<()> {"),
]:
    if a not in s:
        raise SystemExit(f"wgpu_integration.rs: {a!r} not found")
    s = s.replace(a, b)
if "pollster" in s or "egui_wgpu" in s or "wgpu_options" in s:
    raise SystemExit("wgpu_integration.rs: wgpu left in the d3d11 copy")
(DST / "src/native/d3d11_integration.rs").write_text(s, encoding="utf-8")
print("vendor/eframe forked from", SRC)
