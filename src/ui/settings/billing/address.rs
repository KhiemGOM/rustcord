use egui::{RichText, Ui};
use serde_json::{Value, json};

use super::{COUNTRIES, country, get, input, mark, put};
use crate::theme::*;
use crate::ui::settings::dropdown;

const NAME: usize = 0;
const LINE_1: usize = 1;
const LINE_2: usize = 2;
const CITY: usize = 3;
const STATE: usize = 4;
const POSTAL: usize = 5;
const COUNTRY: usize = 6;
const MONTH: usize = 7;
const YEAR: usize = 8;

fn seed(s: &Value) -> Vec<String> {
    let a = &s["billing_address"];
    let text = |v: &Value| v.as_str().unwrap_or("").to_string();
    let num = |v: &Value| v.as_u64().map_or(String::new(), |n| n.to_string());
    vec![text(&a["name"]), text(&a["line_1"]), text(&a["line_2"]), text(&a["city"]), text(&a["state"]), text(&a["postal_code"]), text(&a["country"]), num(&s["expires_month"]), num(&s["expires_year"])]
}

fn label(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text.to_uppercase()).font(semi(12.0)).color(pal().header_secondary));
    ui.add_space(6.0);
}

fn field(ui: &mut Ui, form: &mut [String], at: usize, name: &str, hint: &str, key: &str) {
    label(ui, name);
    let r = input(ui, &mut form[at], hint, false);
    mark(ui, key, r.rect);
}

/// The fields of the address and, for a card, its expiry; what's typed stays until the dialog is left.
pub(super) fn fields(ui: &mut Ui, s: &Value) {
    let id = s["id"].as_str().unwrap_or_default().to_string();
    if get::<String>(ui, "form_for") != id {
        put(ui, "form", seed(s));
        put(ui, "form_for", id);
    }
    let mut form: Vec<String> = get(ui, "form");
    let card = s["type"] == 1;
    let tall = ui.ctx().content_rect().height() - 330.0;
    egui::ScrollArea::vertical().max_height(tall.max(120.0)).auto_shrink([false, true]).show(ui, |ui| {
        ui.label(RichText::new("BILLING ADDRESS").font(semi(12.0)).color(pal().header_secondary));
        field(ui, &mut form, NAME, "Name", "", "billing:addr:name");
        field(ui, &mut form, LINE_1, "Address", "", "billing:addr:line_1");
        field(ui, &mut form, LINE_2, "Address 2 (Optional)", "Apt, Ste, Dimension", "billing:addr:line_2");
        ui.columns(2, |c| {
            field(&mut c[0], &mut form, CITY, "City", "", "billing:addr:city");
            field(&mut c[1], &mut form, STATE, "State", "", "billing:addr:state");
        });
        ui.columns(2, |c| {
            field(&mut c[0], &mut form, POSTAL, "Postal Code", "", "billing:addr:postal_code");
            label(&mut c[1], "Country");
            let shown = if form[COUNTRY].is_empty() { "Select".to_string() } else { country(&form[COUNTRY]) };
            let w = c[1].available_width();
            let picked = dropdown(&mut c[1], w, None, &shown, |ui| {
                for (code, name) in COUNTRIES {
                    if ui.selectable_label(form[COUNTRY].eq_ignore_ascii_case(code), name).clicked() {
                        form[COUNTRY] = code.to_string();
                    }
                }
            });
            mark(&c[1], "billing:addr:country", picked.rect);
        });
        if card {
            ui.columns(2, |c| {
                field(&mut c[0], &mut form, MONTH, "Expiration Month", "MM", "billing:addr:month");
                field(&mut c[1], &mut form, YEAR, "Expiration Year", "YYYY", "billing:addr:year");
            });
        }
    });
    put(ui, "form", form);
}

/// What the edit sends: the address as the form has it, the expiry for a card, and the default flag.
pub(super) fn body(ui: &Ui, s: &Value, default: bool) -> Value {
    let form: Vec<String> = get(ui, "form");
    let mut b = json!({
        "billing_address": {
            "name": form[NAME], "line_1": form[LINE_1], "line_2": form[LINE_2], "city": form[CITY],
            "state": form[STATE], "postal_code": form[POSTAL], "country": form[COUNTRY],
        },
        "default": default,
    });
    if s["type"] == 1 {
        for (key, at) in [("expires_month", MONTH), ("expires_year", YEAR)] {
            if let Ok(n) = form[at].trim().parse::<u32>() {
                b[key] = json!(n);
            }
        }
    }
    b
}
