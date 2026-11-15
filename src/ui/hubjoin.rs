//! Joining a Student Hub: a school email, the school it belongs to (or the waitlist when it has
//! no hub yet), then the code discord mails out.

use egui::{Align2, Rect, RichText, Sense, Ui, pos2, vec2};
use serde_json::{Value, json};

use super::create::{field, label, modal};
use super::{App, modal_button};
use crate::net::Tag;
use crate::theme::*;

const SEND: &str = "/guilds/automations/email-domain-lookup";

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Step {
    #[default]
    Email,
    School,
    Waitlist,
    Listed,
    Code,
}

#[derive(Default)]
pub struct Join {
    pub step: Step,
    pub email: String,
    pub school: String,
    pub code: String,
    search: String,
    guild: Option<String>,
    schools: Vec<Value>,
    error: Option<String>,
}

/// What discord does with the answer to a school email: straight to the code when the one hub
/// is known, a choice between several, or the waitlist when there is none.
pub fn after_email(answer: &Value, asked_for: Option<&str>) -> (Step, Option<String>) {
    let schools = answer["guilds_info"].as_array().map_or(&[][..], |a| a.as_slice());
    if answer["has_matching_guild"] == true {
        (Step::Code, asked_for.map(str::to_string))
    } else {
        match schools {
            [] => (Step::Waitlist, None),
            [one] => (Step::Code, one["id"].as_str().map(str::to_string)),
            _ => (Step::School, None),
        }
    }
}

fn send(app: &mut App, email: &str, guild: Option<&str>) {
    app.keyed.insert("hub_email".into(), None);
    app.rest("POST", SEND.into(), Some(json!({ "email": email, "guild_id": guild, "allow_multiple_guilds": true, "use_verification_code": true })), Tag::Keyed("hub_email".into()));
}

fn settle(app: &mut App, j: &mut Join) -> Option<u64> {
    let mut joined = None;
    for key in ["hub_email", "hub_code", "hub_wait"] {
        if !matches!(app.keyed.get(key), Some(Some(_))) {
            continue;
        }
        let Some(Some(res)) = app.keyed.remove(key) else { continue };
        match (key, res) {
            (_, Err(e)) => j.error = Some(e),
            ("hub_email", Ok(v)) if j.guild.is_none() => {
                let (step, guild) = after_email(&v, None);
                j.schools = v["guilds_info"].as_array().cloned().unwrap_or_default();
                j.step = step;
                if let Some(g) = guild {
                    send(app, &j.email, Some(&g));
                    j.guild = Some(g);
                }
            }
            ("hub_email", Ok(_)) => j.step = Step::Code,
            ("hub_wait", Ok(_)) => j.step = Step::Listed,
            (_, Ok(v)) => joined = v["guild"]["id"].as_str().and_then(|g| g.parse().ok()),
        }
    }
    joined
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(reg(16.0)).color(pal().header_secondary));
    ui.add_space(16.0);
}

fn problem(ui: &mut Ui, e: &Option<String>) {
    if let Some(e) = e {
        ui.add_space(4.0);
        ui.label(RichText::new(e).font(reg(12.0)).color(pal().critical));
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut j) = app.hub_join.take() else { return };
    if let Some(g) = settle(app, &mut j) {
        app.select_guild(Some(g));
        return;
    }
    let busy = ["hub_email", "hub_code", "hub_wait"].iter().any(|k| matches!(app.keyed.get(*k), Some(None)));
    enum Act {
        Email,
        Pick(String),
        ToWaitlist,
        Waitlist,
        Back,
        Again,
        Resend,
        Code,
    }
    let mut act = None;
    let title = match j.step {
        Step::Email => "Find study groups, clubs, and friends in the Student Hub for your school!",
        Step::School => "Select your school",
        Step::Waitlist => "Join the waitlist",
        Step::Listed => "Thanks! You're on the waitlist.",
        Step::Code => "Verify your email to join",
    };
    let closed = modal(ctx, "hub_join", Some(title), |ui| {
        let full = ui.available_width();
        match j.step {
            Step::Email => {
                label(ui, "Your Student Email");
                field(ui, &mut j.email, "name@school.edu", None, 1, None);
                problem(ui, &j.error);
                ui.add_space(8.0);
                ui.label(RichText::new("Hubs are not affiliated with or managed by schools. Servers listed in the school's Student Hub are run by students. Users do not need to be a member of a Student Hub to be invited to join a server listed there.").font(reg(12.0)).color(pal().text_muted));
                ui.add_space(24.0);
                let go = ui.add_enabled_ui(!busy && j.email.contains('@'), |ui| modal_button(ui, "Join Hub", Some(pal().blurple), full)).inner;
                super::settings::mark(ui, "hub_join:go", go.rect);
                if go.clicked() {
                    act = Some(Act::Email);
                }
            }
            Step::School => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new("Can't find your school?").font(reg(16.0)).color(pal().header_secondary));
                    if ui.add(egui::Label::new(RichText::new("Join the waitlist.").font(reg(16.0)).color(pal().link)).sense(Sense::click())).clicked() {
                        act = Some(Act::ToWaitlist);
                    }
                });
                ui.add_space(16.0);
                field(ui, &mut j.search, "Search", Some(super::icons::Icon::Search), 1, None);
                problem(ui, &j.error);
                ui.add_space(8.0);
                let q = j.search.to_lowercase();
                let shown: Vec<&Value> = j.schools.iter().filter(|s| q.is_empty() || s["name"].as_str().unwrap_or("").to_lowercase().contains(&q)).collect();
                if shown.is_empty() {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("No matches found").font(semi(20.0)).color(pal().header_primary)));
                    ui.add_space(16.0);
                }
                let tall = (shown.len() as f32 * 56.0).min(240.0);
                egui::ScrollArea::vertical().max_height(tall).auto_shrink([false, false]).show(ui, |ui| {
                    for s in shown {
                        let id = s["id"].as_str().unwrap_or("");
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
                        ui.painter().rect_filled(r.shrink2(vec2(0.0, 4.0)), 8, if resp.hovered() { pal().control_hover } else { pal().control });
                        let icon = Rect::from_center_size(pos2(r.left() + 32.0, r.center().y), vec2(40.0, 40.0));
                        match s["icon"].as_str() {
                            Some(h) => super::avatar(ui, icon, &format!("{}/icons/{id}/{h}.png?size=80", crate::api::cdn())),
                            None => {
                                ui.painter().circle_filled(icon.center(), 20.0, pal().bg_accent);
                            }
                        }
                        super::text::paint(ui.painter(), pos2(icon.right() + 12.0, r.center().y), Align2::LEFT_CENTER, s["name"].as_str().unwrap_or(""), semi(16.0), pal().header_primary, r.width() - 80.0);
                        super::settings::mark(ui, &format!("hub_join:school:{id}"), r);
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() && !busy {
                            act = Some(Act::Pick(id.to_string()));
                        }
                    }
                });
            }
            Step::Waitlist => {
                note(ui, "The Hub for your school isn't live yet. Enter your school's name and we'll let you know when it is!");
                label(ui, "School Name");
                field(ui, &mut j.school, "Full school name", None, 1, None);
                problem(ui, &j.error);
                ui.add_space(24.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let w = (full - 8.0) / 2.0;
                    if modal_button(ui, "Back", None, w).clicked() {
                        act = Some(Act::Back);
                    }
                    let go = ui.add_enabled_ui(!busy && !j.school.trim().is_empty(), |ui| modal_button(ui, "Join Waitlist", Some(pal().blurple), w)).inner;
                    super::settings::mark(ui, "hub_join:waitlist", go.rect);
                    if go.clicked() {
                        act = Some(Act::Waitlist);
                    }
                });
            }
            Step::Listed => {
                note(ui, &format!("We'll reach out when the Student Hub for {} is live.", j.school.trim()));
                if modal_button(ui, "Try A Different Email", Some(pal().blurple), full).clicked() {
                    act = Some(Act::Again);
                }
            }
            Step::Code => {
                ui.label(RichText::new("Your code has been emailed to your official school email.").font(reg(16.0)).color(pal().header_secondary));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new("Didn't get the code?").font(reg(16.0)).color(pal().header_secondary));
                    if ui.add(egui::Label::new(RichText::new("Resend code").font(reg(16.0)).color(pal().link)).sense(Sense::click())).clicked() {
                        act = Some(Act::Resend);
                    }
                    ui.label(RichText::new(format!("to {}", j.email)).font(reg(16.0)).color(pal().header_secondary));
                });
                ui.add_space(16.0);
                label(ui, "Enter Discord verification code");
                let r = field(ui, &mut j.code, "", None, 1, None);
                problem(ui, &j.error);
                ui.add_space(24.0);
                let go = ui.add_enabled_ui(!busy && !j.code.trim().is_empty(), |ui| modal_button(ui, "Submit", Some(pal().blurple), full)).inner;
                super::settings::mark(ui, "hub_join:submit", go.rect);
                if go.clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !j.code.trim().is_empty()) {
                    act = Some(Act::Code);
                }
            }
        }
    });
    if act.is_some() {
        j.error = None;
    }
    match act {
        Some(Act::Email) => {
            j.guild = None;
            send(app, &j.email.trim().to_string(), None);
        }
        Some(Act::Pick(g)) => {
            send(app, &j.email, Some(&g));
            j.guild = Some(g);
        }
        Some(Act::Resend) => send(app, &j.email, j.guild.as_deref()),
        Some(Act::ToWaitlist) => j.step = Step::Waitlist,
        Some(Act::Back) => j.step = if j.schools.len() > 1 { Step::School } else { Step::Email },
        Some(Act::Again) => j = Join::default(),
        Some(Act::Waitlist) => {
            app.keyed.insert("hub_wait".into(), None);
            app.rest("POST", "/hub-waitlist/signup".into(), Some(json!({ "email": j.email, "school": j.school.trim() })), Tag::Keyed("hub_wait".into()));
        }
        Some(Act::Code) => {
            app.keyed.insert("hub_code".into(), None);
            app.rest("POST", format!("{SEND}/verify-code"), Some(json!({ "code": j.code.trim(), "guild_id": j.guild, "email": j.email })), Tag::Keyed("hub_code".into()));
        }
        None => {}
    }
    if !closed {
        app.hub_join = Some(j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_school_email_leads_to_the_code_a_choice_or_the_waitlist() {
        assert!(after_email(&json!({ "has_matching_guild": false, "guilds_info": [] }), None) == (Step::Waitlist, None));
        assert!(after_email(&json!({ "guilds_info": [{ "id": "5" }] }), None) == (Step::Code, Some("5".into())));
        assert!(after_email(&json!({ "guilds_info": [{ "id": "5" }, { "id": "6" }] }), None) == (Step::School, None));
        assert!(after_email(&json!({ "has_matching_guild": true }), Some("9")) == (Step::Code, Some("9".into())));
    }
}
