//! Vencord's own settings section, as its tabs lay it out: Vencord, Themes, Updater, Cloud,
//! Backup & Restore (Plugins is its own file). The values are Vencord's settings.json keys.

use egui::{Frame, RichText, Ui, vec2};
use serde_json::{Value, json};

use super::{Look, action, button, caption, card, dropdown, input, note, primary, secondary, section, slider, sub, toggle};
use crate::theme::*;
use crate::ui::App;

fn get<'a>(app: &'a App, path: &[&str]) -> &'a Value {
    path.iter().fold(&app.vc.settings, |v, k| &v[*k])
}

fn set(app: &mut App, path: &[&str], v: Value) {
    app.vc_call("setPath", json!({ "path": path, "value": v }));
}

fn no_host(app: &App, ui: &mut Ui) -> bool {
    if app.vc.host.is_none() {
        note(ui, &format!("No Vencord here. Install Vencord (vencord.dev), or put plugin folders in {}.", crate::cache::dir().join("userplugins").display()));
        return true;
    }
    false
}

pub fn main(app: &mut App, ui: &mut Ui) -> bool {
    if no_host(app, ui) {
        return false;
    }
    card(ui, |ui| {
        ui.label(RichText::new("Support the Project").font(semi(20.0)).color(pal().header_primary));
        ui.add_space(4.0);
        ui.label(RichText::new("Please consider supporting the development of Vencord by donating!").font(reg(14.0)).color(pal().header_secondary));
        ui.add_space(12.0);
        if primary(ui, "Donate").clicked() {
            let _ = open::that_detached("https://github.com/sponsors/Vendicated");
        }
    });

    section(ui, "Quick Actions");
    if action(ui, "Notification Log", "Every notification Vencord plugins showed you.", "Open", Look::Secondary) {
        app.vc.log_open = true;
        app.vc_call("log", json!({}));
    }
    if action(ui, "Edit QuickCSS", "Your own CSS. Rustcord applies its colour variables.", "Edit", Look::Secondary) {
        app.settings_page = Some(super::Page::VcThemes);
    }
    if action(ui, "Relaunch Rustcord", "Starts Rustcord again, plugins and all.", "Relaunch", Look::Secondary) {
        crate::ui::vencord::relaunch(ui.ctx());
    }
    if action(ui, "Open Settings Folder", "Where Rustcord keeps Vencord's settings, QuickCSS and plugin data.", "Open", Look::Secondary) {
        let _ = open::that_detached(crate::cache::dir());
    }
    let remote = app.vc.host.as_ref().and_then(|h| h.version().1).unwrap_or_else(|| "Vendicated/Vencord".into());
    if action(ui, "View Source Code", &format!("github.com/{remote}"), "Open", Look::Secondary) {
        let _ = open::that_detached(format!("https://github.com/{remote}"));
    }

    section(ui, "Settings");
    let loc = get(app, &["plugins", "Settings", "settingsLocation"]).as_str().unwrap_or("aboveNitro").to_string();
    caption(ui, "Settings section position", "Where this Vencord section sits in the settings sidebar.");
    const LOCS: [(&str, &str); 6] = [("top", "At the very top"), ("aboveNitro", "Above the Nitro section"), ("belowNitro", "Below the Nitro section"), ("aboveActivity", "Above Activity Settings"), ("belowActivity", "Below Activity Settings"), ("bottom", "At the very bottom")];
    let shown = LOCS.iter().find(|(v, _)| *v == loc).map_or("Above the Nitro section", |(_, l)| *l);
    let mut pick = None;
    dropdown(ui, ui.available_width(), None, shown, |ui| {
        for (v, l) in LOCS {
            if ui.selectable_label(v == loc, l).clicked() {
                pick = Some(v);
            }
        }
    });
    if let Some(v) = pick {
        set(app, &["plugins", "Settings", "settingsLocation"], v.into());
    }
    super::end(ui, 24.0);
    for (key, title, desc) in [
        ("useQuickCss", "Enable Custom CSS", "Apply your configured QuickCSS"),
        ("winCtrlQ", "Register Ctrl+Q as shortcut to close Rustcord (Alternative to Alt+F4)", ""),
        ("disableMinSize", "Disable minimum window size", "Allows you to resize the window to any size, even smaller than Discord's minimum size. Takes effect next launch."),
    ] {
        let mut on = get(app, &[key]) == true;
        if toggle(ui, &mut on, title, desc) {
            set(app, &[key], on.into());
        }
    }

    section(ui, "Notifications");
    note(ui, "Settings for Notifications sent by Vencord. This does NOT include Discord notifications (messages, etc)");
    super::lead(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if primary(ui, "Notification Settings").clicked() {
            app.vc.notify_settings_open = true;
        }
        if secondary(ui, "View Notification Log").clicked() {
            app.vc.log_open = true;
            app.vc_call("log", json!({}));
        }
    });
    super::end(ui, 24.0);
    false
}

/// Vencord's Notification Settings modal.
pub fn notification_settings(app: &mut App, ui: &mut Ui) {
    let n = app.vc.settings["notifications"].clone();
    ui.label(RichText::new("Notification Style").font(semi(12.0)).color(pal().header_secondary));
    ui.add_space(4.0);
    ui.label(RichText::new("Some plugins may show you notifications. These come in two styles: Vencord Notifications, in the app, and Desktop Notifications, from Windows like when you get a ping.").font(reg(14.0)).color(pal().text_normal));
    ui.add_space(8.0);
    let style = n["useNative"].as_str().unwrap_or("not-focused").to_string();
    const STYLES: [(&str, &str); 3] = [("not-focused", "Only use Desktop notifications when Rustcord is not focused"), ("always", "Always use Desktop notifications"), ("never", "Always use Vencord notifications")];
    egui::ComboBox::from_id_salt("vc_notif_style").width(ui.available_width()).selected_text(STYLES.iter().find(|(v, _)| *v == style).map_or("", |(_, l)| *l)).show_ui(ui, |ui| {
        for (v, l) in STYLES {
            if ui.selectable_label(v == style, l).clicked() {
                set(app, &["notifications", "useNative"], v.into());
            }
        }
    });
    ui.add_space(16.0);
    ui.label(RichText::new("Notification Position").font(semi(12.0)).color(pal().header_secondary));
    let pos = n["position"].as_str().unwrap_or("bottom-right").to_string();
    ui.add_enabled_ui(style != "always", |ui| {
        egui::ComboBox::from_id_salt("vc_notif_pos").width(ui.available_width()).selected_text(if pos == "top-right" { "Top Right" } else { "Bottom Right" }).show_ui(ui, |ui| {
            for (v, l) in [("bottom-right", "Bottom Right"), ("top-right", "Top Right")] {
                if ui.selectable_label(v == pos, l).clicked() {
                    set(app, &["notifications", "position"], v.into());
                }
            }
        });
    });
    ui.add_space(16.0);
    let mut timeout = n["timeout"].as_f64().unwrap_or(5000.0) as f32;
    ui.label(RichText::new(format!("Notification Timeout: {:.2}s", timeout / 1000.0)).font(semi(12.0)).color(pal().header_secondary));
    ui.label(RichText::new("Set to 0s to never automatically time out").font(reg(13.0)).color(pal().text_muted));
    if slider(ui, ui.available_width(), &mut timeout, 0.0, 20_000.0) {
        set(app, &["notifications", "timeout"], ((timeout / 250.0).round() * 250.0).into());
    }
    ui.add_space(16.0);
    let mut limit = n["logLimit"].as_f64().unwrap_or(50.0) as f32;
    let shown = if limit >= 200.0 { "∞".to_string() } else { format!("{}", limit as u32) };
    ui.label(RichText::new(format!("Notification Log Limit: {shown}")).font(semi(12.0)).color(pal().header_secondary));
    ui.label(RichText::new("The amount of notifications to save in the log until old ones are removed. Set to 0 to disable Notification log and ∞ to never automatically remove old Notifications").font(reg(13.0)).color(pal().text_muted));
    if slider(ui, ui.available_width(), &mut limit, 0.0, 200.0) {
        let stops = [0.0f32, 25.0, 50.0, 75.0, 100.0, 200.0];
        let v = stops.iter().copied().min_by(|a, b| (a - limit).abs().total_cmp(&(b - limit).abs())).unwrap_or(50.0);
        set(app, &["notifications", "logLimit"], (v as u64).into());
    }
}

/// Vencord's Notification Log modal.
pub fn notification_log(app: &mut App, ui: &mut Ui) {
    let log = app.vc.log.clone().unwrap_or(Value::Array(Vec::new()));
    let list = log.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No notifications yet").font(semi(16.0)).color(pal().text_muted)));
        ui.add_space(24.0);
    }
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for n in &list {
            Frame::new().fill(pal().bg_secondary).corner_radius(8).inner_margin(12).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if let Some(icon) = n["icon"].as_str() {
                        ui.add(egui::Image::from_uri(icon.to_string()).fit_to_exact_size(vec2(32.0, 32.0)).corner_radius(16));
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(n["title"].as_str().unwrap_or("")).font(semi(15.0)).color(pal().header_primary));
                        ui.label(RichText::new(n["body"].as_str().unwrap_or("")).font(reg(14.0)).color(pal().text_normal));
                        let at = n["timestamp"].as_i64().and_then(chrono::DateTime::from_timestamp_millis).map(|t| t.with_timezone(&chrono::Local).format("%b %-d, %Y %-I:%M %p").to_string()).unwrap_or_default();
                        ui.label(RichText::new(at).font(reg(12.0)).color(pal().text_muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if crate::ui::icon_button(ui, crate::ui::icons::Icon::Close, 20.0, "Remove").clicked() {
                            app.vc_call("logDelete", json!({ "id": n["id"] }));
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if button(ui, "Clear Notification Log", Look::Danger).clicked() && !list.is_empty() {
            app.vc_call("logDelete", json!({ "id": null }));
        }
        if secondary(ui, "Notification Settings").clicked() {
            app.vc.notify_settings_open = true;
        }
    });
}

/// A theme file's name and its BetterDiscord-style header: (file, name, description, author).
fn themes() -> Vec<(String, String, String, String)> {
    let mut out: Vec<(String, String, String, String)> = Vec::new();
    for dir in [crate::vencord::vencord_dir().join("themes"), crate::cache::dir().join("themes")] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            if !file.ends_with(".css") || out.iter().any(|(f, ..)| *f == file) {
                continue;
            }
            let head = std::fs::read_to_string(e.path()).unwrap_or_default();
            let meta = |k: &str| head.lines().find_map(|l| l.trim().trim_start_matches(['*', '/', ' ']).strip_prefix(k).map(|v| v.trim().to_string())).unwrap_or_default();
            let name = Some(meta("@name")).filter(|n| !n.is_empty()).unwrap_or_else(|| file.trim_end_matches(".css").to_string());
            out.push((file, name, meta("@description"), meta("@author")));
        }
    }
    out
}

pub fn themes_page(app: &mut App, ui: &mut Ui) -> bool {
    if no_host(app, ui) {
        return false;
    }
    super::lead(ui);
    ui.horizontal(|ui| {
        for (i, t) in ["Local Themes", "Online Themes"].iter().enumerate() {
            let on = app.vc.themes_tab == i as u8;
            if ui.add(egui::Button::selectable(on, RichText::new(*t).font(medium(16.0)).color(if on { pal().header_primary } else { pal().text_muted })).frame(false)).clicked() {
                app.vc.themes_tab = i as u8;
            }
            ui.add_space(16.0);
        }
    });
    super::end(ui, 24.0);
    card(ui, |ui| {
        ui.label(RichText::new("Rustcord applies the colour variables themes set (--background-primary and the rest); Discord's layout rules in them have nothing to style here.").font(reg(14.0)).color(pal().header_secondary));
    });
    if app.vc.themes_tab == 0 {
        sub(ui, "Find Themes:");
        note(ui, "Themes from BetterDiscord work too: drop .css files in the themes folder.");
        super::lead(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if secondary(ui, "Open Themes Folder").clicked() {
                let dir = crate::vencord::vencord_dir().join("themes");
                let dir = if dir.exists() { dir } else { crate::cache::dir().join("themes") };
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
            if secondary(ui, "Load missing Themes").clicked() {
                app.vc.restyle = true;
            }
            if secondary(ui, "BetterDiscord Themes").clicked() {
                let _ = open::that_detached("https://betterdiscord.app/themes");
            }
        });
        super::end(ui, 24.0);
        let enabled: Vec<String> = app.vc.settings["enabledThemes"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
        let list = themes();
        if list.is_empty() {
            note(ui, "No themes in the themes folder yet.");
        }
        for (file, name, desc, author) in list {
            let mut on = enabled.contains(&file);
            let detail = [desc, if author.is_empty() { String::new() } else { format!("by {author}") }].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
            if toggle(ui, &mut on, &name, &detail) {
                let mut next: Vec<Value> = enabled.iter().filter(|f| **f != file).map(|f| Value::from(f.as_str())).collect();
                if on {
                    next.push(file.into());
                }
                set(app, &["enabledThemes"], next.into());
            }
        }
    } else {
        sub(ui, "Paste links to css files");
        note(ui, "One link per line. Prefix with @light or @dark to only use it with that theme.");
        super::lead(ui);
        let id = egui::Id::new("vc_theme_links");
        let saved = app.vc.settings["themeLinks"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or(saved);
        let r = Frame::new().fill(pal().input_bg).corner_radius(8).inner_margin(10).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).desired_rows(6).desired_width(ui.available_width()).font(egui::FontId::monospace(13.0)).hint_text("https://example.com/theme.css"))
        });
        if r.inner.changed() {
            let links: Vec<Value> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(Value::from).collect();
            set(app, &["themeLinks"], links.into());
        }
        ui.data_mut(|d| d.insert_temp(id, text));
        super::end(ui, 24.0);
    }

    super::divider(ui);
    section(ui, "QuickCSS");
    let mut on = get(app, &["useQuickCss"]) == true;
    if toggle(ui, &mut on, "Enable Custom CSS", "Apply your configured QuickCSS") {
        set(app, &["useQuickCss"], on.into());
    }
    super::lead(ui);
    let id = egui::Id::new("vc_quickcss");
    let mut css = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(crate::ui::vencord::quick_css);
    let r = Frame::new().fill(pal().bg_code).corner_radius(8).inner_margin(10).show(ui, |ui| {
        ui.add(egui::TextEdit::multiline(&mut css).code_editor().desired_rows(14).desired_width(ui.available_width()).hint_text(":root {\n    --background-primary: #1e1e2e;\n}"))
    });
    if r.inner.changed() {
        crate::ui::vencord::set_quick_css(&css);
        app.vc.restyle = true;
    }
    ui.data_mut(|d| d.insert_temp(id, css));
    super::end(ui, 24.0);
    false
}

pub fn updater(app: &mut App, ui: &mut Ui) -> bool {
    if no_host(app, ui) {
        return false;
    }
    let (hash, remote) = app.vc.host.as_ref().map(|h| h.version()).unwrap_or_default();
    let remote = remote.unwrap_or_else(|| "Vendicated/Vencord".into());
    let hash = hash.unwrap_or_else(|| "unknown".into());
    section(ui, "Updater Settings");
    for (key, title, desc) in [
        ("autoUpdate", "Automatically update", "Automatically update Vencord without confirmation prompt"),
        ("autoUpdateNotification", "Get notified when an automatic update completes", "Shows a notification when Vencord automatically updates"),
    ] {
        let mut on = get(app, &[key]) == true;
        if toggle(ui, &mut on, title, desc) {
            set(app, &[key], on.into());
        }
    }
    section(ui, "Repo");
    super::lead(ui);
    ui.horizontal(|ui| {
        if super::link(ui, &remote).clicked() {
            let _ = open::that_detached(format!("https://github.com/{remote}"));
        }
        ui.label(RichText::new(format!(" ({hash})")).font(reg(14.0)).color(pal().text_muted));
    });
    super::end(ui, 24.0);
    section(ui, "Updates");
    match app.vcnet.updates.clone() {
        None => note(ui, "Not checked yet."),
        Some(Err(e)) => note(ui, &format!("Couldn't check for updates: {e}")),
        Some(Ok(v)) if v["outdated"] != true => note(ui, "Up to Date!"),
        Some(Ok(v)) => {
            let commits = v["commits"].as_array().cloned().unwrap_or_default();
            note(ui, &format!("There are {} updates. Updating replaces the files in Vencord's dist folder, so Discord gets the update too (after it restarts).", commits.len().max(1)));
            card(ui, |ui| {
                for c in &commits {
                    ui.horizontal_wrapped(|ui| {
                        if super::link(ui, c["hash"].as_str().unwrap_or("")).clicked() {
                            let _ = open::that_detached(format!("https://github.com/{remote}/commit/{}", c["hash"].as_str().unwrap_or("")));
                        }
                        ui.label(RichText::new(c["message"].as_str().unwrap_or("")).font(reg(14.0)).color(pal().text_normal));
                        ui.label(RichText::new(format!("- {}", c["author"].as_str().unwrap_or(""))).font(reg(13.0)).color(pal().text_muted));
                    });
                }
            });
        }
    }
    super::lead(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let busy = app.vcnet.busy.is_some();
        let outdated = matches!(&app.vcnet.updates, Some(Ok(v)) if v["outdated"] == true);
        if outdated && ui.add_enabled_ui(!busy, |ui| primary(ui, "Update Now")).inner.clicked() {
            app.vc_update();
        }
        if ui.add_enabled_ui(!busy, |ui| secondary(ui, "Check for Updates")).inner.clicked() {
            app.vc_check_updates();
        }
        if busy {
            ui.add(egui::Spinner::new().color(pal().text_muted));
        }
    });
    super::end(ui, 24.0);
    false
}

pub fn cloud(app: &mut App, ui: &mut Ui) -> bool {
    if no_host(app, ui) {
        return false;
    }
    section(ui, "Cloud Settings");
    note(ui, "Vencord comes with a cloud integration that adds goodies like settings sync across devices. It respects your privacy, and the source code is AGPL 3.0 licensed so you can host it yourself.");
    let authed = get(app, &["cloud", "authenticated"]) == true;
    let mut on = authed;
    if toggle(ui, &mut on, "Enable Cloud Integrations", "This will request authorization if you have not yet set up cloud integrations.") {
        if on {
            app.vc_cloud_setup();
        } else {
            app.vc_cloud_forget();
        }
    }
    caption(ui, "Backend URL", "Which backend to use when using cloud integrations.");
    let id = egui::Id::new("vc_cloud_url");
    let mut url = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| app.vc_cloud_url());
    if input(ui, &mut url, "https://api.vencord.dev/", false).changed() && url.starts_with("http") {
        set(app, &["cloud", "url"], url.clone().into());
        set(app, &["cloud", "authenticated"], false.into());
    }
    ui.data_mut(|d| d.insert_temp(id, url));
    super::end(ui, 24.0);
    ui.add_enabled_ui(authed, |ui| {
        section(ui, "Settings Sync");
        let mut sync = get(app, &["cloud", "settingsSync"]) == true;
        if toggle(ui, &mut sync, "Settings Sync", "Synchronize your settings to the cloud. This allows easy synchronization across multiple devices with minimal effort.") {
            set(app, &["cloud", "settingsSync"], sync.into());
        }
        caption(ui, "Sync Rules for This Device", "");
        let dir = app.vc.data_values.get("Vencord_cloudSyncDirection").and_then(|v| v.as_str()).unwrap_or("both").to_string();
        const DIRS: [(&str, &str); 4] = [("both", "Two-way sync"), ("push", "Push only (this device's settings go up)"), ("pull", "Pull only (this device takes the cloud's)"), ("manual", "Manual only")];
        let mut pick = None;
        dropdown(ui, ui.available_width(), None, DIRS.iter().find(|(v, _)| *v == dir).map_or("", |(_, l)| *l), |ui| {
            for (v, l) in DIRS {
                if ui.selectable_label(v == dir, l).clicked() {
                    pick = Some(v);
                }
            }
        });
        if let Some(v) = pick {
            app.vc_call("dataSet", json!({ "key": "Vencord_cloudSyncDirection", "value": v }));
            app.vc.data_values.insert("Vencord_cloudSyncDirection".into(), v.into());
        }
        super::end(ui, 16.0);
        super::lead(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let busy = app.vcnet.busy.is_some();
            ui.add_enabled_ui(sync && !busy, |ui| {
                if secondary(ui, "Sync to Cloud").clicked() {
                    app.vc_cloud_push(true);
                }
                if secondary(ui, "Sync from Cloud").clicked() {
                    app.vc_cloud_pull(true);
                }
                if button(ui, "Delete Cloud Settings", Look::Danger).clicked() {
                    app.vc_cloud_delete(false);
                }
            });
        });
        super::end(ui, 24.0);
        if action(ui, "Erase All Data", "Removes everything Vencord Cloud keeps for this account and turns cloud integrations off.", "Erase All Data", Look::Filled) {
            app.vc_cloud_delete(true);
        }
    });
    false
}

pub fn backup(app: &mut App, ui: &mut Ui) -> bool {
    if no_host(app, ui) {
        return false;
    }
    super::lead(ui);
    Frame::new().fill(YELLOW.gamma_multiply(0.15)).stroke(egui::Stroke::new(1.0, YELLOW)).corner_radius(8).inner_margin(12).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Warning").font(semi(16.0)).color(pal().header_primary));
        ui.label(RichText::new("Importing a settings file will overwrite your current settings.").font(reg(14.0)).color(pal().text_normal));
    });
    super::end(ui, 16.0);
    note(ui, "You can import and export your Vencord settings as a JSON file. This allows you to easily transfer your settings to another device, or recover your settings after reinstalling Vencord or Discord. Files from Vencord itself work here, and Rustcord's work in Vencord.");
    sub(ui, "Settings Export contains:");
    note(ui, "— Custom QuickCSS\n— Theme Links\n— Plugin Settings");
    super::lead(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if primary(ui, "Import Settings").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Vencord Settings Backup", &["json"]).pick_file()
        {
            match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| format!("Failed to parse JSON: {e}"))) {
                Ok(v) if v["settings"].is_object() && v.get("quickCss").is_some() => {
                    app.vc_import(v["settings"].clone(), v["quickCss"].as_str().unwrap_or(""));
                    app.toast("Settings successfully imported.");
                }
                Ok(_) => app.toast("Invalid Settings. Is this even a Vencord Settings file?"),
                Err(e) => app.toast(format!("Failed to import settings: {e}")),
            }
        }
        if secondary(ui, "Export Settings").clicked() {
            let name = format!("vencord-settings-backup-{}.json", chrono::Local::now().format("%Y-%m-%d"));
            if let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() {
                match std::fs::write(&path, app.vc_export(true)) {
                    Ok(_) => app.toast("Settings exported."),
                    Err(e) => app.toast(format!("Couldn't export settings: {e}")),
                }
            }
        }
    });
    super::end(ui, 24.0);
    false
}
