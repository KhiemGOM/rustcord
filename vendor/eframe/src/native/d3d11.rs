//! egui on Direct3D 11: one device for every viewport, a flip-model swapchain per window.
//! Draws exactly as egui-wgpu does into a gamma-space framebuffer (`d3d11.hlsl`).
//! Each texture keeps its pixels (the font atlas's are egui's), so when the device is lost (a
//! driver update or reset) the painter makes a new one and puts every window and texture back
//! without the app's help.
#![allow(unsafe_code)]

use std::{
    cell::RefCell,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};

use ahash::HashMap;
use egui::{ColorImage, TextureId, UserData, ViewportId, ViewportIdSet, epaint};
use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND, RECT},
        Graphics::{
            Direct3D::{
                D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0,
                D3D_FEATURE_LEVEL_11_1, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            },
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
    },
    core::{Interface as _, PCSTR, s},
};

// tools/hlsl.py makes these from d3d11.hlsl
const VS: &[u8] = include_bytes!("d3d11_vs.cso");
const PS: &[u8] = include_bytes!("d3d11_ps.cso");
const FONT: &[u8] = include_bytes!("d3d11_font.cso");
const MAX_TEXTURE_SIDE: usize = 8192;
// Measured on an AMD card: a texture of its own costs the process twice its pixels plus 25-40 KB,
// for good (the driver keeps what it once took), and one made to be shared costs its pixels once
// but takes five times as long to make. So small images (most of a chat: avatars, emoji, icons)
// are tiles on shared pages, where one costs its pixels and no texture is made for it.
const PAGE: u32 = 1024;
const TILE: usize = 128;

pub type Error = windows::core::Error;

static LOSE: AtomicBool = AtomicBool::new(false);
static RESETS: AtomicU32 = AtomicU32::new(0);
static RELOADABLE: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<TextureId>>> = std::sync::LazyLock::new(Default::default);

/// A texture an image loader made and can make again. The painter keeps no copy of its pixels
/// for a lost device; it has the loaders forget their images instead, and they load them anew.
pub fn reloadable(id: TextureId) {
    RELOADABLE.lock().unwrap().insert(id);
}

/// Acts on the next frame as if the device had been lost, for a test to see the recovery.
pub fn lose_device() {
    LOSE.store(true, Relaxed);
}

/// How many times the painter has made a new device after losing one.
pub fn device_resets() -> u32 {
    RESETS.load(Relaxed)
}

struct Surface {
    hwnd: HWND,
    swapchain: IDXGISwapChain1,
    target: Option<ID3D11RenderTargetView>,
    size: [u32; 2],
}

enum Place {
    Own(ID3D11Texture2D, ID3D11ShaderResourceView),
    // the glyph atlas while it is white at some coverage and nothing else: a quarter the memory
    Mono(ID3D11Texture2D, ID3D11ShaderResourceView),
    // the slot's corner on its page; the image sits one pixel in, inside a border of its own edge
    Tile { page: usize, x: u32, y: u32 },
}

struct Shelf {
    y: u32,
    height: u32,
    right: u32,
    // slots given back: where, and how wide
    free: Vec<(u32, u32)>,
}

struct Page {
    texture: ID3D11Texture2D,
    view: ID3D11ShaderResourceView,
    shelves: Vec<Shelf>,
    bottom: u32,
    tiles: u32,
}

#[derive(Default)]
struct Atlas {
    // a page that empties goes, and its place is taken by the next one made
    pages: Vec<Option<Page>>,
}

/// A tile's slot side for an image side: room for the border, in steps of 8 so slots get reused.
fn slot(side: usize) -> u32 {
    ((side + 2 + 7) & !7) as u32
}

impl Page {
    fn take(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        for s in self.shelves.iter_mut().filter(|s| s.height == h) {
            if let Some(i) = s.free.iter().position(|f| f.1 == w) {
                return Some((s.free.swap_remove(i).0, s.y));
            }
            if s.right + w <= PAGE {
                s.right += w;
                return Some((s.right - w, s.y));
            }
        }
        if self.bottom + h > PAGE {
            return None;
        }
        self.shelves.push(Shelf { y: self.bottom, height: h, right: w, free: Vec::new() });
        self.bottom += h;
        Some((0, self.bottom - h))
    }
}

impl Atlas {
    fn take(&mut self, gpu: &Gpu, size: [usize; 2]) -> windows::core::Result<Place> {
        let (w, h) = (slot(size[0]), slot(size[1]));
        for (page, p) in self.pages.iter_mut().enumerate() {
            if let Some(p) = p
                && let Some((x, y)) = p.take(w, h)
            {
                p.tiles += 1;
                return Ok(Place::Tile { page, x, y });
            }
        }
        let (texture, view) = gpu.texture([PAGE as usize; 2], None)?;
        let mut p = Page { texture, view, shelves: Vec::new(), bottom: 0, tiles: 1 };
        let (x, y) = p.take(w, h).ok_or_else(Error::empty)?;
        let page = self.pages.iter().position(Option::is_none).unwrap_or(self.pages.len());
        if page == self.pages.len() {
            self.pages.push(None);
        }
        self.pages[page] = Some(p);
        Ok(Place::Tile { page, x, y })
    }

    fn give(&mut self, page: usize, x: u32, y: u32, size: [usize; 2]) {
        let Some(p) = self.pages.get_mut(page).and_then(Option::as_mut) else {
            return;
        };
        p.tiles -= 1;
        if p.tiles > 0 {
            if let Some(s) = p.shelves.iter_mut().find(|s| s.y == y) {
                s.free.push((x, slot(size[0])));
            }
            return;
        }
        // empty, its shelves start over; one empty page is kept for the next screenful
        p.shelves.clear();
        p.bottom = 0;
        if self.pages.iter().flatten().filter(|p| p.tiles == 0).count() > 1 {
            self.pages[page] = None;
        }
    }

    /// `image` into a tile of `size` at `pos`: the whole of it, or a patch. Where it reaches the
    /// tile's edge the border, which repeats that edge, is written with it.
    fn write(&self, gpu: &Gpu, page: usize, x: u32, y: u32, size: [usize; 2], pos: [usize; 2], image: &ColorImage) {
        let Some(p) = self.pages.get(page).and_then(Option::as_ref) else {
            return;
        };
        let [w, h] = image.size;
        let cols = pos[0] + usize::from(pos[0] > 0)..pos[0] + w + 1 + usize::from(pos[0] + w == size[0]);
        let rows = pos[1] + usize::from(pos[1] > 0)..pos[1] + h + 1 + usize::from(pos[1] + h == size[1]);
        let mut edged = Vec::with_capacity(cols.len() * rows.len());
        for row in rows.clone() {
            let from = &image.pixels[((row.max(1) - 1).min(size[1] - 1) - pos[1]) * w..][..w];
            edged.extend(cols.clone().map(|col| from[(col.max(1) - 1).min(size[0] - 1) - pos[0]]));
        }
        let area = D3D11_BOX { left: x + cols.start as u32, top: y + rows.start as u32, front: 0, right: x + cols.end as u32, bottom: y + rows.end as u32, back: 1 };
        unsafe { gpu.ctx.UpdateSubresource(&p.texture, 0, Some(&area), edged.as_ptr().cast(), (cols.len() * 4) as u32, 0) };
    }
}

/// Whether an image is drawn from a tile: small, and not repeated past its edge.
fn tiled(id: TextureId, size: [usize; 2], options: epaint::textures::TextureOptions) -> bool {
    id != TextureId::default() && size[0] <= TILE && size[1] <= TILE && options.wrap_mode == epaint::textures::TextureWrapMode::ClampToEdge
}

struct Texture {
    // None while the device is lost
    gpu: Option<Place>,
    options: epaint::textures::TextureOptions,
    size: [usize; 2],
    // None for the font atlas, which egui keeps whole, and for what a loader makes again
    image: Option<Arc<ColorImage>>,
}

struct Buffer {
    buffer: ID3D11Buffer,
    capacity: usize,
}

/// Everything that belongs to one device and goes when it's lost.
struct Gpu {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    factory: IDXGIFactory2,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    font: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    blend: ID3D11BlendState,
    raster: ID3D11RasterizerState,
    locals: ID3D11Buffer,
    // whether textures are still made to be shared: one that couldn't be ends it for this device
    shared: std::cell::Cell<bool>,
}

pub struct Painter {
    egui_ctx: egui::Context,
    gpu: Gpu,
    vertices: Option<Buffer>,
    indices: Option<Buffer>,
    surfaces: HashMap<ViewportId, Surface>,
    textures: HashMap<TextureId, Texture>,
    atlas: Atlas,
    samplers: HashMap<epaint::textures::TextureOptions, ID3D11SamplerState>,
    dithering: bool,
    lost: bool,
    // windows waiting for a swapchain on the new device
    orphans: HashMap<ViewportId, (HWND, [u32; 2])>,
    captured: RefCell<Vec<(ViewportId, Vec<UserData>, Arc<ColorImage>)>>,
}

fn device(kind: D3D_DRIVER_TYPE) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0];
    let (mut device, mut ctx) = (None, None);
    unsafe {
        D3D11CreateDevice(None, kind, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, Some(&levels), D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx))?;
    }
    Ok((device.ok_or_else(Error::empty)?, ctx.ok_or_else(Error::empty)?))
}

impl Gpu {
    fn new() -> windows::core::Result<Self> {
        let (device, ctx) = device(D3D_DRIVER_TYPE_HARDWARE).or_else(|err| {
            log::warn!("no Direct3D 11 hardware device ({err}), drawing with WARP");
            device(D3D_DRIVER_TYPE_WARP)
        })?;
        let dxgi: IDXGIDevice1 = device.cast()?;
        unsafe { dxgi.SetMaximumFrameLatency(2)? };
        let factory: IDXGIFactory2 = unsafe { dxgi.GetAdapter()?.GetParent()? };
        let element = |name: PCSTR, format, offset| D3D11_INPUT_ELEMENT_DESC {
            SemanticName: name,
            SemanticIndex: 0,
            Format: format,
            InputSlot: 0,
            AlignedByteOffset: offset,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        };
        let elements = [
            element(s!("POSITION"), DXGI_FORMAT_R32G32_FLOAT, 0),
            element(s!("TEXCOORD"), DXGI_FORMAT_R32G32_FLOAT, 8),
            element(s!("COLOR"), DXGI_FORMAT_R8G8B8A8_UNORM, 16),
        ];
        let (mut vs, mut ps, mut font, mut layout, mut blend, mut raster, mut locals) = (None, None, None, None, None, None, None);
        unsafe {
            device.CreateVertexShader(VS, None, Some(&mut vs))?;
            device.CreatePixelShader(PS, None, Some(&mut ps))?;
            device.CreatePixelShader(FONT, None, Some(&mut font))?;
            device.CreateInputLayout(&elements, VS, Some(&mut layout))?;
            let mut b = D3D11_BLEND_DESC::default();
            b.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_INV_DEST_ALPHA,
                DestBlendAlpha: D3D11_BLEND_ONE,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            device.CreateBlendState(&b, Some(&mut blend))?;
            let r = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                ScissorEnable: true.into(),
                DepthClipEnable: true.into(),
                ..Default::default()
            };
            device.CreateRasterizerState(&r, Some(&mut raster))?;
            let l = D3D11_BUFFER_DESC {
                ByteWidth: 16,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            device.CreateBuffer(&l, None, Some(&mut locals))?;
        }
        Ok(Self {
            device,
            ctx,
            factory,
            vs: vs.ok_or_else(Error::empty)?,
            ps: ps.ok_or_else(Error::empty)?,
            font: font.ok_or_else(Error::empty)?,
            layout: layout.ok_or_else(Error::empty)?,
            blend: blend.ok_or_else(Error::empty)?,
            raster: raster.ok_or_else(Error::empty)?,
            locals: locals.ok_or_else(Error::empty)?,
            shared: std::cell::Cell::new(true),
        })
    }

    fn swapchain(&self, hwnd: HWND, size: [u32; 2]) -> windows::core::Result<IDXGISwapChain1> {
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: size[0],
            Height: size[1],
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            ..Default::default()
        };
        let swapchain = unsafe { self.factory.CreateSwapChainForHwnd(&self.device, hwnd, &desc, None, None)? };
        unsafe { self.factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)? };
        Ok(swapchain)
    }

    /// `pixels` are four bytes each, or one for a texture of coverage alone.
    fn texture(&self, [w, h]: [usize; 2], pixels: Option<&[u8]>) -> windows::core::Result<(ID3D11Texture2D, ID3D11ShaderResourceView)> {
        let mono = pixels.is_some_and(|p| p.len() == w * h);
        let mut desc = D3D11_TEXTURE2D_DESC {
            Width: w as u32,
            Height: h as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: if mono { DXGI_FORMAT_R8_UNORM } else { DXGI_FORMAT_R8G8B8A8_UNORM },
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            MiscFlags: if self.shared.get() { D3D11_RESOURCE_MISC_SHARED.0 as u32 } else { 0 },
            ..Default::default()
        };
        let data = pixels.map(|p| D3D11_SUBRESOURCE_DATA { pSysMem: p.as_ptr().cast(), SysMemPitch: (w * if mono { 1 } else { 4 }) as u32, SysMemSlicePitch: 0 });
        let data = data.as_ref().map(|d| d as *const _);
        let (mut texture, mut view) = (None, None);
        unsafe {
            let mut made = self.device.CreateTexture2D(&desc, data, Some(&mut texture));
            if made.is_err() && self.shared.replace(false) {
                desc.MiscFlags = 0;
                made = self.device.CreateTexture2D(&desc, data, Some(&mut texture));
            }
            made?;
            let texture = texture.as_ref().ok_or_else(Error::empty)?;
            self.device.CreateShaderResourceView(texture, None, Some(&mut view))?;
        }
        Ok((texture.ok_or_else(Error::empty)?, view.ok_or_else(Error::empty)?))
    }

    fn place(&self, atlas: &mut Atlas, id: TextureId, image: &ColorImage, options: epaint::textures::TextureOptions) -> windows::core::Result<Place> {
        if id == TextureId::default()
            && let Some(c) = coverage(&image.pixels)
        {
            let (texture, view) = self.texture(image.size, Some(&c))?;
            return Ok(Place::Mono(texture, view));
        }
        if !tiled(id, image.size, options) {
            let (texture, view) = self.texture(image.size, Some(raw(&image.pixels)))?;
            return Ok(Place::Own(texture, view));
        }
        let place = atlas.take(self, image.size)?;
        if let Place::Tile { page, x, y } = place {
            atlas.write(self, page, x, y, image.size, [0, 0], image);
        }
        Ok(place)
    }
}

/// Pixels as coverage alone, if every one of them is white at some alpha, as glyphs are.
fn coverage(pixels: &[egui::Color32]) -> Option<Vec<u8>> {
    pixels.iter().map(|c| (c.r() == c.a() && c.g() == c.a() && c.b() == c.a()).then_some(c.a())).collect()
}

fn removed(err: &Error) -> bool {
    err.code() == DXGI_ERROR_DEVICE_REMOVED || err.code() == DXGI_ERROR_DEVICE_RESET
}

impl Painter {
    pub fn new(egui_ctx: egui::Context, dithering: bool) -> Result<Self, Error> {
        Ok(Self {
            egui_ctx,
            gpu: Gpu::new()?,
            vertices: None,
            indices: None,
            surfaces: Default::default(),
            textures: Default::default(),
            atlas: Default::default(),
            samplers: Default::default(),
            dithering,
            lost: false,
            orphans: Default::default(),
            captured: Default::default(),
        })
    }

    pub fn max_texture_side(&self) -> Option<usize> {
        Some(MAX_TEXTURE_SIDE)
    }

    pub fn set_window(&mut self, viewport_id: ViewportId, window: Option<Arc<winit::window::Window>>) -> Result<(), Error> {
        let Some(window) = window else {
            self.surfaces.remove(&viewport_id);
            return Ok(());
        };
        if self.surfaces.contains_key(&viewport_id) {
            return Ok(());
        }
        let Ok(RawWindowHandle::Win32(handle)) = window.window_handle().map(|h| h.as_raw()) else {
            return Err(Error::empty());
        };
        let hwnd = HWND(handle.hwnd.get() as _);
        let size = window.inner_size();
        let size = [size.width.max(1), size.height.max(1)];
        let swapchain = self.gpu.swapchain(hwnd, size)?;
        self.surfaces.insert(viewport_id, Surface { hwnd, swapchain, target: None, size });
        Ok(())
    }

    pub fn on_window_resize_state_change(&mut self, _viewport_id: ViewportId, _resizing: bool) {}

    pub fn on_window_resized(&mut self, viewport_id: ViewportId, width: NonZeroU32, height: NonZeroU32) {
        let Some(surface) = self.surfaces.get_mut(&viewport_id) else {
            return;
        };
        let size = [width.get(), height.get()];
        if surface.size == size {
            return;
        }
        surface.target = None;
        unsafe { self.gpu.ctx.OMSetRenderTargets(None, None) };
        if let Err(err) = unsafe { surface.swapchain.ResizeBuffers(0, size[0], size[1], DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0)) } {
            log::error!("ResizeBuffers: {err}");
            self.lost |= removed(&err);
        }
        surface.size = size;
    }

    pub fn handle_screenshots(&self, events: &mut Vec<egui::Event>) {
        for (viewport_id, user_data, image) in self.captured.borrow_mut().drain(..) {
            for data in user_data {
                events.push(egui::Event::Screenshot { viewport_id, user_data: data, image: Arc::clone(&image) });
            }
        }
    }

    pub fn gc_viewports(&mut self, active: &ViewportIdSet) {
        self.surfaces.retain(|id, _| active.contains(id));
        self.orphans.retain(|id, _| active.contains(id));
    }

    pub fn destroy(&mut self) {
        self.surfaces.clear();
        self.textures.clear();
        self.atlas = Atlas::default();
        unsafe { self.gpu.ctx.ClearState() };
    }

    #[expect(clippy::too_many_arguments)]
    pub fn paint_and_update_textures(
        &mut self,
        viewport_id: ViewportId,
        pixels_per_point: f32,
        clear_color: [f32; 4],
        clipped_primitives: &[epaint::ClippedPrimitive],
        textures_delta: &mut epaint::textures::TexturesDelta,
        capture: Vec<UserData>,
        _window: &Arc<winit::window::Window>,
    ) -> f32 {
        if LOSE.swap(false, Relaxed) || unsafe { self.gpu.device.GetDeviceRemovedReason() }.is_err() {
            self.lost = true;
        }
        if self.lost {
            if let Err(err) = self.recover() {
                log::error!("no new Direct3D 11 device yet: {err}");
                self.egui_ctx.request_repaint_after(Duration::from_secs(1));
            }
        }
        for (id, deltas) in textures_delta.set.drain() {
            for delta in deltas {
                if let Err(err) = self.update_texture(id, &delta) {
                    log::error!("texture {id:?}: {err}");
                    self.lost |= removed(&err);
                }
            }
        }
        let vsync = if self.lost {
            0.0
        } else {
            self.paint(viewport_id, pixels_per_point, clear_color, clipped_primitives, capture).unwrap_or_else(|err| {
                log::error!("paint: {err}");
                if removed(&err) {
                    self.lost = true;
                    self.egui_ctx.request_repaint();
                }
                0.0
            })
        };
        for id in textures_delta.free.drain() {
            self.forget(id);
        }
        vsync
    }

    fn forget(&mut self, id: TextureId) {
        RELOADABLE.lock().unwrap().remove(&id);
        if let Some(Texture { gpu: Some(Place::Tile { page, x, y }), size, .. }) = self.textures.remove(&id) {
            self.atlas.give(page, x, y, size);
        }
    }

    /// A new device, and on it every window's swapchain and every texture as they were.
    fn recover(&mut self) -> windows::core::Result<()> {
        log::warn!("the Direct3D 11 device was lost, making a new one");
        self.orphans.extend(self.surfaces.drain().map(|(id, s)| (id, (s.hwnd, s.size))));
        self.samplers.clear();
        self.vertices = None;
        self.indices = None;
        unsafe {
            self.gpu.ctx.ClearState();
            self.gpu.ctx.Flush();
        }
        // nothing points into the old device while the new one fills: a recovery that fails
        // halfway leaves blanks, and starts over on the next frame
        for t in self.textures.values_mut() {
            t.gpu = None;
        }
        self.atlas = Atlas::default();
        self.gpu = Gpu::new()?;
        for (id, t) in &mut self.textures {
            // what a loader made has no copy here; the loader sees the reset and makes it again
            let image = match &t.image {
                Some(image) => Arc::clone(image),
                None if *id == TextureId::default() => Arc::new(self.egui_ctx.fonts(|f| f.image())),
                None => continue,
            };
            t.gpu = Some(self.gpu.place(&mut self.atlas, *id, &image, t.options)?);
        }
        for (&id, &(hwnd, size)) in &self.orphans {
            let swapchain = self.gpu.swapchain(hwnd, size)?;
            self.surfaces.insert(id, Surface { hwnd, swapchain, target: None, size });
        }
        self.orphans.clear();
        self.lost = false;
        RESETS.fetch_add(1, Relaxed);
        self.egui_ctx.request_repaint();
        Ok(())
    }

    fn update_texture(&mut self, id: TextureId, delta: &epaint::ImageDelta) -> windows::core::Result<()> {
        let epaint::ImageData::Color(image) = &delta.image;
        let [w, h] = image.size;
        if w == 0 || h == 0 {
            return Ok(());
        }
        if let Some(pos) = delta.pos {
            let Some(t) = self.textures.get_mut(&id) else {
                return Ok(());
            };
            if let Some(whole) = &mut t.image {
                let whole = Arc::make_mut(whole);
                let width = whole.size[0];
                for (y, row) in image.pixels.chunks_exact(w).enumerate() {
                    let at = (pos[1] + y) * width + pos[0];
                    whole.pixels[at..at + w].copy_from_slice(row);
                }
            }
            let area = D3D11_BOX { left: pos[0] as u32, top: pos[1] as u32, front: 0, right: (pos[0] + w) as u32, bottom: (pos[1] + h) as u32, back: 1 };
            let mut coloured = false;
            match &t.gpu {
                Some(Place::Own(texture, _)) => unsafe { self.gpu.ctx.UpdateSubresource(texture, 0, Some(&area), image.pixels.as_ptr().cast(), (w * 4) as u32, 0) },
                Some(Place::Mono(texture, _)) => match coverage(&image.pixels) {
                    Some(c) => unsafe { self.gpu.ctx.UpdateSubresource(texture, 0, Some(&area), c.as_ptr().cast(), w as u32, 0) },
                    None => coloured = true,
                },
                Some(Place::Tile { page, x, y }) => self.atlas.write(&self.gpu, *page, *x, *y, t.size, pos, image),
                None => {}
            }
            t.options = delta.options;
            // a tile can't repeat past its edge: one that now must moves to a texture of its own
            if let (Some(&Place::Tile { page, x, y }), Some(whole)) = (t.gpu.as_ref(), t.image.clone())
                && !tiled(id, t.size, t.options)
            {
                self.atlas.give(page, x, y, t.size);
                t.gpu = None;
                t.gpu = Some(self.gpu.place(&mut self.atlas, id, &whole, t.options)?);
            }
            if coloured {
                // a glyph in colour: the atlas goes up again as it is, in full colour
                let whole = self.egui_ctx.fonts(|f| f.image());
                t.gpu = Some(self.gpu.place(&mut self.atlas, id, &whole, delta.options)?);
            }
            return Ok(());
        }
        if let Some(t) = self.textures.get_mut(&id)
            && let Some(place) = &t.gpu
            && t.size == image.size
            && matches!(place, Place::Tile { .. }) == tiled(id, image.size, delta.options)
            && !matches!(place, Place::Mono(..))
        {
            match place {
                Place::Own(texture, _) => unsafe { self.gpu.ctx.UpdateSubresource(texture, 0, None, image.pixels.as_ptr().cast(), (w * 4) as u32, 0) },
                Place::Mono(..) => {}
                &Place::Tile { page, x, y } => self.atlas.write(&self.gpu, page, x, y, image.size, [0, 0], image),
            }
            if t.image.is_some() {
                t.image = Some(Arc::clone(image));
            }
            t.options = delta.options;
            return Ok(());
        }
        let kept = id != TextureId::default() && !RELOADABLE.lock().unwrap().contains(&id);
        if let Some(Texture { gpu: Some(Place::Tile { page, x, y }), size, .. }) = self.textures.remove(&id) {
            self.atlas.give(page, x, y, size);
        }
        let made = self.gpu.place(&mut self.atlas, id, image, delta.options);
        let (gpu, made) = match made {
            Ok(place) => (Some(place), Ok(())),
            Err(err) => (None, Err(err)),
        };
        self.textures.insert(id, Texture { gpu, options: delta.options, size: image.size, image: kept.then(|| Arc::clone(image)) });
        made
    }

    fn sampler(&mut self, options: epaint::textures::TextureOptions) -> windows::core::Result<ID3D11SamplerState> {
        use epaint::textures::{TextureFilter::*, TextureWrapMode};
        if let Some(s) = self.samplers.get(&options) {
            return Ok(s.clone());
        }
        let filter = match (options.minification, options.magnification) {
            (Linear, Linear) => D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            (Nearest, Nearest) => D3D11_FILTER_MIN_MAG_MIP_POINT,
            (Nearest, Linear) => D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
            (Linear, Nearest) => D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
        };
        let address = match options.wrap_mode {
            TextureWrapMode::ClampToEdge => D3D11_TEXTURE_ADDRESS_CLAMP,
            TextureWrapMode::Repeat => D3D11_TEXTURE_ADDRESS_WRAP,
            TextureWrapMode::MirroredRepeat => D3D11_TEXTURE_ADDRESS_MIRROR,
        };
        let desc = D3D11_SAMPLER_DESC {
            Filter: filter,
            AddressU: address,
            AddressV: address,
            AddressW: address,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let mut s = None;
        unsafe { self.gpu.device.CreateSamplerState(&desc, Some(&mut s))? };
        let s = s.ok_or_else(Error::empty)?;
        self.samplers.insert(options, s.clone());
        Ok(s)
    }

    fn fill(&mut self, index: bool, len: usize, write: impl FnOnce(*mut u8)) -> windows::core::Result<ID3D11Buffer> {
        let slot = if index { &mut self.indices } else { &mut self.vertices };
        if slot.as_ref().is_none_or(|b| b.capacity < len) {
            let capacity = len.next_power_of_two().max(64 * 1024);
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: capacity as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: if index { D3D11_BIND_INDEX_BUFFER.0 } else { D3D11_BIND_VERTEX_BUFFER.0 } as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            let mut buffer = None;
            unsafe { self.gpu.device.CreateBuffer(&desc, None, Some(&mut buffer))? };
            *slot = Some(Buffer { buffer: buffer.ok_or_else(Error::empty)?, capacity });
        }
        let buffer = slot.as_ref().ok_or_else(Error::empty)?.buffer.clone();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.gpu.ctx.Map(&buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            write(mapped.pData.cast());
            self.gpu.ctx.Unmap(&buffer, 0);
        }
        Ok(buffer)
    }

    fn paint(&mut self, viewport_id: ViewportId, ppp: f32, clear_color: [f32; 4], prims: &[epaint::ClippedPrimitive], capture: Vec<UserData>) -> windows::core::Result<f32> {
        let Some(surface) = self.surfaces.get_mut(&viewport_id) else {
            return Ok(0.0);
        };
        if surface.target.is_none() {
            let back: ID3D11Texture2D = unsafe { surface.swapchain.GetBuffer(0)? };
            let mut target = None;
            unsafe { self.gpu.device.CreateRenderTargetView(&back, None, Some(&mut target))? };
            surface.target = target;
        }
        let size = surface.size;
        let target = surface.target.clone().ok_or_else(Error::empty)?;
        let swapchain = surface.swapchain.clone();

        let meshes: Vec<(&epaint::Rect, &epaint::Mesh)> = prims
            .iter()
            .filter_map(|p| match &p.primitive {
                epaint::Primitive::Mesh(m) => Some((&p.clip_rect, m)),
                epaint::Primitive::Callback(_) => None,
            })
            .collect();
        // what each mesh is drawn from, and for a tile where on the page its image starts and how far it spans
        let sources: Vec<Option<(ID3D11ShaderResourceView, epaint::textures::TextureOptions, Option<[f32; 4]>, bool)>> = meshes
            .iter()
            .map(|(_, m)| {
                let t = self.textures.get(&m.texture_id)?;
                match t.gpu.as_ref()? {
                    Place::Own(_, view) => Some((view.clone(), t.options, None, false)),
                    Place::Mono(_, view) => Some((view.clone(), t.options, None, true)),
                    &Place::Tile { page, x, y } => {
                        let side = PAGE as f32;
                        let span = [(x + 1) as f32 / side, (y + 1) as f32 / side, t.size[0] as f32 / side, t.size[1] as f32 / side];
                        Some((self.atlas.pages.get(page)?.as_ref()?.view.clone(), t.options, Some(span), false))
                    }
                }
            })
            .collect();

        let mut locals = [0u8; 16];
        locals[0..4].copy_from_slice(&(size[0] as f32 / ppp).to_le_bytes());
        locals[4..8].copy_from_slice(&(size[1] as f32 / ppp).to_le_bytes());
        locals[8..12].copy_from_slice(&u32::from(self.dithering).to_le_bytes());
        let g = &self.gpu;
        write(&g.ctx, &g.locals, &locals)?;

        unsafe {
            g.ctx.OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            g.ctx.ClearRenderTargetView(&target, &clear_color);
            g.ctx.RSSetViewports(Some(&[D3D11_VIEWPORT { TopLeftX: 0.0, TopLeftY: 0.0, Width: size[0] as f32, Height: size[1] as f32, MinDepth: 0.0, MaxDepth: 1.0 }]));
            g.ctx.RSSetState(&g.raster);
            g.ctx.OMSetBlendState(&g.blend, Some(&[0.0; 4]), u32::MAX);
            g.ctx.IASetInputLayout(&g.layout);
            g.ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            g.ctx.VSSetShader(&g.vs, None);
            g.ctx.PSSetShader(&g.ps, None);
            g.ctx.VSSetConstantBuffers(0, Some(&[Some(g.locals.clone())]));
            g.ctx.PSSetConstantBuffers(0, Some(&[Some(g.locals.clone())]));
        }

        if meshes.iter().any(|(_, m)| !m.indices.is_empty()) {
            let vb = self.fill(false, meshes.iter().map(|(_, m)| std::mem::size_of_val(&m.vertices[..])).sum(), |mut at| unsafe {
                for ((_, m), source) in meshes.iter().zip(&sources) {
                    let bytes = raw(&m.vertices);
                    match source {
                        Some((_, _, Some([x, y, w, h]), _)) => {
                            for (i, v) in m.vertices.iter().enumerate() {
                                let uv = epaint::pos2(x + v.uv.x.clamp(0.0, 1.0) * w, y + v.uv.y.clamp(0.0, 1.0) * h);
                                at.cast::<epaint::Vertex>().add(i).write_unaligned(epaint::Vertex { uv, ..*v });
                            }
                        }
                        _ => std::ptr::copy_nonoverlapping(bytes.as_ptr(), at, bytes.len()),
                    }
                    at = at.add(bytes.len());
                }
            })?;
            let ib = self.fill(true, meshes.iter().map(|(_, m)| std::mem::size_of_val(&m.indices[..])).sum(), |mut at| unsafe {
                for (_, m) in &meshes {
                    let bytes = raw(&m.indices);
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), at, bytes.len());
                    at = at.add(bytes.len());
                }
            })?;
            let stride = std::mem::size_of::<epaint::Vertex>() as u32;
            unsafe {
                self.gpu.ctx.IASetVertexBuffers(0, 1, Some(&Some(vb)), Some(&stride), Some(&0));
                self.gpu.ctx.IASetIndexBuffer(&ib, DXGI_FORMAT_R32_UINT, 0);
            }
            let (mut first_index, mut first_vertex) = (0u32, 0i32);
            let (mut bound, mut options, mut clipped, mut glyphs) = (None, None, None, false);
            for ((clip, mesh), source) in meshes.into_iter().zip(sources) {
                let (count, verts) = (mesh.indices.len() as u32, mesh.vertices.len() as i32);
                let r = scissor(clip, ppp, size);
                if r.right > r.left && r.bottom > r.top
                    && let Some((view, opts, _, mono)) = source
                {
                    if glyphs != mono {
                        glyphs = mono;
                        unsafe { self.gpu.ctx.PSSetShader(if mono { &self.gpu.font } else { &self.gpu.ps }, None) };
                    }
                    if options != Some(opts) {
                        let sampler = self.sampler(opts)?;
                        unsafe { self.gpu.ctx.PSSetSamplers(0, Some(&[Some(sampler)])) };
                        options = Some(opts);
                    }
                    let ctx = &self.gpu.ctx;
                    unsafe {
                        if bound != Some(view.as_raw()) {
                            bound = Some(view.as_raw());
                            ctx.PSSetShaderResources(0, Some(&[Some(view)]));
                        }
                        if clipped != Some((r.left, r.top, r.right, r.bottom)) {
                            ctx.RSSetScissorRects(Some(&[r]));
                            clipped = Some((r.left, r.top, r.right, r.bottom));
                        }
                        ctx.DrawIndexed(count, first_index, first_vertex);
                    }
                }
                first_index += count;
                first_vertex += verts;
            }
        }

        if !capture.is_empty() {
            match self.read_back(&swapchain, size) {
                Ok(image) => {
                    self.captured.borrow_mut().push((viewport_id, capture, Arc::new(image)));
                    self.egui_ctx.request_repaint();
                }
                Err(err) => log::error!("screenshot: {err}"),
            }
        }

        let start = Instant::now();
        unsafe { swapchain.Present(1, DXGI_PRESENT(0)).ok()? };
        Ok(start.elapsed().as_secs_f32())
    }

    fn read_back(&self, swapchain: &IDXGISwapChain1, size: [u32; 2]) -> windows::core::Result<ColorImage> {
        let (device, ctx) = (&self.gpu.device, &self.gpu.ctx);
        let back: ID3D11Texture2D = unsafe { swapchain.GetBuffer(0)? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { back.GetDesc(&mut desc) };
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging))? };
        let staging = staging.ok_or_else(Error::empty)?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            ctx.CopyResource(&staging, &back);
            ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        }
        let [w, h] = [size[0] as usize, size[1] as usize];
        let mut pixels = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>().add(y * mapped.RowPitch as usize), w * 4) };
            pixels.extend(row.chunks_exact(4).map(|c| egui::Color32::from_rgba_premultiplied(c[2], c[1], c[0], c[3])));
        }
        unsafe { ctx.Unmap(&staging, 0) };
        Ok(ColorImage::new([w, h], pixels))
    }
}

fn raw<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
}

fn write(ctx: &ID3D11DeviceContext, buffer: &ID3D11Buffer, data: &[u8]) -> windows::core::Result<()> {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        ctx.Map(buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), mapped.pData.cast(), data.len());
        ctx.Unmap(buffer, 0);
    }
    Ok(())
}

fn scissor(clip: &epaint::Rect, ppp: f32, size: [u32; 2]) -> RECT {
    let min_x = ((ppp * clip.min.x).round() as u32).clamp(0, size[0]);
    let min_y = ((ppp * clip.min.y).round() as u32).clamp(0, size[1]);
    let max_x = ((ppp * clip.max.x).round() as u32).clamp(min_x, size[0]);
    let max_y = ((ppp * clip.max.y).round() as u32).clamp(min_y, size[1]);
    RECT { left: min_x as i32, top: min_y as i32, right: max_x as i32, bottom: max_y as i32 }
}
