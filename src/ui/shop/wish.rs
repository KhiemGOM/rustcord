//! Wishlists the way Discord's client keeps them: a user's wishlist id is the key of their
//! profile's `wishlist_settings`, its items come from `/wishlists/{id}`, and the hearts add and
//! remove through the signed-in user's own routes, which answer with the whole list.

use serde_json::{Value, json};

use crate::net::Tag;
use crate::state::Id;
use crate::ui::App;

pub fn skus(v: &Value) -> Vec<String> {
    let items = v["wishlist_items"].as_array().or(v["items"].as_array());
    items.into_iter().flatten().filter_map(|i| i["sku"]["id"].as_str().or(i["sku_id"].as_str()).map(str::to_string)).collect()
}

/// The id of `uid`'s wishlist: None while their profile is still on its way, Some(None) when
/// they have no wishlist.
pub fn id_of(app: &mut App, uid: Id) -> Option<Option<String>> {
    let from = |p: &Value| p["wishlist_settings"].as_object().and_then(|m| m.keys().next().cloned());
    if let Some(p) = app.profiles.get(&uid).filter(|p| !p.is_null()) {
        return Some(from(p));
    }
    let key = format!("profile_wish:{uid}");
    if !app.keyed.contains_key(&key) {
        app.keyed.insert(key.clone(), None);
        app.rest("GET", format!("/users/{uid}/profile?with_mutual_guilds=false&with_mutual_friends=false&with_mutual_friends_count=false"), None, Tag::Keyed(key.clone()));
    }
    match app.keyed.get(&key)? {
        Some(Ok(p)) => Some(from(p)),
        Some(Err(_)) => Some(None),
        None => None,
    }
}

pub fn reload(app: &mut App) {
    app.keyed.retain(|k, _| !k.starts_with("shop_wish") && !k.starts_with("profile_wish:"));
}

fn take(app: &mut App, key: &str) -> Option<Result<Value, String>> {
    if matches!(app.keyed.get(key), Some(Some(_))) { app.keyed.remove(key).flatten() } else { None }
}

pub fn sync(app: &mut App) {
    let Some(state) = app.shop.as_ref().map(|s| s.wish_load) else { return };
    let me = app.store.me;
    if state == 0 {
        match id_of(app, me) {
            None => {}
            Some(None) => set_load(app, 2),
            Some(Some(id)) => {
                app.keyed.insert("shop_wishlist".into(), None);
                app.rest("GET", format!("/wishlists/{id}?source=user_profile"), None, Tag::Keyed("shop_wishlist".into()));
                if let Some(s) = &mut app.shop {
                    s.wish_id = Some(id);
                    s.wish_load = 1;
                }
            }
        }
    }
    if state == 1
        && let Some(res) = take(app, "shop_wishlist")
    {
        if let (Ok(v), Some(s)) = (&res, &mut app.shop) {
            s.wish = skus(v);
        }
        set_load(app, 2);
    }
    if let Some(res) = take(app, "shop_wish") {
        match res {
            Ok(v) => {
                if let Some(s) = &mut app.shop {
                    s.wish_id = v["id"].as_str().map(str::to_string).or(s.wish_id.take());
                    s.wish = skus(&v);
                }
            }
            Err(e) => {
                app.toast(e);
                if let Some(s) = &mut app.shop {
                    s.wish_load = 0;
                }
                reload(app);
            }
        }
    }
}

fn set_load(app: &mut App, n: u8) {
    if let Some(s) = &mut app.shop {
        s.wish_load = n;
    }
}

pub fn toggle(app: &mut App, sku: String) {
    let Some(s) = &mut app.shop else { return };
    if let Some(i) = s.wish.iter().position(|w| *w == sku) {
        s.wish.remove(i);
        if let Some(list) = s.wish_id.clone() {
            app.keyed.insert("shop_wish".into(), None);
            app.rest("DELETE", format!("/users/@me/wishlists/{list}/items/{sku}"), None, Tag::Keyed("shop_wish".into()));
        }
    } else {
        s.wish.push(sku.clone());
        app.keyed.insert("shop_wish".into(), None);
        app.rest("POST", "/users/@me/wishlist/items".into(), Some(json!({ "sku_id": sku })), Tag::Keyed("shop_wish".into()));
    }
}

/// The heart in the Shop's header: the profile editor, on its Wishlist tab, as in Discord.
pub fn open(app: &mut App) {
    crate::ui::profed::open(app, None);
    if let Some(e) = &mut app.profed {
        e.tab = crate::ui::profed::Tab::Wishlist;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_come_from_either_shape() {
        assert_eq!(skus(&json!({ "wishlist_items": [{ "sku": { "id": "5" } }, { "sku_id": "6" }] })), ["5", "6"]);
        assert_eq!(skus(&json!({ "items": [{ "sku_id": "7" }] })), ["7"]);
        assert!(skus(&json!({})).is_empty());
    }
}
