//! A linked role's requirements on the accounts a person connects: for the platforms that report
//! more than that the account exists, the fields (age, followers, verified...) a requirement can
//! ask for, and how each is saved in the role's connection configuration.

use egui::{Rect, RichText, Ui, pos2};
use serde_json::{Value, json};

use crate::theme::*;
use crate::ui::settings;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    At(&'static str, &'static str),
    AtMost(&'static str, &'static str),
    Flag,
}

struct Field {
    platform: &'static str,
    key: &'static str,
    title: &'static str,
    lead: &'static str,
    kind: Kind,
}

const AGE: Kind = Kind::At("day", "days");

const FIELDS: &[Field] = &[
    Field { platform: "bluesky", key: "created_at", title: "Account Age", lead: "Account must be at least ", kind: AGE },
    Field { platform: "bluesky", key: "followers_count", title: "Followers", lead: "Must have at least ", kind: Kind::At("follower", "followers") },
    Field { platform: "bluesky", key: "statuses_count", title: "Posts", lead: "Must have at least ", kind: Kind::At("post", "posts") },
    Field { platform: "steam", key: "created_at", title: "Account Age", lead: "Account must be at least ", kind: AGE },
    Field { platform: "steam", key: "game_count", title: "Games", lead: "Must have at least ", kind: Kind::At("game", "games") },
    Field { platform: "twitter", key: "created_at", title: "Account Age", lead: "Account must be at least ", kind: AGE },
    Field { platform: "twitter", key: "followers_count", title: "Followers", lead: "Must have at least ", kind: Kind::At("follower", "followers") },
    Field { platform: "twitter", key: "statuses_count", title: "Posts", lead: "Must have at least ", kind: Kind::At("post", "posts") },
    Field { platform: "twitter", key: "verified", title: "Verified", lead: "Must be verified", kind: Kind::Flag },
    Field { platform: "reddit", key: "created_at", title: "Account Age", lead: "Account must be at least ", kind: AGE },
    Field { platform: "reddit", key: "total_karma", title: "Karma", lead: "Must have at least ", kind: Kind::At("Karma", "Karma") },
    Field { platform: "reddit", key: "mod", title: "Mod", lead: "Must be a verified moderator", kind: Kind::Flag },
    Field { platform: "reddit", key: "gold", title: "Gold", lead: "Must have Reddit Gold", kind: Kind::Flag },
    Field { platform: "paypal", key: "created_at", title: "Account Age", lead: "Account must be at least ", kind: AGE },
    Field { platform: "paypal", key: "verified", title: "Verified Bank Account", lead: "Must have verified bank account", kind: Kind::Flag },
    Field { platform: "ebay", key: "created_at", title: "Account Age", lead: "Account must be at least ", kind: AGE },
    Field { platform: "ebay", key: "positive_feedback_percentage", title: "Percentage of Positive Ratings", lead: "Must have at least ", kind: Kind::At("percent positive ratings", "percent positive ratings") },
    Field { platform: "ebay", key: "unique_positive_feedback_count", title: "Positive Feedback Reviews", lead: "Must have at least ", kind: Kind::At("positive feedback review(s)", "positive feedback review(s)") },
    Field { platform: "ebay", key: "unique_negative_feedback_count", title: "Negative Feedback Reviews", lead: "Must have at most ", kind: Kind::AtMost("negative feedback review(s)", "negative feedback review(s)") },
    Field { platform: "ebay", key: "top_rated_seller", title: "Top-Rated Seller", lead: "Must be Top-Rated Seller", kind: Kind::Flag },
    Field { platform: "tiktok", key: "verified", title: "Verified", lead: "Must be verified", kind: Kind::Flag },
    Field { platform: "tiktok", key: "follower_count", title: "Followers", lead: "Must have at least ", kind: Kind::At("follower", "followers") },
    Field { platform: "tiktok", key: "following_count", title: "Following", lead: "Must follow at least ", kind: Kind::At("user", "users") },
    Field { platform: "tiktok", key: "likes_count", title: "Likes", lead: "Must have at least ", kind: Kind::At("like", "likes") },
];

const EQUAL: u8 = 1;
const LESS_THAN: u8 = 3;
const GREATER_THAN: u8 = 4;

/// One condition on a connected account: the field, how it's compared and what to, as saved.
#[derive(Clone, PartialEq)]
pub struct Cond {
    pub platform: String,
    pub field: String,
    pub operator: u8,
    pub value: String,
}

pub fn has_fields(platform: &str) -> bool {
    FIELDS.iter().any(|f| f.platform == platform)
}

/// The conditions in a configuration answer: every element that names a metadata field.
pub fn parse(v: &Value) -> Vec<Cond> {
    v.as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g.as_array().into_iter().flatten().chain(std::iter::once(g)))
        .filter_map(|c| {
            Some(Cond {
                platform: c["connection_type"].as_str()?.to_string(),
                field: c["connection_metadata_field"].as_str()?.to_string(),
                operator: c["operator"].as_u64()? as u8,
                value: c["value"].as_str().map(str::to_string).or_else(|| c["value"].as_u64().map(|n| n.to_string()))?,
            })
        })
        .collect()
}

/// A requirement's elements: the connection itself, then each condition on it.
pub fn elements(platform: &str, conds: &[Cond]) -> Vec<Value> {
    let mut out = vec![json!({ "connection_type": platform })];
    out.extend(conds.iter().filter(|c| c.platform == platform).map(|c| json!({ "connection_type": platform, "connection_metadata_field": c.field, "operator": c.operator, "value": c.value })));
    out
}

fn shown(c: &Cond) -> i64 {
    let n: i64 = c.value.parse().unwrap_or(0);
    match c.operator {
        GREATER_THAN => n + 1,
        LESS_THAN => (n - 1).max(0),
        _ => n,
    }
}

fn stored(operator: u8, n: i64) -> String {
    match operator {
        GREATER_THAN => (n - 1).max(0),
        LESS_THAN => (n + 1).max(1),
        _ => n,
    }
    .to_string()
}

fn phrase(f: &Field, n: i64) -> String {
    match f.kind {
        Kind::Flag => f.lead.to_string(),
        Kind::At(one, many) | Kind::AtMost(one, many) => format!("{}{n} {}", f.lead, if n == 1 { one } else { many }),
    }
}

/// The fields of one platform as rows with a switch each; false-locked rows can't change.
pub fn editor(ui: &mut Ui, platform: &str, conds: &mut Vec<Cond>, locked: bool) -> bool {
    let mut changed = false;
    for f in FIELDS.iter().filter(|f| f.platform == platform) {
        let at = conds.iter().position(|c| c.platform == platform && c.field == f.key);
        let mut on = at.is_some();
        let operator = match f.kind {
            Kind::Flag => EQUAL,
            Kind::At(..) => GREATER_THAN,
            Kind::AtMost(..) => LESS_THAN,
        };
        let n = at.map_or(1, |i| shown(&conds[i]));
        let top = ui.cursor().top();
        settings::toggle(ui, &mut on, f.title, &phrase(f, n));
        settings::mark(ui, &format!("linked:{platform}:{}", f.key), Rect::from_min_max(pos2(ui.min_rect().left(), top), pos2(ui.max_rect().right(), ui.cursor().top())));
        if on != at.is_some() && !locked {
            match at {
                Some(i) => {
                    conds.remove(i);
                }
                None => conds.push(Cond { platform: platform.into(), field: f.key.into(), operator, value: if f.kind == Kind::Flag { "1".into() } else { stored(operator, 1) } }),
            }
            changed = true;
        }
        if let (Some(i), false) = (at, f.kind == Kind::Flag) {
            let mut text = shown(&conds[i]).to_string();
            ui.horizontal(|ui| {
                ui.label(RichText::new(if matches!(f.kind, Kind::AtMost(..)) { "At most" } else { "At least" }).font(medium(14.0)).color(pal().header_secondary));
                settings::input(ui, &mut text, "0", false);
            });
            if let Ok(v) = text.trim().parse::<i64>()
                && v >= 0
                && v != shown(&conds[i])
                && !locked
            {
                conds[i].value = stored(conds[i].operator, v);
                changed = true;
            }
        }
    }
    changed
}
