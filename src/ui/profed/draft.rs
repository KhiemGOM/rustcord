//! What the profile editor edits: the draft, how it reads a profile response, and the bodies it
//! writes back.

use serde_json::{Value, json};

use crate::state::{Id, User, sid};
use crate::ui::cosmo::{Item, Kind};

#[derive(Clone, PartialEq, Default, Debug)]
pub enum Pic {
    #[default]
    Keep,
    Gone,
    New(String),
    Recent(String, String),
}

#[derive(Clone, PartialEq, Default, Debug)]
pub struct Style {
    pub font: u8,
    pub effect: u8,
    pub colors: Vec<u32>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub comment: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    Favorite,
    Played,
    Current,
    Want,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::Favorite, Slot::Played, Slot::Current, Slot::Want];

    pub fn wire(self) -> &'static str {
        match self {
            Slot::Favorite => "favorite_games",
            Slot::Played => "played_games",
            Slot::Current => "current_games",
            Slot::Want => "want_to_play_games",
        }
    }

    pub fn of(wire: &str) -> Option<Slot> {
        Slot::ALL.into_iter().find(|s| s.wire() == wire)
    }

    pub fn title(self) -> &'static str {
        match self {
            Slot::Favorite => "Favorite Game",
            Slot::Played => "Games I Like",
            Slot::Current => "Games in Rotation",
            Slot::Want => "Want to Play",
        }
    }

    pub fn max(self) -> usize {
        match self {
            Slot::Favorite => 1,
            Slot::Current => 5,
            Slot::Played | Slot::Want => 20,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Widget {
    pub id: Option<String>,
    pub slot: Slot,
    pub games: Vec<Game>,
}

#[derive(Clone, PartialEq, Default, Debug)]
pub struct Draft {
    pub name: String,
    pub pronouns: String,
    pub bio: String,
    pub accent: Option<u32>,
    pub theme: [Option<u32>; 2],
    pub avatar: Pic,
    pub banner: Pic,
    pub decoration: Option<Item>,
    pub nameplate: Option<Item>,
    pub effect: Option<Item>,
    pub frame: Option<Item>,
    pub style: Option<Style>,
    pub tag: Option<Id>,
    pub widgets: Vec<Widget>,
    pub avatar_url: String,
    pub banner_url: Option<String>,
    /// For a server's profile: what the account has, which the preview shows where this has none.
    pub inherit_theme: [Option<u32>; 2],
    pub inherit_banner: Option<String>,
}

fn worn(v: &Value, asset_key: &str) -> Option<Item> {
    let d = &v[asset_key];
    d.is_object().then(|| Item {
        sku: d["sku_id"].as_str().or(d["id"].as_str()).unwrap_or("").to_string(),
        kind: Kind::Decoration,
        name: String::new(),
        label: d["label"].as_str().unwrap_or("").to_string(),
        asset: d["asset"].as_str().unwrap_or("").to_string(),
        thumb: String::new(),
        layers: Vec::new(),
        nitro: false,
    })
}

fn sku_only(v: &Value, kind: Kind) -> Option<Item> {
    let sku = v["id"].as_str().or(v["sku_id"].as_str())?;
    Some(Item { sku: sku.to_string(), kind, name: String::new(), label: String::new(), asset: String::new(), thumb: String::new(), layers: Vec::new(), nitro: false })
}

pub fn widgets_of(v: &Value) -> Vec<Widget> {
    v["widgets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| {
            let d = &w["data"];
            let slot = Slot::of(d["type"].as_str()?)?;
            let games = d["games"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|g| Game {
                    id: g["game_id"].as_str().unwrap_or("").to_string(),
                    name: g["name"].as_str().unwrap_or("").to_string(),
                    comment: g["comment"].as_str().map(str::to_string),
                    tags: g["tags"].as_array().into_iter().flatten().filter_map(|t| t.as_str().map(str::to_string)).collect(),
                })
                .collect();
            Some(Widget { id: w["id"].as_str().map(str::to_string), slot, games })
        })
        .collect()
}

pub fn from_profile(v: &Value, me: &User, guild: Option<Id>) -> Draft {
    let (p, m) = if guild.is_some() { (&v["guild_member_profile"], &v["guild_member"]) } else { (&v["user_profile"], &v["user"]) };
    let theme = |i: usize| p["theme_colors"][i].as_u64().map(|c| c as u32);
    let s = &v["user"]["display_name_styles"];
    let colors: Vec<u32> = s["colors"].as_array().into_iter().flatten().filter_map(|c| c.as_u64()).map(|c| c as u32).collect();
    let style = s.is_object().then(|| Style { font: s["font_id"].as_u64().unwrap_or(11) as u8, effect: s["effect_id"].as_u64().unwrap_or(1) as u8, colors });
    let uid = me.id;
    let banner = p["banner"].as_str().map(|b| match guild {
        Some(g) => format!("{}/guilds/{g}/users/{uid}/banners/{b}.png?size=600", crate::api::cdn()),
        None => format!("{}/banners/{uid}/{b}.png?size=600", crate::api::cdn()),
    });
    let avatar_url = match (guild, m["avatar"].as_str()) {
        (Some(g), Some(a)) => format!("{}/guilds/{g}/users/{uid}/avatars/{a}.png?size=256", crate::api::cdn()),
        _ => me.avatar_url().replace("size=80", "size=256"),
    };
    let (effect, frame) = crate::ui::cosmo::worn(v);
    let account = &v["user_profile"];
    let inherit = |i: usize| guild.and(account["theme_colors"][i].as_u64()).map(|c| c as u32);
    let inherit_banner = guild.and(account["banner"].as_str().or(v["user"]["banner"].as_str())).map(|b| format!("{}/banners/{uid}/{b}.png?size=600", crate::api::cdn()));
    Draft {
        name: if guild.is_some() { m["nick"].as_str().unwrap_or("").to_string() } else { me.global_name.clone().unwrap_or_default() },
        pronouns: p["pronouns"].as_str().unwrap_or("").to_string(),
        bio: p["bio"].as_str().or(v["user"]["bio"].as_str()).unwrap_or("").to_string(),
        accent: p["accent_color"].as_u64().or(v["user"]["accent_color"].as_u64()).map(|c| c as u32),
        theme: [theme(0), theme(1)],
        avatar: Pic::Keep,
        banner: Pic::Keep,
        decoration: worn(if guild.is_some() && m["avatar_decoration_data"].is_object() { m } else { &v["user"] }, "avatar_decoration_data"),
        nameplate: worn(&v["user"]["collectibles"], "nameplate").map(|mut n| {
            n.kind = Kind::Nameplate;
            n
        }),
        effect: sku_only(&json!({ "id": effect }), Kind::Effect).filter(|e| !e.sku.is_empty()),
        frame: sku_only(&json!({ "id": frame }), Kind::Frame).filter(|e| !e.sku.is_empty()),
        style,
        tag: v["user"]["primary_guild"]["identity_enabled"].as_bool().filter(|e| *e).map(|_| sid(&v["user"]["primary_guild"]["identity_guild_id"])),
        widgets: widgets_of(v),
        avatar_url,
        banner_url: banner,
        inherit_theme: [inherit(0), inherit(1)],
        inherit_banner,
    }
}

fn sku(i: &Option<Item>) -> Value {
    i.as_ref().map_or(Value::Null, |i| json!(i.sku))
}

fn pic(p: &Pic) -> Option<Value> {
    match p {
        Pic::Keep => None,
        Pic::Gone => Some(Value::Null),
        Pic::New(u) => Some(json!(u)),
        Pic::Recent(..) => None,
    }
}

pub struct Writes {
    pub profile: Value,
    pub account: Value,
}

pub fn writes(d: &Draft, base: &Draft, guild: Option<Id>) -> Writes {
    let mut prof = json!({ "bio": d.bio, "pronouns": d.pronouns, "accent_color": d.accent });
    if let Some(a) = d.theme[0].or(d.theme[1]) {
        prof["theme_colors"] = json!([d.theme[0].unwrap_or(a), d.theme[1].unwrap_or(a)]);
    }
    if let Some(b) = pic(&d.banner) {
        prof["banner"] = b;
    }
    if d.effect != base.effect {
        prof["profile_effect_id"] = sku(&d.effect);
    }
    if d.frame != base.frame {
        prof["profile_frame_id"] = sku(&d.frame);
    }
    let mut acct = json!({});
    if let Some(a) = pic(&d.avatar) {
        acct["avatar"] = a;
    }
    if let Pic::Recent(id, _) = &d.avatar {
        acct["avatar_id"] = json!(id);
    }
    if d.decoration != base.decoration {
        acct["avatar_decoration_sku_id"] = sku(&d.decoration);
    }
    let name = if d.name.trim().is_empty() { Value::Null } else { json!(d.name.trim()) };
    match guild {
        Some(_) => {
            acct["nick"] = name;
            if d.nameplate != base.nameplate {
                acct["collectibles"] = json!({ "nameplate": d.nameplate.as_ref().map(|n| json!({ "sku_id": n.sku })) });
            }
        }
        None => {
            acct["global_name"] = name;
            if d.nameplate != base.nameplate {
                acct["nameplate_sku_id"] = sku(&d.nameplate);
            }
            if d.tag != base.tag {
                acct["primary_guild_id"] = d.tag.map_or(Value::Null, |g| json!(g.to_string()));
            }
        }
    }
    if d.style != base.style {
        match &d.style {
            Some(s) => {
                acct["display_name_font_id"] = json!(s.font);
                acct["display_name_effect_id"] = json!(s.effect);
                acct["display_name_colors"] = json!(s.colors);
            }
            None => {
                acct["display_name_font_id"] = Value::Null;
                acct["display_name_effect_id"] = Value::Null;
                acct["display_name_colors"] = Value::Null;
            }
        }
    }
    Writes { profile: prof, account: acct }
}

pub fn widgets_body(list: &[Widget]) -> Value {
    let w: Vec<Value> = list
        .iter()
        .filter(|w| !w.games.is_empty())
        .map(|w| {
            let games: Vec<Value> = w.games.iter().map(|g| json!({ "game_id": g.id, "comment": g.comment, "tags": g.tags })).collect();
            let mut o = json!({ "data": { "type": w.slot.wire(), "games": games } });
            if let Some(id) = &w.id {
                o["id"] = json!(id);
            }
            o
        })
        .collect();
    json!({ "widgets": w })
}
