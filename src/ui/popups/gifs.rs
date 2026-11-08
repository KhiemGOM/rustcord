use super::*;

pub(crate) struct Gifs {
    pub query: String,
    pub sent: String,
    pub results: Option<Vec<Gif>>,
    pub at: Pos2,
    // discord's opening view: search terms over a preview each, and a trending preview
    pub cats: Option<Vec<(String, String)>>,
    pub trend: Option<String>,
    pub trending: bool,
    // the starred ones, opened from their tile
    pub favorites: bool,
    // opened from the avatar dialog: no tabs or stars, and a click hands the picture to the cropper
    pub profile: bool,
}

/// Discord's gifs come from Klipy now.
pub(super) const GIF_Q: &str = "provider=klipy&locale=en-US&media_format=gif";

pub(crate) struct Gif {
    pub url: String,
    pub src: String,
    pub w: f32,
    pub h: f32,
}

impl App {
    pub fn open_gifs(&mut self, at: Pos2) {
        self.gifs = Some(Gifs { query: String::new(), sent: String::new(), results: None, at, cats: None, trend: None, trending: false, favorites: false, profile: false });
        self.rest("GET", format!("/gifs/trending?{GIF_Q}"), None, Tag::GifCats);
    }

    /// What /gif and the picker's box search for.
    pub fn open_gifs_for_profile(&mut self, at: Pos2) {
        self.open_gifs(at);
        if let Some(g) = &mut self.gifs {
            g.profile = true;
        }
    }

    pub fn search_gifs(&mut self, q: String) {
        let Some(g) = &mut self.gifs else { return };
        g.query = q.clone();
        g.sent = q.clone();
        g.results = None;
        g.favorites = false;
        if q.is_empty() {
            g.trending = false;
        } else {
            let enc: String = q.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
            self.rest("GET", format!("/gifs/search?q={enc}&{GIF_Q}"), None, Tag::Gifs(q));
        }
    }

    pub fn set_gif_cats(&mut self, v: &serde_json::Value) {
        let Some(g) = &mut self.gifs else { return };
        let cats = v["categories"].as_array().into_iter().flatten();
        g.cats = Some(cats.filter_map(|c| Some((c["name"].as_str()?.to_string(), c["src"].as_str()?.to_string()))).collect());
        g.trend = v["gifs"][0]["src"].as_str().or(v["gifs"][0]["gif_src"].as_str()).map(str::to_string);
    }

    pub fn set_gifs(&mut self, q: String, v: &serde_json::Value) {
        let Some(g) = &mut self.gifs else { return };
        if g.sent != q {
            return; // an older search answered late
        }
        let list = if v.is_array() { v } else { &v["gifs"] };
        g.results = Some(
            list.as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| {
                    Some(Gif {
                        url: x["url"].as_str()?.to_string(),
                        src: x["src"].as_str().or(x["gif_src"].as_str())?.to_string(),
                        w: x["width"].as_f64().unwrap_or(200.0) as f32,
                        h: x["height"].as_f64().unwrap_or(150.0) as f32,
                    })
                })
                .collect(),
        );
    }
}

pub(super) fn gifs(app: &mut App, ctx: &egui::Context) {
    let autoplay = app.settings.gif_autoplay;
    let profile = app.gifs.as_ref().is_some_and(|g| g.profile);
    let favs: Vec<Gif> = if profile { Vec::new() } else { app.settings.fav_gifs.iter().map(|(url, src, w, h)| Gif { url: url.clone(), src: src.clone(), w: *w, h: *h }).collect() };
    let Some(g) = &mut app.gifs else { return };
    let mut pick = None;
    let mut search = None;
    let mut trending = false;
    let mut back = false;
    let mut star = None;
    let mut open_favs = false;
    let home = g.query.is_empty() && !g.trending && !g.favorites;
    let mut cat_rects = Vec::new();
    let (area, to) = crate::ui::picker::shell(ctx, g.at, Align2::RIGHT_BOTTOM, (!profile).then_some(crate::ui::picker::Tab::Gifs), |ui| {
        let r = crate::ui::picker::search_box(ui, &mut g.query, "Search Klipy", (!home).then_some(&mut back));
        r.request_focus();
        if r.changed() {
            search = Some(g.query.trim().to_string());
        }
        crate::ui::picker::header_rule(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin { left: 12, right: 12, top: 12, bottom: 12 }).show(ui, |ui| {
                // two 230x110 tiles a row: trending first, then discord's search terms
                if home {
                    let Some(cats) = &g.cats else {
                        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                        return;
                    };
                    let top = ui.cursor().min;
                    // discord leads with Favorites once something's starred
                    let fav = favs.first().map(|f| ("Favorites".to_string(), Some(f.src.clone())));
                    let tiles = fav.into_iter().chain(std::iter::once(("Trending GIFs".to_string(), g.trend.clone()))).chain(cats.iter().map(|(n, s)| (n.clone(), Some(s.clone()))));
                    let first_cat = if favs.is_empty() { 1 } else { 2 };
                    let mut n = 0;
                    for (i, (name, src)) in tiles.enumerate() {
                        let r = Rect::from_min_size(top + vec2((i % 2) as f32 * 242.0, (i / 2) as f32 * 122.0), vec2(230.0, 110.0));
                        let resp = ui.interact(r, egui::Id::new(("gifcat", i)), Sense::click());
                        if ui.is_rect_visible(r) {
                            ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
                            if let Some(src) = &src {
                                egui::Image::from_uri(crate::gif::uri(ui.ctx(), src, autoplay || resp.hovered())).corner_radius(CornerRadius::same(5)).show_loading_spinner(false).paint_at(ui, r);
                            }
                            let shade = if resp.hovered() { 140 } else { 184 };
                            ui.painter().rect_filled(r, CornerRadius::same(5), egui::Color32::from_black_alpha(shade));
                            let g = ui.painter().layout_no_wrap(name.clone(), semi(16.0), egui::Color32::WHITE);
                            let mut at = r.center() - g.size() / 2.0;
                            if i < first_cat {
                                at.x += 12.0;
                                let icon = if name == "Favorites" { Icon::Star } else { Icon::Insights };
                                crate::ui::icons::draw(ui.painter(), icon, Rect::from_center_size(pos2(at.x - 14.0, r.center().y), vec2(20.0, 20.0)), egui::Color32::WHITE);
                            }
                            ui.painter().galley(at, g, egui::Color32::WHITE);
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if name == "Trending GIFs" {
                            cat_rects.push(("gifcat:trending".to_string(), r));
                        }
                        if name == "Favorites" {
                            cat_rects.push(("gifcat:favorites".to_string(), r));
                        }
                        if resp.clicked() {
                            match name.as_str() {
                                "Favorites" if i == 0 && !favs.is_empty() => open_favs = true,
                                "Trending GIFs" if i < first_cat => trending = true,
                                _ => search = Some(name.clone()),
                            }
                        }
                        n = i + 1;
                    }
                    ui.allocate_space(vec2(472.0, n.div_ceil(2) as f32 * 122.0));
                    return;
                }
                let shown = if g.favorites { Some(&favs) } else { g.results.as_ref() };
                match shown {
                    None => {
                        crate::ui::motion::spinner(ui, 24.0, pal().text_muted);
                    }
                    Some(list) if list.is_empty() => {
                        ui.label(RichText::new("No GIFs found.").color(pal().header_secondary));
                    }
                    Some(list) => {
                        // two columns, each image keeps its aspect, shortest column gets the next one
                        let col_w = (ui.available_width() - 12.0) / 2.0;
                        let top = ui.cursor().min;
                        let mut ys = [0.0f32; 2];
                        for (i, gif) in list.iter().enumerate() {
                            let c = if ys[0] <= ys[1] { 0 } else { 1 };
                            let h = col_w * gif.h / gif.w.max(1.0);
                            let r = Rect::from_min_size(top + vec2(c as f32 * (col_w + 12.0), ys[c]), vec2(col_w, h));
                            ys[c] += h + 12.0;
                            let resp = ui.interact(r, egui::Id::new(("gif", i)), Sense::click());
                            if ui.is_rect_visible(r) {
                                ui.painter().rect_filled(r, CornerRadius::same(5), pal().bg_floating);
                                let play = autoplay || resp.hovered();
                                egui::Image::from_uri(crate::gif::uri(ui.ctx(), &gif.src, play)).corner_radius(CornerRadius::same(5)).show_loading_spinner(false).paint_at(ui, r);
                            }
                            // the star that saves it to favourites, in the corner while hovered
                            let starred = favs.iter().any(|f| f.url == gif.url);
                            let sr = Rect::from_min_size(pos2(r.right() - 36.0, r.top() + 8.0), vec2(28.0, 28.0));
                            if resp.hovered() || starred {
                                ui.painter().circle_filled(sr.center(), 14.0, egui::Color32::from_black_alpha(150));
                                let col = if starred { pal().yellow } else { egui::Color32::WHITE };
                                crate::ui::icons::draw(ui.painter(), Icon::Star, sr.shrink(6.0), col);
                            }
                            if resp.hovered() {
                                ui.painter().rect_stroke(r, CornerRadius::same(5), egui::Stroke::new(2.0, pal().blurple), egui::StrokeKind::Inside);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if i == 0 {
                                cat_rects.push(("gif:0".to_string(), r));
                                cat_rects.push(("gif:0:star".to_string(), sr));
                            }
                            if resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| sr.contains(p)) {
                                star = Some((gif.url.clone(), gif.src.clone(), gif.w, gif.h));
                            } else if resp.clicked() {
                                pick = Some(if profile { gif.src.clone() } else { gif.url.clone() });
                            }
                        }
                        ui.allocate_space(vec2(ui.available_width(), ys[0].max(ys[1])));
                    }
                }
            });
        });
    });
    if back {
        g.query.clear();
        g.trending = false;
        g.favorites = false;
        g.results = None;
        search = None;
    }
    if open_favs {
        g.favorites = true;
    }
    if trending {
        g.trending = true;
        g.sent = String::new();
        g.results = None;
        app.rest("GET", format!("/gifs/trending-gifs?{GIF_Q}"), None, Tag::Gifs(String::new()));
    } else if let Some(q) = search {
        app.search_gifs(q);
    }
    if let Some(f) = star {
        let favs = &mut app.settings.fav_gifs;
        match favs.iter().position(|x| x.0 == f.0) {
            Some(i) => {
                favs.remove(i);
            }
            None => favs.insert(0, f),
        }
        crate::cache::set_settings(&app.settings);
    }
    for (k, r) in cat_rects {
        app.rects.insert(k, r);
    }
    if let Some(t) = to {
        crate::ui::picker::toggle(app, t);
    } else if let Some(url) = pick {
        app.gifs = None;
        if profile {
            app.gif_load = Some(url);
        } else if let Some(ch) = app.cur() {
            // shift+click puts the link in the composer instead of sending it
            if ctx.input(|i| i.modifiers.shift) {
                let d = app.drafts.entry(ch).or_default();
                if !d.is_empty() && !d.ends_with(' ') {
                    d.push(' ');
                }
                d.push_str(&url);
                app.focus_composer = true;
                app.caret_end = true;
            } else {
                app.drafts.insert(ch, url);
                app.send_message(ch);
            }
        }
    } else if dismissed(ctx, &area) && !crate::ui::picker::on_picker_button(app, ctx) {
        app.gifs = None;
    }
}
