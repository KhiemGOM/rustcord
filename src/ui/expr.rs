//! Vencord's emoji, sticker and image menu plugins: CopyEmojiMarkdown, CopyStickerLinks,
//! ExpressionCloner (with its server picker) and ReverseImageSearch.

use base64::Engine;
use egui::{Align, Layout, RichText, Sense, Ui};
use serde_json::json;

use super::outgoing::{self, Done};
use super::{App, Trail, menu_item, menu_row};
use crate::plugins;
use crate::state::Id;
use crate::theme::*;

#[derive(Clone)]
pub enum Cloning {
    Emoji(Id, String, bool),
    Sticker(Id, String, u8),
}

const ENGINES: [(&str, &str); 7] = [
    ("Google Lens", "https://lens.google.com/uploadbyurl?url="),
    ("Yandex", "https://yandex.com/images/search?rpt=imageview&url="),
    ("SauceNAO", "https://saucenao.com/search.php?url="),
    ("IQDB", "https://iqdb.org/?url="),
    ("Bing", "https://www.bing.com/images/search?view=detailv2&iss=sbi&q=imgurl:"),
    ("TinEye", "https://www.tineye.com/search?url="),
    ("ImgOps", "https://imgops.com/start?url="),
];

pub fn emoji_markdown(id: Id, name: &str, animated: bool) -> String {
    format!("<{}:{name}:{id}>", if animated { "a" } else { "" })
}

pub fn sticker_url(id: Id, fmt: u8) -> String {
    format!("https://media.discordapp.net/stickers/{id}.{}?size=512", if fmt == 4 { "gif" } else { "png" })
}

pub fn ask(ctx: &egui::Context, c: Cloning) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("cloning"), Some(c)));
}

/// The rows an image's menu gains: a reverse search submenu, and Clone for a sticker's own picture.
pub fn image_menu(ui: &mut Ui, url: &str) {
    if plugins::on("ReverseImageSearch") {
        super::menu_sep(ui);
        let row = menu_row(ui, None, "Search Image", None, false, Trail::Caret);
        egui::containers::menu::SubMenu::new().show(ui, &row, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (name, base) in ENGINES {
                if menu_item(ui, None, name, false) {
                    let _ = open::that_detached(format!("{base}{}", outgoing::enc(url)));
                }
            }
            if menu_item(ui, None, "All", false) {
                for (_, base) in ENGINES {
                    let _ = open::that_detached(format!("{base}{}", outgoing::enc(url)));
                }
            }
        });
    }
    if plugins::on("ExpressionCloner")
        && let Some(id) = url.split("/stickers/").nth(1).and_then(|r| r.split(['.', '?']).next()).and_then(|i| i.parse::<Id>().ok())
        && menu_item(ui, None, "Clone Sticker", false)
    {
        ask(ui.ctx(), Cloning::Sticker(id, "sticker".into(), if url.contains(".gif") { 4 } else { 2 }));
    }
}

/// The sticker picker's right click menu.
pub fn sticker_menu(ui: &mut Ui, id: Id, name: &str, fmt: u8) -> bool {
    let mut used = false;
    if plugins::on("CopyStickerLinks") {
        if menu_item(ui, Some(super::icons::Icon::Link), "Copy Link", false) {
            ui.ctx().copy_text(sticker_url(id, fmt));
            used = true;
        }
        if menu_item(ui, Some(super::icons::Icon::Launched), "Open Link", false) {
            let _ = open::that_detached(sticker_url(id, fmt));
            used = true;
        }
    }
    if plugins::on("ExpressionCloner") && menu_item(ui, Some(super::icons::Icon::Copy), "Clone Sticker", false) {
        ask(ui.ctx(), Cloning::Sticker(id, name.to_string(), fmt));
        used = true;
    }
    used
}

pub fn has_sticker_menu() -> bool {
    plugins::on("CopyStickerLinks") || plugins::on("ExpressionCloner")
}

/// Servers you may add emoji and stickers to.
fn targets(app: &App) -> Vec<(Id, String)> {
    const EXPRESSIONS: u64 = (1 << 30) | (1 << 43);
    app.store.guild_order.iter().filter_map(|g| app.store.guilds.get(g)).filter(|g| app.store.my_perms(g.id) & EXPRESSIONS != 0).map(|g| (g.id, g.name.clone())).collect()
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if let Some(c) = ctx.data_mut(|d| d.remove_temp::<Option<Cloning>>(egui::Id::new("cloning"))).flatten() {
        app.plug.cloning = Some(c);
    }
    let Some(what) = app.plug.cloning.clone() else { return };
    let list = targets(app);
    let mut pick = None;
    let mut close = false;
    let modal = egui::Modal::new("clone".into())
        .frame(egui::Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(16))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            let name = match &what {
                Cloning::Emoji(_, n, _) | Cloning::Sticker(_, n, _) => n,
            };
            ui.label(RichText::new(format!("Clone {name}")).font(semi(20.0)).color(pal().header_primary));
            ui.add_space(8.0);
            if list.is_empty() {
                ui.label(RichText::new("You can't add emoji or stickers to any of your servers.").color(pal().header_secondary));
            }
            for (g, n) in &list {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 4, pal().control_hover);
                }
                super::text::paint(ui.painter(), r.left_center() + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, n, medium(14.0), pal().header_primary, r.width() - 16.0);
                if resp.clicked() {
                    pick = Some(*g);
                }
            }
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Label::new(RichText::new("Cancel").color(pal().header_primary)).sense(Sense::click())).clicked() {
                    close = true;
                }
            });
        });
    if let Some(g) = pick {
        clone_to(app, g, what);
        close = true;
    }
    if close || modal.should_close() {
        app.plug.cloning = None;
    }
}

fn clone_to(app: &mut App, guild: Id, what: Cloning) {
    let Some(api) = outgoing::api(app) else { return };
    outgoing::spawn(app, async move {
        let res = async {
            match what {
                Cloning::Emoji(id, name, animated) => {
                    let ext = if animated { "gif" } else { "png" };
                    let bytes = crate::api::http().get(format!("{}/emojis/{id}.{ext}?size=128", crate::api::cdn())).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
                    let image = format!("data:image/{ext};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
                    api.call(reqwest::Method::POST, &format!("/guilds/{guild}/emojis"), Some(json!({ "name": name, "image": image }))).await.map(|_| format!("Cloned {name}"))
                }
                Cloning::Sticker(id, name, fmt) => {
                    let bytes = crate::api::http().get(sticker_url(id, fmt)).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
                    api.clone_sticker(guild, &name, bytes.to_vec(), if fmt == 4 { "gif" } else { "png" }).await.map(|_| format!("Cloned {name}"))
                }
            }
        };
        Done::Toast(res.await.unwrap_or_else(|e| format!("Failed to clone: {e}")))
    });
}
