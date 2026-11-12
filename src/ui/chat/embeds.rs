use super::*;

#[derive(Default)]
pub(super) struct CodeJob;

pub(super) type CodeJobs = egui::cache::FrameCache<std::sync::Arc<egui::text::LayoutJob>, CodeJob>;

impl egui::cache::ComputerMut<(Id, &str, &str, Color32, Color32), std::sync::Arc<egui::text::LayoutJob>> for CodeJob {
    fn compute(&mut self, (_, lang, code, base, comment): (Id, &str, &str, Color32, Color32)) -> std::sync::Arc<egui::text::LayoutJob> {
        std::sync::Arc::new(crate::highlight::fence(lang, code, mono(14.0), base, comment))
    }
}

pub(super) fn fit(w: u32, h: u32, max_w: f32, max_h: f32, avail: f32) -> (f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let s = (max_w.min(avail) / w).min(max_h / h).min(1.0);
    ((w * s).round(), (h * s).round())
}

/// The ALT tag on an image that has a description; hovering it reads the description out.
pub(super) fn alt_badge(ui: &Ui, image: &egui::Response, text: &str) {
    let tag = Rect::from_min_size(image.rect.left_bottom() + vec2(8.0, -28.0), vec2(36.0, 20.0));
    ui.painter().rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(160));
    ui.painter().text(tag.center(), Align2::CENTER_CENTER, "ALT", bold(11.0), Color32::WHITE);
    ui.interact(tag, ui.id().with(("alt", image.id)), Sense::hover()).on_hover_text(text);
}

/// `poster` is a still drawn underneath while the animation downloads and decodes.
pub(super) fn media(ui: &mut Ui, d: &mut D, url: &str, full: &str, w: f32, h: f32, poster: Option<&str>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    d.image_menu |= resp.secondary_clicked();
    media_at(ui, d, &resp, rect, url, full, poster, crate::ui::player::Look::FULL);
    resp
}

pub(super) fn media_at(ui: &mut Ui, d: &mut D, resp: &egui::Response, rect: Rect, url: &str, full: &str, poster: Option<&str>, look: crate::ui::player::Look) {
    resp.context_menu(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if crate::ui::menu_item(ui, None, "Copy Image", false) {
            d.acts.push(Act::CopyImage(full.to_string()));
        }
        if crate::ui::menu_item(ui, None, "Save Image", false) {
            d.acts.push(Act::Save(full.to_string()));
        }
        crate::ui::menu_sep(ui);
        if crate::ui::menu_item(ui, None, "Copy Link", false) {
            ui.ctx().copy_text(full.to_string());
        }
        if crate::ui::menu_item(ui, None, "Open Link", false) {
            d.acts.push(Act::Url(full.to_string()));
        }
    });
    ui.painter().rect_filled(rect, look.radius, pal().bg_secondary);
    if let Some(p) = poster {
        Image::from_uri(p.to_string()).uv(look.uv).corner_radius(look.radius).show_loading_spinner(false).paint_at(ui, rect);
    }
    let (auto, on_hover) = if url.contains("/stickers/") {
        use crate::cache::Animate::*;
        match d.s.stickers {
            Always => (true, true),
            OnInteraction => (false, true),
            Never => (false, false),
        }
    } else {
        (d.s.gif_autoplay, true)
    };
    let play = (auto || (on_hover && resp.hovered())) && ui.is_rect_visible(rect);
    Image::from_uri(gif::uri(ui.ctx(), url, play))
        .uv(look.uv)
        .corner_radius(look.radius)
        .show_loading_spinner(false)
        .paint_at(ui, rect);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// Discord's voice-message player: a pill with a play-pause button, the amplitude waveform
/// that fills as it plays, and the running time.
pub(super) fn audio_card(ui: &mut Ui, d: &mut D, a: &crate::state::Attachment) {
    let w = ui.available_width().min(340.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::from_rgba_unmultiplied(151, 151, 159, 31));
    // play / pause button
    let btn = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(32.0, 32.0));
    let pr = ui.interact(btn, ui.id().with(("aud", &a.url)), Sense::click());
    ui.painter().circle_filled(btn.center(), 16.0, crate::ui::motion::mix(pal().blurple, Color32::from_rgb(0x44, 0x52, 0xbb), crate::ui::motion::hot(ui.ctx(), &pr)));
    let playing = crate::audioplay::is_playing(&a.url);
    if playing {
        for dx in [-3.5, 3.5] {
            ui.painter().rect_filled(Rect::from_center_size(btn.center() + vec2(dx, 0.0), vec2(3.5, 13.0)), CornerRadius::same(1), Color32::WHITE);
        }
    } else {
        icons::draw(ui.painter(), Icon::Play, Rect::from_center_size(btn.center() + vec2(1.0, 0.0), vec2(18.0, 18.0)), Color32::WHITE);
    }
    let prog = crate::audioplay::progress(&a.url).unwrap_or(0.0);
    // the waveform / track
    let bar = Rect::from_min_max(pos2(btn.right() + 10.0, rect.top() + 14.0), pos2(rect.right() - 54.0, rect.bottom() - 14.0));
    let bars = decode_waveform(&a.waveform);
    if !bars.is_empty() {
        let n = bars.len();
        let step = bar.width() / n as f32;
        for (i, amp) in bars.iter().enumerate() {
            let h = (amp / 255.0 * bar.height()).max(2.0);
            let x = bar.left() + i as f32 * step;
            let past = (i as f32 + 0.5) / n as f32 <= prog;
            let c = if past { Color32::WHITE } else { pal().text_muted };
            ui.painter().rect_filled(Rect::from_center_size(pos2(x + step / 2.0, bar.center().y), vec2((step - 1.5).max(1.5), h)), CornerRadius::same(1), c);
        }
    } else {
        ui.painter().rect_filled(Rect::from_min_size(bar.left_center() - vec2(0.0, 2.0), vec2(bar.width(), 4.0)), CornerRadius::same(2), pal().text_muted.gamma_multiply(0.4));
        ui.painter().rect_filled(Rect::from_min_size(bar.left_center() - vec2(0.0, 2.0), vec2(bar.width() * prog, 4.0)), CornerRadius::same(2), Color32::WHITE);
        ui.painter().circle_filled(pos2(bar.left() + bar.width() * prog, bar.center().y), 5.0, Color32::WHITE);
    }
    // time: counts up while playing, else the clip length
    let secs = if prog > 0.0 && a.duration > 0.0 { a.duration * prog } else { a.duration };
    let t = format!("{}:{:02}", secs as u32 / 60, secs as u32 % 60);
    ui.painter().text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, t, medium(12.0), pal().header_secondary);
    if pr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if pr.clicked() {
        // ogg/opus plays inline; anything else (an mp3 upload) opens in the browser
        d.acts.push(if a.playable() { Act::PlayAudio(a.url.clone()) } else { Act::Url(a.url.clone()) });
    }
    if crate::audioplay::progress(&a.url).is_some() {
        ui.ctx().request_repaint_after_secs(0.05);
    }
}

pub(super) fn decode_waveform(b64: &str) -> Vec<f32> {
    use base64::Engine;
    if b64.is_empty() {
        return Vec::new();
    }
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map(|bytes| bytes.iter().map(|b| *b as f32).collect())
        .unwrap_or_default()
}

pub(crate) fn human_size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} bytes"),
        1024..1_048_576 => format!("{:.2} KB", n as f64 / 1024.0),
        _ => format!("{:.2} MB", n as f64 / 1_048_576.0),
    }
}

/// Discord's card for a file it doesn't show inline, videos it can't play included: its
/// file glyph, the name as a link that underlines under the pointer, and the size.
pub(super) fn file_card(ui: &mut Ui, name: &str, size: u64) -> egui::Response {
    let w = ui.available_width().min(432.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 66.0), Sense::click());
    ui.painter().rect(rect, CornerRadius::same(8), pal().bg_secondary, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    icons::draw(ui.painter(), Icon::File, Rect::from_min_size(rect.min + vec2(16.0, 21.0), vec2(24.0, 24.0)), pal().blurple);
    let n = text::paint(ui.painter(), pos2(rect.left() + 52.0, rect.top() + 23.0), Align2::LEFT_CENTER, name, reg(14.0), pal().link, rect.width() - 64.0);
    if resp.hovered() {
        ui.painter().hline(n.x_range(), n.bottom() - 1.0, Stroke::new(1.0, pal().link));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().with_clip_rect(rect.shrink(8.0)).text(pos2(rect.left() + 52.0, rect.top() + 44.0), Align2::LEFT_CENTER, human_size(size), reg(12.0), pal().text_muted);
    resp
}

/// Everything in a message the image viewer can step through: its pictures, then its embeds'.
pub(super) fn gallery(m: &Message) -> Vec<(String, String, u32, u32)> {
    let pics = m.attachments.iter().filter(|a| a.is_image()).map(|a| (a.proxy.clone(), a.url.clone(), a.w, a.h));
    let embeds = m.embeds.iter().filter_map(|e| {
        let x = e.image.as_ref().or(e.thumb.as_ref())?;
        Some((x.url.clone(), e.url.clone().unwrap_or_else(|| x.url.clone()), x.w, x.h))
    });
    pics.chain(embeds).collect()
}

/// Discord drops the text of a message that is nothing but the link its image or gifv embed came from.
pub(super) fn link_only(m: &Message) -> bool {
    let c = m.content.trim();
    m.embeds.iter().any(|e| matches!(e.kind.as_str(), "image" | "gifv") && e.url.as_deref() == Some(c))
}

pub(super) fn direct(v: &crate::state::Media) -> bool {
    v.proxied || is_video_file(&v.url)
}

pub(super) fn inline_video(ui: &mut Ui, d: &mut D, e: &Embed, v: &crate::state::Media, msg: Id) -> Rect {
    let name = v.url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("video.mp4").to_string();
    let a = crate::state::Attachment { url: v.url.clone(), proxy: v.url.clone(), filename: name, size: 0, w: v.w, h: v.h, ctype: "video/mp4".into(), duration: 0.0, waveform: String::new(), description: String::new() };
    let (w, h) = fit(v.w, v.h, 400.0, 300.0, ui.available_width());
    let poster = e.thumb.as_ref().map_or(v.url.as_str(), |t| t.url.as_str());
    let mut outs = Vec::new();
    let rect = crate::ui::player::video(ui, &a, vec2(w, h), poster, &mut outs);
    d.player(outs);
    d.rects.push((format!("embed_player:{msg}"), rect.intersect(ui.clip_rect())));
    rect
}

pub(super) fn is_video_file(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
    [".mp4", ".m4v", ".mov", ".webm"].iter().any(|e| path.ends_with(e))
}

pub(super) fn embed(ui: &mut Ui, d: &mut D, e: &Embed, msg: Id, idx: usize) -> Option<Rect> {
    let bare = e.title.is_none() && e.desc.is_none() && e.author.is_none() && e.fields.is_empty();
    let media_ok = d.s.inline_links;
    // gifv is always just the looping clip, whatever title the provider gave it
    let gifv = e.kind == "gifv";
    let video = e.video.as_ref().filter(|_| e.kind == "video" && media_ok);
    if let Some(v) = video.filter(|v| bare && direct(v)) {
        return Some(inline_video(ui, d, e, v, msg));
    }
    if gifv || (bare && e.kind == "image") {
        if !media_ok {
            return None;
        }
        let still = e.thumb.as_ref().or(e.image.as_ref());
        if let Some(m) = e.video.as_ref().filter(|_| gifv).or(still) {
            let (mw, mh) = if m.w > 0 { (m.w, m.h) } else { still.map_or((0, 0), |s| (s.w, s.h)) };
            let (w, h) = fit(mw, mh, 400.0, 300.0, ui.available_width());
            let poster = still.filter(|s| gifv && s.url != m.url).map(|s| s.url.as_str());
            let r = media(ui, d, &m.url, e.url.as_deref().unwrap_or(&m.url), w, h, poster);
            if ui.clip_rect().contains(r.rect.center()) {
                d.rects.push((format!("{}:{msg}", if gifv { "gifv" } else { "embed_image" }), r.rect.intersect(ui.clip_rect())));
            }
            if r.clicked() {
                let open = e.url.clone().unwrap_or_else(|| m.url.clone());
                d.acts.push(Act::View(m.url.clone(), open, mw, mh));
            }
            return Some(r.rect);
        }
        return None;
    }
    // discord's: a 4px bar, a 1px border round the rest, the text 12 in from the bar and 16 from
    // the right, 2 from the top and 16 from the bottom, each part 8 under the last. It's as wide
    // as its longest unwrapped line, up to 488 of text.
    let inner = (ui.available_width() - 33.0).min(488.0);
    let bar = e.color.map(role_color).unwrap_or(pal().border_normal);
    let start = ui.cursor().min;
    let thumb = e.thumb.as_ref().filter(|_| media_ok && e.kind != "video" && e.image.is_none());
    let text_w = inner - if thumb.is_some() { 96.0 } else { 0.0 };
    let desc = e.desc.as_ref().map(|t| {
        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: pal().text_normal, revealed: d.revealed.contains(&msg), edited: false, embed: true };
        markdown::parse(t, &cx)
    });
    let widest = desc
        .iter()
        .flatten()
        .map(|b| match b {
            Block::Text(t) => ui.fonts_mut(|f| f.layout_job(t.job.clone())).size().x,
            _ => 0.0,
        })
        .chain(e.title.iter().map(|t| ui.fonts_mut(|f| f.layout_no_wrap(t.clone(), semi(16.0), Color32::WHITE)).size().x))
        .fold(0.0, f32::max);
    let r = Frame::new()
        .fill(pal().bg_secondary)
        .corner_radius(4)
        .inner_margin(Margin { left: 16, right: 17, top: 3, bottom: 17 })
        .show(ui, |ui| {
            ui.set_max_width(inner);
            ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(text_w);
                    if widest > text_w {
                        ui.set_min_width(text_w);
                    }
                    if let Some(p) = &e.provider {
                        ui.add_space(8.0);
                        ui.label(RichText::new(p).font(reg(12.0)).line_height(Some(16.0)).color(pal().text_normal));
                    }
                    if let Some(a) = &e.author {
                        ui.add_space(8.0);
                        ui.label(RichText::new(a).font(semi(14.0)).line_height(Some(22.0)).color(pal().header_primary));
                    }
                    if let Some(t) = &e.title {
                        ui.add_space(8.0);
                        let col = if e.url.is_some() { pal().link } else { pal().header_primary };
                        let r = ui.add(Label::new(RichText::new(t).font(semi(16.0)).line_height(Some(22.0)).color(col)).sense(Sense::click()));
                        if e.url.is_some() && r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if let Some(u) = &e.url {
                            if r.clicked() {
                                d.acts.push(Act::Url(u.clone()));
                            } else if r.middle_clicked() {
                                d.acts.push(Act::Browser(u.clone()));
                            }
                        }
                    }
                    if let Some(blocks) = &desc {
                        ui.add_space(8.0);
                        blocks_ui(ui, blocks, d, msg);
                    }
                    // the fields are a grid 8 apart, three inline ones to a row
                    let mut row: Vec<&(String, String, bool)> = Vec::new();
                    let flush = |ui: &mut Ui, d: &mut D, row: &mut Vec<&(String, String, bool)>| {
                        if row.is_empty() {
                            return;
                        }
                        ui.add_space(8.0);
                        let cw = ((ui.available_width() + 8.0) / row.len() as f32).max(80.0);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for f in row.iter() {
                                ui.allocate_ui(vec2(cw - 8.0, 0.0), |ui| {
                                    ui.set_min_width(cw - 8.0);
                                    field(ui, d, f, msg)
                                });
                            }
                        });
                        row.clear();
                    };
                    for f in &e.fields {
                        if f.2 {
                            row.push(f);
                            if row.len() == 3 {
                                flush(ui, d, &mut row);
                            }
                        } else {
                            flush(ui, d, &mut row);
                            ui.add_space(8.0);
                            field(ui, d, f, msg);
                        }
                    }
                    flush(ui, d, &mut row);
                });
                if let Some(t) = thumb {
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        let (w, h) = fit(t.w, t.h, 80.0, 80.0, 80.0);
                        media(ui, d, &t.url, &t.url, w, h, None);
                    });
                }
            });
            if let Some(v) = video.filter(|v| direct(v)) {
                ui.add_space(16.0);
                inline_video(ui, d, e, v, msg);
            } else if let Some(m) = e.image.as_ref().or(if e.kind == "video" { e.thumb.as_ref() } else { None }).filter(|_| media_ok) {
                ui.add_space(16.0);
                let framed = video.filter(|v| crate::ui::embedplay::src(&v.url).is_some());
                let (mw, mh) = framed.filter(|v| v.w > 0).map_or((m.w, m.h), |v| (v.w, v.h));
                let (w, h) = fit(mw, mh, 400.0, 300.0, ui.available_width());
                let key = (msg, idx);
                if framed.is_some() && d.playing == Some(key) {
                    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
                    ui.painter().rect_filled(rect, CornerRadius::same(4), Color32::BLACK);
                    d.frame_at = Some((key, rect, ui.clip_rect()));
                    d.rects.push((format!("embed_frame:{msg}"), rect));
                    d.rects.push(("embed_clip".into(), ui.clip_rect()));
                } else {
                    let r = media(ui, d, &m.url, &m.url, w, h, None);
                    let mut link = false;
                    if e.kind == "video" {
                        link = crate::ui::embedplay::pill(ui, r.rect, framed.is_some(), e.url.is_some());
                        d.rects.push((format!("embed_poster:{msg}"), r.rect.intersect(ui.clip_rect())));
                    }
                    if link {
                        d.acts.extend(e.url.clone().map(Act::Url));
                    } else if r.clicked() {
                        match (&e.url, framed, e.kind == "video") {
                            (_, Some(v), _) => d.acts.push(Act::Frame(key, v.url.clone())),
                            (Some(u), _, true) => d.acts.push(Act::Url(u.clone())),
                            _ => d.acts.push(Act::View(m.url.clone(), e.url.clone().unwrap_or_else(|| m.url.clone()), m.w, m.h)),
                        }
                    }
                }
            }
            let when = e.timestamp.map(|t| {
                let s = stamp(t);
                if s.contains(',') || s.starts_with("Yesterday") { s } else { format!("Today at {s}") }
            });
            let foot = match (&e.footer, when) {
                (Some(f), Some(w)) => Some(format!("{f} • {w}")),
                (f, w) => f.clone().or(w),
            };
            if let Some(f) = foot {
                ui.add_space(8.0);
                ui.label(RichText::new(f).font(reg(12.0)).line_height(Some(16.0)).color(pal().text_normal));
            }
        });
    let rect = r.response.rect;
    // over the fill, as css draws it, so the border picks up the embed's colour
    ui.painter().rect_stroke(rect, 4, Stroke::new(1.0, pal().border_subtle), egui::StrokeKind::Inside);
    ui.painter().rect_filled(
        Rect::from_min_max(start, pos2(start.x + 4.0, rect.bottom())),
        CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 },
        bar,
    );
    Some(rect)
}

pub(super) fn field(ui: &mut Ui, d: &mut D, f: &(String, String, bool), msg: Id) {
    ui.vertical(|ui| {
        ui.label(RichText::new(&f.0).font(semi(14.0)).line_height(Some(18.0)).color(pal().header_primary));
        ui.add_space(2.0);
        let cx = Cx { store: d.store, ctx: d.ctx, guild: d.guild, size: 14.0, color: pal().text_normal, revealed: d.revealed.contains(&msg), edited: false, embed: true };
        let blocks = markdown::parse(&f.1, &cx);
        blocks_ui(ui, &blocks, d, msg);
    });
}

pub(crate) fn emoji_url(e: &ReactEmoji) -> String {
    if e.id != 0 {
        markdown::emoji_url(e.id, e.animated)
    } else {
        markdown::twemoji(&e.name)
    }
}

pub(super) fn reactions(ui: &mut Ui, d: &mut D, m: &Message) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for r in &m.reactions {
            let count = r.count.to_string();
            // discord gives the count a slot per digit, so a pill doesn't change width as it ticks over
            let slot = 12.0 * count.len() as f32 - 2.0;
            let g = ui.painter().layout_no_wrap(count, semi(16.0), if r.me { pal().mention_fg } else { pal().header_secondary });
            let slot = slot.max(g.size().x);
            let size = vec2(8.0 + 20.0 + 6.0 + slot + 8.0, 30.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let on = crate::ui::motion::tween(ui.ctx(), resp.id.with("me"), r.me, 0.1, crate::ui::motion::EASE);
            let hot = crate::ui::motion::hot(ui.ctx(), &resp);
            let idle = (pal().bg_secondary, crate::ui::motion::mix(Color32::TRANSPARENT, pal().bg_accent, hot));
            let (fill, stroke) = (
                crate::ui::motion::mix(idle.0, wash(pal().blurple, 38), on),
                crate::ui::motion::mix(idle.1, pal().blurple, on),
            );
            ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
            let er = Rect::from_min_size(pos2(rect.left() + 8.0, rect.center().y - 10.0), vec2(20.0, 20.0));
            Image::from_uri(gif::uri(ui.ctx(), &emoji_url(&r.emoji), ui.is_rect_visible(er))).show_loading_spinner(false).paint_at(ui, er);
            ui.painter().galley(pos2(er.right() + 6.0 + (slot - g.size().x) / 2.0, rect.center().y - g.size().y / 2.0), g, pal().header_secondary);
            if resp.clicked() {
                d.acts.push(Act::React(m.id, r.emoji.clone(), !r.me));
            }
            let burst = r.burst > 0 && r.burst == r.count;
            if resp.hovered() {
                d.acts.push(Act::Reactors(m.id, r.emoji.clone(), burst, r.count));
            }
            resp.on_hover_ui(|ui| {
                ui.label(crate::ui::reactions::tooltip(d.store, d.guild, m, r, burst));
            });
        }
    });
}
