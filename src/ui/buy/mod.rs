//! Buying inside the client: a Shop item (Orbs or a saved payment method), a gift of either, a
//! Nitro subscription or gift, Server Boosts, and a server's own products. One dialog walks the
//! steps Discord's checkout does: the item, who it's for, how to pay, the server's own total
//! and tax, then Purchase. The only call that spends anything is sent from the click on that
//! button, once, by `purchase`; adding a card stays in Discord's small window.

mod req;
pub mod server_shop;
mod view;

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::App;
use super::settings::refetch;
use crate::net::{Cmd, Tag};
use crate::state::{FRIEND, Id};
use req::{Invoice, Line, Outcome};

pub use req::{BASIC_MONTH, BASIC_YEAR, NITRO_MONTH, NITRO_YEAR};

const POLL: Duration = Duration::from_secs(3);
const GIFT_STYLES: [(u8, &str); 7] = [(1, "Snowglobe"), (2, "Box"), (3, "Cup"), (4, "Standard Box"), (5, "Cake"), (6, "Chest"), (7, "Coffee")];
const KEYS: [&str; 6] = ["shop_owned", "shop_orbs", "nitro_subs", "subs", "payments", "slots"];
const EEA: [&str; 31] = [
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR", "GR", "HR", "HU", "IE", "IS", "IT", "LI", "LT", "LU", "LV", "MT", "NL", "NO", "PL", "PT", "RO", "SE", "SI", "SK",
];

#[derive(Clone)]
pub enum What {
    Shop(Value),
    Listing(Value),
    Nitro(String),
    NitroGift,
    Boost(Id),
}

#[derive(Clone, PartialEq)]
pub enum Step {
    Plans,
    Review,
    Sending,
    Confirming { payment: String },
    Done(String),
    Failed { msg: String, unsure: bool },
}

#[derive(Default)]
struct Preview {
    key: String,
    seq: u64,
    main: Option<Result<Invoice, String>>,
    renewal: Option<Result<Invoice, String>>,
    asked: u8,
}

pub struct Buy {
    pub what: What,
    pub step: Step,
    gift: bool,
    to: Option<Id>,
    style: u8,
    plan: Option<String>,
    qty: u32,
    sources: Option<Vec<Value>>,
    source: Option<String>,
    orbs: Option<u64>,
    pay_orbs: bool,
    subs: Option<Vec<Value>>,
    slots: Option<Vec<Value>>,
    plans: [Option<Vec<Value>>; 2],
    prev: Preview,
    waiver: bool,
    load: String,
    sent: bool,
    card_window: bool,
    polled: Option<Instant>,
    polling: bool,
    pub filter: String,
    pub message: String,
    seq: u64,
    note: String,
}

fn random_id() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn purchase_token() -> String {
    let path = crate::cache::dir().join("purchase_token");
    let now = chrono::Utc::now().timestamp();
    if let Some((t, until)) = std::fs::read_to_string(&path).ok().and_then(|s| s.split_once(' ').map(|(t, u)| (t.to_string(), u.parse::<i64>().unwrap_or(0))))
        && until > now
        && !t.is_empty()
    {
        return t;
    }
    let t = random_id();
    let _ = std::fs::write(&path, format!("{t} {}", now + 60 * 86_400));
    t
}

impl App {
    pub fn open_buy(&mut self, what: What) {
        if self.buy.is_some() {
            return;
        }
        let gift = matches!(what, What::NitroGift);
        let plan = match &what {
            What::Nitro(p) => Some(p.clone()),
            _ => None,
        };
        let step = if gift { Step::Plans } else { Step::Review };
        self.buy = Some(Buy {
            step,
            gift,
            to: None,
            style: 4,
            message: String::new(),
            plan,
            qty: 1,
            sources: None,
            source: None,
            orbs: None,
            pay_orbs: matches!(&what, What::Shop(p) if !shop_money(p)),
            subs: None,
            slots: None,
            plans: [None, None],
            prev: Preview::default(),
            waiver: false,
            load: random_id(),
            sent: false,
            card_window: false,
            polled: None,
            polling: false,
            filter: String::new(),
            seq: 0,
            note: String::new(),
            what: what.clone(),
        });
        let get = |app: &App, name: &'static str, path: String| app.rest("GET", path, None, Tag::Buy(name, 0));
        get(self, "sources", "/users/@me/billing/payment-sources".into());
        match &what {
            What::Shop(p) if shop_orbs(p).is_some() => get(self, "orbs", "/users/@me/virtual-currency/balance".into()),
            What::Nitro(_) => {
                get(self, "subs", "/users/@me/billing/subscriptions?sync_level=1".into());
                get(self, "plans0", format!("/store/published-listings/skus/{}/subscription-plans?revenue_surface=0", crate::ui::settings::billing::NITRO_SKU));
            }
            What::NitroGift => {
                for (i, sku) in [crate::ui::settings::billing::NITRO_SKU, req::BASIC_SKU].into_iter().enumerate() {
                    get(self, ["plans0", "plans1"][i], format!("/store/published-listings/skus/{sku}/subscription-plans?revenue_surface=0"));
                }
            }
            What::Boost(_) => {
                get(self, "subs", "/users/@me/billing/subscriptions?sync_level=1".into());
                get(self, "slots", "/users/@me/guilds/premium/subscription-slots".into());
                get(self, "plans0", format!("/store/published-listings/skus/{}/subscription-plans?revenue_surface=0", req::BOOST_SKU));
            }
            _ => {}
        }
    }
}

pub fn shop_orbs(p: &Value) -> Option<u64> {
    ["4", "0"].iter().find_map(|k| {
        p["prices"][*k]["country_prices"]["prices"].as_array()?.iter().find(|x| x["currency"] == "discord_orb").and_then(|x| x["amount"].as_u64())
    })
}

fn shop_money(p: &Value) -> bool {
    ["4", "0"].iter().any(|k| p["prices"][*k]["country_prices"]["prices"].as_array().is_some_and(|l| l.iter().any(|x| x["currency"] != "discord_orb")))
}

fn shop_currency(p: &Value) -> String {
    ["4", "0"]
        .iter()
        .find_map(|k| p["prices"][*k]["country_prices"]["prices"].as_array()?.iter().find(|x| x["currency"] != "discord_orb").and_then(|x| x["currency"].as_str()))
        .unwrap_or("usd")
        .to_lowercase()
}

pub fn usable(s: &Value) -> bool {
    matches!(s["type"].as_u64(), Some(1 | 2 | 11)) && s["invalid"] != true
}

fn is_free(slot: &Value) -> bool {
    slot["premium_guild_subscription"].is_null() && slot["canceled"] != true && slot["cooldown_ends_at"].is_null()
}

impl Buy {
    pub fn paying_orbs(&self) -> bool {
        self.pay_orbs
    }

    fn orbs_price(&self) -> Option<u64> {
        match &self.what {
            What::Shop(p) if !self.gift => shop_orbs(p),
            _ => None,
        }
    }

    fn money_priced(&self) -> bool {
        match &self.what {
            What::Shop(p) => shop_money(p),
            _ => true,
        }
    }

    pub(crate) fn free_slots(&self) -> usize {
        self.slots.as_ref().map_or(0, |s| s.iter().filter(|x| is_free(x)).count())
    }

    fn plan_currency(&self) -> String {
        let plans = if matches!(self.what, What::NitroGift) {
            let i = usize::from(self.plan.as_deref().is_some_and(|p| matches!(p, BASIC_MONTH | BASIC_YEAR)));
            self.plans[i].as_ref()
        } else {
            self.plans[0].as_ref()
        };
        plans.and_then(|p| p.first()).and_then(|p| p["currency"].as_str()).unwrap_or("usd").to_lowercase()
    }

    fn currency(&self) -> String {
        match &self.what {
            What::Shop(p) => shop_currency(p),
            What::Listing(_) => String::new(),
            _ => self.plan_currency(),
        }
    }

    fn mine(&self) -> Option<&Value> {
        self.subs.as_ref()?.iter().find(|s| s["type"] == 1 && matches!(s["status"].as_u64(), Some(1..=3)))
    }

    fn line(&self) -> Option<Line> {
        match &self.what {
            What::Shop(p) => Some(Line::Sku { sku: p["sku_id"].as_str()?.to_string(), plan: None }),
            What::Listing(l) => Some(Line::Sku { sku: l["id"].as_str()?.to_string(), plan: None }),
            What::NitroGift => {
                let plan = self.plan.clone()?;
                Some(Line::Sku { sku: req::nitro_sku(&plan).to_string(), plan: Some(plan) })
            }
            What::Nitro(_) => Some(Line::New { plan: self.plan.clone()?, qty: 1 }),
            What::Boost(_) => {
                let sub = self.mine();
                let year = sub.is_some_and(|s| s["items"].as_array().is_some_and(|i| i.iter().any(|x| x["plan_id"].as_str().is_some_and(req::yearly))));
                let plan = req::boost_plan(year).to_string();
                let Some(sub) = sub else { return Some(Line::New { plan, qty: self.qty }) };
                let have = |p: &str| sub["items"].as_array().into_iter().flatten().find(|x| x["plan_id"] == p);
                let mut items: Vec<(Option<String>, String, u32)> = Vec::new();
                if let Some(n) = sub["items"].as_array().into_iter().flatten().find(|x| x["plan_id"].as_str().is_some_and(|p| req::NITRO_MONTH == p || req::NITRO_YEAR == p || req::BASIC_MONTH == p || req::BASIC_YEAR == p)) {
                    items.push((n["id"].as_str().map(str::to_string), n["plan_id"].as_str()?.to_string(), n["quantity"].as_u64().unwrap_or(1) as u32));
                }
                let (id, had) = have(&plan).map_or((None, 0), |x| (x["id"].as_str().map(str::to_string), x["quantity"].as_u64().unwrap_or(0) as u32));
                items.push((id, plan, had + self.qty));
                Some(Line::Add { sub: sub["id"].as_str()?.to_string(), items })
            }
        }
    }

    fn selected(&self) -> Option<&Value> {
        let id = self.source.as_deref()?;
        self.sources.as_ref()?.iter().find(|s| s["id"] == id)
    }

    fn want(&self) -> Option<String> {
        if self.pay_orbs || !self.money_priced() {
            return None;
        }
        match &self.what {
            What::Nitro(_) if self.subs.is_none() || self.mine().is_some() => return None,
            What::Boost(_) if self.subs.is_none() || self.slots.is_none() => return None,
            _ => {}
        }
        let line = self.line()?;
        let src = self.selected().filter(|s| usable(s))?;
        Some(format!("{:?}|{}|{}", line, self.gift, src["id"].as_str().unwrap_or_default()))
    }

    fn ready(&self) -> Option<(&Invoice, Option<&Invoice>)> {
        let key = self.want()?;
        if self.prev.key != key {
            return None;
        }
        let main = self.prev.main.as_ref()?.as_ref().ok()?;
        match (&self.what, &self.prev.renewal) {
            (What::Nitro(_) | What::Boost(_), Some(Ok(r))) => Some((main, Some(r))),
            (What::Nitro(_) | What::Boost(_), _) => None,
            _ => Some((main, None)),
        }
    }

    fn eea(&self) -> bool {
        !self.pay_orbs && self.selected().is_some_and(|s| s["country"].as_str().or(s["billing_address"]["country"].as_str()).is_some_and(|c| EEA.contains(&c)))
    }

    /// Everything the Purchase button waits for, as the reason it can't be pressed, or nothing.
    pub fn blocker(&self) -> Option<String> {
        if self.sent {
            return Some("Processing...".into());
        }
        if self.gift && self.to.is_none() {
            return Some("Select a gift recipient to continue".into());
        }
        if self.pay_orbs {
            return match (self.orbs, self.orbs_price()) {
                (Some(b), Some(p)) if b >= p => None,
                (Some(_), _) => Some("Not enough Orbs.".into()),
                _ => Some("Loading your Orbs balance".into()),
            };
        }
        if !self.money_priced() {
            return Some("This item is not sold for money.".into());
        }
        if self.selected().is_none_or(|s| !usable(s)) {
            return Some("Select a payment source to continue".into());
        }
        if self.ready().is_none() {
            return Some(match (&self.prev.main, self.want()) {
                (Some(Err(e)), Some(k)) if self.prev.key == k => e.clone(),
                _ => "Loading the total".into(),
            });
        }
        if self.eea() && !self.waiver {
            return Some("Accept the Terms of Service and Paid Services Terms to continue".into());
        }
        None
    }

    fn done_text(&self, body: &Value) -> String {
        let name = match &self.what {
            What::Shop(p) => p["name"].as_str().unwrap_or("Your item").to_string(),
            What::Listing(l) => l["name"].as_str().unwrap_or("Your item").to_string(),
            What::Nitro(p) => crate::ui::settings::billing::plan_name(p).to_string(),
            What::NitroGift => self.plan.as_deref().map_or("Nitro", crate::ui::settings::billing::plan_name).to_string(),
            What::Boost(_) => format!("{} Server Boost{}", self.qty, if self.qty == 1 { "" } else { "s" }),
        };
        if self.gift {
            let who = self.to.map_or(String::new(), |_| self.note.clone());
            return format!("{name} has been sent to {who}.");
        }
        match &self.what {
            What::Nitro(_) => "You can start using your premium features now!".to_string(),
            What::Boost(_) => {
                if body["unapplied"] == true {
                    format!("{name} bought. They couldn't be applied to the server yet: apply them from Settings > Server Boost.")
                } else {
                    format!("{name} applied to the server.")
                }
            }
            _ => format!("{name} has been purchased."),
        }
    }
}

fn friends(app: &App) -> Vec<(Id, String)> {
    let mut v: Vec<(Id, String)> = app.store.relationships.iter().filter(|(_, k)| **k == FRIEND).map(|(id, _)| (*id, app.store.name_in(0, *id))).collect();
    v.sort_by_key(|a| a.1.to_lowercase());
    v
}

fn send_preview(app: &mut App) {
    let Some(b) = app.buy.as_mut() else { return };
    let Some(key) = b.want() else { return };
    if b.prev.key == key || b.prev.asked != 0 {
        return;
    }
    let (Some(line), Some(src)) = (b.line(), b.source.clone()) else { return };
    b.seq += 1;
    let seq = b.seq;
    b.prev = Preview { key, seq, main: None, renewal: None, asked: 1 };
    let c = req::preview(&line, b.gift, Some(&src), &b.currency(), &b.load, false);
    app.rest(c.method, c.path, c.body, Tag::Buy("prev", seq));
}

fn ask_renewal(app: &mut App) {
    let Some(b) = app.buy.as_mut() else { return };
    let (Some(line), Some(src)) = (b.line(), b.source.clone()) else { return };
    b.prev.asked = 2;
    let seq = b.prev.seq;
    let c = req::preview(&line, b.gift, Some(&src), &b.currency(), &b.load, true);
    app.rest(c.method, c.path, c.body, Tag::Buy("renew", seq));
}

fn pick_source(b: &mut Buy) {
    let Some(list) = &b.sources else { return };
    if b.source.as_ref().is_some_and(|id| list.iter().any(|s| s["id"] == *id && usable(s))) {
        return;
    }
    b.source = list.iter().find(|s| s["default"] == true && usable(s)).or_else(|| list.iter().find(|s| usable(s))).and_then(|s| s["id"].as_str().map(str::to_string));
}

pub fn reply(app: &mut App, name: &'static str, gid: Id, res: Result<Value, String>) {
    if name == "listings" {
        return server_shop::loaded(app, gid, res);
    }
    let Some(b) = app.buy.as_mut() else { return };
    let list = |r: &Result<Value, String>| r.as_ref().ok().and_then(|v| v.as_array().cloned());
    match name {
        "sources" => {
            b.sources = Some(list(&res).unwrap_or_default());
            pick_source(b);
        }
        "orbs" => b.orbs = res.ok().and_then(|v| v["balance"].as_u64()),
        "subs" => b.subs = Some(list(&res).unwrap_or_default()),
        "slots" => b.slots = Some(list(&res).unwrap_or_default()),
        "plans0" => b.plans[0] = Some(list(&res).unwrap_or_default()),
        "plans1" => b.plans[1] = Some(list(&res).unwrap_or_default()),
        "prev" | "renew" if gid != b.prev.seq => {}
        "prev" => {
            let r = res.and_then(|v| req::invoice(&v).ok_or_else(|| "Discord sent no total for this purchase.".to_string()));
            let renew = matches!(b.what, What::Nitro(_) | What::Boost(_)) && r.is_ok();
            b.prev.main = Some(r);
            b.prev.asked = 0;
            if renew {
                ask_renewal(app);
            }
        }
        "renew" => {
            b.prev.renewal = Some(res.and_then(|v| req::invoice(&v).ok_or_else(|| "Discord sent no renewal price.".to_string())));
            b.prev.asked = 0;
        }
        "charge" => return settled(app, res),
        "slots2" => return apply_boosts(app, res),
        "apply" => return finish(app, json!({ "unapplied": res.is_err() })),
        "pay" => return payment_polled(app, res),
        _ => {}
    }
}

/// The one place a purchase leaves: from the click on the final button, once. A dialog that has
/// sent a charge never sends another.
fn purchase(app: &mut App) {
    let Some(b) = app.buy.as_mut() else { return };
    if b.step != Step::Review || b.sent || b.blocker().is_some() {
        return;
    }
    if let Some(why) = crate::net::charge_barred() {
        b.step = Step::Failed { msg: why.into(), unsure: false };
        return;
    }
    let call = if b.pay_orbs {
        let Some(Line::Sku { sku, .. }) = b.line() else { return };
        req::redeem(&sku, &b.load)
    } else {
        let (Some(line), Some((inv, renewal)), Some(src)) = (b.line(), b.ready(), b.selected()) else { return };
        let token = None::<String>;
        let gift = b.to.filter(|_| b.gift).map(|to| req::Gift { style: b.style, to: to.to_string(), message: b.message.clone() });
        req::charge(&req::Pay {
            line: &line,
            gift: gift.as_ref(),
            source: src["id"].as_str().unwrap_or_default(),
            token,
            load: &b.load,
            purchase_token: &purchase_token(),
            expected: inv,
            renewal,
        })
    };
    b.sent = true;
    b.step = Step::Sending;
    b.note = b.to.map(|t| app.store.name_in(0, t)).unwrap_or_default();
    let Some(body) = call.body else { return };
    app.net.send(Cmd::Charge { method: call.method, path: call.path, body, tag: Tag::Buy("charge", 0) });
}

fn settled(app: &mut App, res: Result<Value, String>) {
    let Some(b) = app.buy.as_mut() else { return };
    if b.step != Step::Sending {
        return;
    }
    let orbs = b.pay_orbs;
    match req::outcome(res) {
        Outcome::Done(body) if orbs && !body.is_array() => b.step = Step::Failed { msg: "Discord didn't confirm the Orbs purchase.".into(), unsure: true },
        Outcome::Done(body) => {
            if matches!(b.what, What::Boost(_)) {
                app.rest("GET", "/users/@me/guilds/premium/subscription-slots".into(), None, Tag::Buy("slots2", 0));
            } else {
                finish(app, body);
            }
        }
        Outcome::Refused(msg) => {
            b.step = Step::Failed { msg, unsure: false };
            b.sent = false;
            b.prev = Preview::default();
        }
        Outcome::Confirm { payment, url } => {
            let opened = url.as_deref().is_some_and(crate::webwin::pay_page);
            if opened {
                b.step = Step::Confirming { payment };
                b.polled = Some(Instant::now());
            } else {
                b.step = Step::Failed { msg: "Your bank or payment provider needs to confirm this payment, which can't be done in Rustcord.".into(), unsure: true };
            }
        }
        Outcome::Unknown(msg) => b.step = Step::Failed { msg, unsure: true },
    }
}

fn apply_boosts(app: &mut App, res: Result<Value, String>) {
    let Some(b) = app.buy.as_mut() else { return };
    let What::Boost(gid) = b.what else { return };
    let ids: Vec<Value> = res.ok().and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter(|s| is_free(s)).filter_map(|s| s["id"].as_str().map(|i| json!(i))).collect();
    if ids.is_empty() {
        return finish(app, json!({ "unapplied": true }));
    }
    app.rest("PUT", format!("/guilds/{gid}/premium/subscriptions"), Some(json!({ "user_premium_guild_subscription_slot_ids": ids })), Tag::Buy("apply", 0));
}

fn finish(app: &mut App, body: Value) {
    let Some(b) = app.buy.as_mut() else { return };
    let text = b.done_text(&body);
    b.step = Step::Done(text);
    for k in KEYS {
        refetch(app, k);
    }
    for k in super::settings::billing::KEYS {
        refetch(app, k);
    }
}

fn payment_polled(app: &mut App, res: Result<Value, String>) {
    let Some(b) = app.buy.as_mut() else { return };
    b.polling = false;
    let Ok(v) = res else { return };
    if !matches!(b.step, Step::Confirming { .. }) {
        return;
    }
    match v["status"].as_u64() {
        Some(1) => finish(app, Value::Null),
        Some(2..=5) => b.step = Step::Failed { msg: "The payment didn't go through.".into(), unsure: true },
        _ => {}
    }
}

fn tick(app: &mut App, ctx: &egui::Context) {
    let Some(b) = app.buy.as_mut() else { return };
    if b.card_window && !crate::webwin::is_open() {
        b.card_window = false;
        b.sources = None;
        app.rest("GET", "/users/@me/billing/payment-sources".into(), None, Tag::Buy("sources", 0));
        return;
    }
    if let Step::Confirming { payment } = &b.step
        && !b.polling
        && b.polled.is_none_or(|t| t.elapsed() >= POLL)
    {
        let path = format!("/users/@me/billing/payments/{payment}");
        b.polling = true;
        b.polled = Some(Instant::now());
        app.rest("GET", path, None, Tag::Buy("pay", 0));
    }
    if matches!(app.buy.as_ref().map(|b| &b.step), Some(Step::Confirming { .. })) {
        ctx.request_repaint_after(POLL);
    }
    if matches!(app.buy.as_ref().map(|b| &b.step), Some(Step::Review)) {
        send_preview(app);
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if app.buy.is_none() {
        return;
    }
    tick(app, ctx);
    let friends = if app.buy.as_ref().is_some_and(|b| b.gift) { friends(app) } else { Vec::new() };
    view::show(app, ctx, &friends);
}
