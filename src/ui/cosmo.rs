//! The shop's profile cosmetics: what each avatar decoration, profile effect, profile frame and
//! nameplate is, where its pictures live, and how an effect's layers play over a profile.

use egui::{Rect, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::App;
use crate::net::Tag;
use crate::state::Map;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Decoration,
    Effect,
    Nameplate,
    Frame,
}

impl Kind {
    pub fn of(ty: u64) -> Option<Kind> {
        match ty {
            0 => Some(Kind::Decoration),
            1 => Some(Kind::Effect),
            2 => Some(Kind::Nameplate),
            3 => Some(Kind::Frame),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Default, Debug)]
pub struct Layer {
    pub src: String,
    pub looped: bool,
    pub duration: u64,
    pub start: u64,
    pub delay: u64,
    pub z: i64,
    // a frame is stretched over its card; an effect keeps its shape, as wide as the card, from the top
    pub fill: bool,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub sku: String,
    pub kind: Kind,
    pub name: String,
    pub label: String,
    pub asset: String,
    pub thumb: String,
    pub layers: Vec<Layer>,
    pub nitro: bool,
}

impl PartialEq for Item {
    fn eq(&self, o: &Item) -> bool {
        self.kind == o.kind && self.sku == o.sku
    }
}

impl Item {
    pub fn parse(i: &Value, product: &Value) -> Option<Item> {
        let kind = Kind::of(i["type"].as_u64()?)?;
        let text = |k: &str| i[k].as_str().unwrap_or("").to_string();
        let layers = i["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| Layer {
                src: l["src"].as_str().unwrap_or("").to_string(),
                looped: l["loop"].as_bool().unwrap_or(false),
                duration: l["duration"].as_u64().unwrap_or(0),
                start: l["start"].as_u64().unwrap_or(0),
                delay: l["loopDelay"].as_u64().unwrap_or(0),
                z: l["zIndex"].as_i64().unwrap_or(0),
                fill: kind == Kind::Frame,
            })
            .collect();
        Some(Item {
            sku: i["sku_id"].as_str().or(i["id"].as_str()).or(product["sku_id"].as_str()).unwrap_or("").to_string(),
            kind,
            name: product["name"].as_str().or(i["title"].as_str()).unwrap_or("").to_string(),
            label: if i["label"].is_string() { text("label") } else { text("accessibilityLabel") },
            asset: text("asset"),
            thumb: text("thumbnailPreviewSrc"),
            layers,
            nitro: product["premium_type"].as_u64() == Some(2),
        })
    }

    pub fn still(&self) -> String {
        match self.kind {
            Kind::Decoration => deco(&self.asset, 96),
            Kind::Nameplate => plate(&self.asset),
            Kind::Effect | Kind::Frame => self.thumb.clone(),
        }
    }
}

pub fn deco(asset: &str, size: u32) -> String {
    format!("{}/avatar-decoration-presets/{asset}.png?size={size}&passthrough=true", crate::api::cdn())
}

pub fn plate(asset: &str) -> String {
    format!("{}/assets/collectibles/{asset}static.png", crate::api::cdn())
}

/// The products in a purchases list, or in the shop's categories.
fn products(v: &Value) -> Vec<&Value> {
    let list: Vec<&Value> = match v["categories"].as_array() {
        Some(cats) => cats.iter().flat_map(|c| c["products"].as_array().into_iter().flatten()).collect(),
        None => v.as_array().into_iter().flatten().collect(),
    };
    list.into_iter().flat_map(|p| std::iter::once(p).chain(p["variants"].as_array().into_iter().flatten())).collect()
}

/// Every single-item product in `v`, bundles unpacked into their items.
pub fn items(v: &Value) -> Vec<Item> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in products(v) {
        for i in p["items"].as_array().into_iter().flatten() {
            if let Some(it) = Item::parse(i, p)
                && seen.insert((it.kind as u8, it.sku.clone()))
            {
                out.push(it);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn owned_variant_groups_include_their_decorations() {
        let v = serde_json::json!([{"type":2000,"variants":[{"type":0,"sku_id":"11","name":"Panda",
            "items":[{"type":0,"sku_id":"11","asset":"panda"}]}]}]);
        let items = super::items(&v);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].asset, "panda");
        assert_eq!(items[0].sku, "11");
    }
}

#[derive(Default)]
pub struct Store {
    pub items: Map<String, Item>,
    pub owned: Option<Vec<Item>>,
    pub shop: Option<Vec<Item>>,
    asked: HashSet<String>,
}

impl Store {
    pub fn absorb(&mut self, list: Vec<Item>) {
        for it in list {
            self.items.insert(it.sku.clone(), it);
        }
    }
}

impl App {
    /// The product with this sku, once it's been fetched; asks for it the first time.
    pub fn product(&mut self, sku: &str) -> Option<Item> {
        if sku.is_empty() {
            return None;
        }
        if !self.cosmo.items.contains_key(sku) && self.cosmo.asked.insert(sku.to_string()) {
            self.rest("GET", format!("/collectibles-products/{sku}"), None, Tag::Product(sku.to_string()));
        }
        self.cosmo.items.get(sku).cloned()
    }

    pub fn product_loaded(&mut self, v: &Value) {
        let one = json!({ "categories": [{ "products": [v] }] });
        let list = items(&one);
        self.cosmo.absorb(list);
    }
}

/// Plays `layers` over `rect` the way discord's profile effects do: a layer starts `start` ms
/// after `since`, runs `duration` ms, and a looping one waits `delay` ms between plays. Layers
/// with a higher z are drawn last. Hands back whether anything is still due to play.
pub fn paint_layers(ui: &Ui, rect: Rect, layers: &[Layer], since: f64) -> bool {
    let now = ((ui.ctx().input(|i| i.time) - since).max(0.0) * 1000.0) as u128;
    let mut order: Vec<&Layer> = layers.iter().collect();
    order.sort_by_key(|l| l.z);
    let mut live = false;
    for l in order {
        if l.src.is_empty() || now < l.start as u128 {
            live |= !l.src.is_empty();
            continue;
        }
        let local = now - l.start as u128;
        let t = if l.looped {
            let period = (l.duration + l.delay).max(1) as u128;
            let t = local % period;
            if t >= l.duration as u128 {
                live = true;
                continue;
            }
            t
        } else if local >= l.duration as u128 {
            continue;
        } else {
            local
        };
        live = true;
        let uri = crate::gif::uri_at(ui.ctx(), &l.src, t, !l.looped);
        let poll = egui::Image::new(uri).load_for_size(ui.ctx(), rect.size());
        if let Ok(egui::load::TexturePoll::Ready { texture }) = poll {
            let p = ui.painter().with_clip_rect(rect);
            let at = if l.fill { rect } else { Rect::from_min_size(rect.min, vec2(rect.width(), rect.width() * texture.size.y / texture.size.x.max(1.0))) };
            p.image(texture.id, at, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
    }
    if live {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
    live
}

/// A profile's effect (and frame) laid over its card, from when the card opened.
pub fn paint_profile(app: &mut App, ui: &Ui, rect: Rect, effect: &str, frame: &str, since: f64) {
    for sku in [effect, frame] {
        if let Some(it) = app.product(sku) {
            paint_layers(ui, rect, &it.layers, since);
        }
    }
}

/// When a card's effect started playing: the first frame it was drawn, until `forget`.
pub fn since(ctx: &egui::Context, key: impl std::hash::Hash + std::fmt::Debug) -> f64 {
    let id = egui::Id::new(("fx_since", key));
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || now))
}

pub fn forget(ctx: &egui::Context, key: impl std::hash::Hash + std::fmt::Debug) {
    ctx.data_mut(|d| d.remove_temp::<f64>(egui::Id::new(("fx_since", key))));
}

/// The layers of the effect and frame a profile response says someone wears.
pub fn layers_of(app: &mut App, p: Option<&Value>) -> Vec<Layer> {
    let (e, f) = p.map(worn).unwrap_or_default();
    [e, f].iter().filter_map(|s| app.product(s)).flat_map(|i| i.layers).collect()
}

/// The sku of the effect and frame a profile response carries.
pub fn worn(p: &Value) -> (String, String) {
    let sku = |v: &Value| v["id"].as_str().or(v["sku_id"].as_str()).unwrap_or("").to_string();
    let prof = if p["guild_member_profile"].is_object() { &p["guild_member_profile"] } else { &p["user_profile"] };
    let pick = |k: &str| if prof[k].is_object() { sku(&prof[k]) } else { sku(&p["user_profile"][k]) };
    (pick("profile_effect"), pick("profile_frame"))
}
