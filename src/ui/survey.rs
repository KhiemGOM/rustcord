//! Discord's survey notice: the strip right of the rail asking how something is working, with a
//! button to the survey. Fetched on connect at most once a day; closing or answering one hides it
//! for good, and its seen ping goes out at most once a day.

use egui::{Color32, Ui};
use serde_json::Value;

use super::App;
use super::notice::Hit;
use crate::net::Tag;
use crate::state::{ADMINISTRATOR, Id, Store};
use crate::theme::*;

const DAY: i64 = 86_400_000;
const KNOWN: [&str; 8] = ["is_owner", "is_admin", "is_community", "guild_size", "is_hub", "is_viewing", "guild_permissions", "guild_size_all"];

fn current(app: &mut App, now: i64) -> Option<Value> {
    if !app.remote.contains_key("survey") {
        if now - app.ui.survey_fetched < DAY {
            return None;
        }
        app.ui.survey_fetched = now;
        app.save_at = Some(std::time::Instant::now());
    }
    let Some(Ok(v)) = crate::ui::settings::remote(app, "survey", "/users/@me/survey?disable_auto_seen=true") else { return None };
    let s = v["survey"].clone();
    let key = s["key"].as_str()?;
    (!app.ui.surveys_hidden.iter().any(|k| k == key) && fits(&app.store, &s, app.ui.guild)).then_some(s)
}

/// Discord's guild requirements: with none every account sees the survey; otherwise some server
/// must pass all of them, or with guild_size_all every server must be the right size.
fn fits(store: &Store, s: &Value, viewing: Option<Id>) -> bool {
    let reqs: Vec<&str> = s["guild_requirements"].as_array().into_iter().flatten().filter_map(|r| r.as_str()).collect();
    if reqs.is_empty() {
        return true;
    }
    if reqs.iter().any(|r| !KNOWN.contains(r)) {
        return false;
    }
    let has = |r: &str| reqs.contains(&r);
    let all = has("guild_size_all");
    let (lo, hi) = (s["guild_size"][0].as_u64(), s["guild_size"][1].as_u64());
    let wanted: Vec<u64> = s["guild_permissions"].as_array().into_iter().flatten().filter_map(|p| p.as_str()?.parse().ok()).collect();
    let mut sized = true;
    for (&id, g) in &store.guilds {
        if all || has("guild_size") {
            let n = g.member_count as u64;
            if lo.is_some_and(|lo| n < lo) || hi.is_some_and(|hi| n > hi) {
                sized = false;
                continue;
            }
        }
        let feature = |f: &str| g.features.iter().any(|x| x == f);
        if has("is_community") && !feature("COMMUNITY") || has("is_hub") && !feature("HUB") {
            continue;
        }
        let perms = store.my_perms(id);
        if has("guild_permissions") && !wanted.iter().any(|p| perms & p == *p) {
            continue;
        }
        if has("is_owner") && g.owner != store.me || has("is_admin") && perms & ADMINISTRATOR == 0 {
            continue;
        }
        if (!has("is_viewing") || viewing == Some(id)) && !all {
            return true;
        }
    }
    all && sized
}

/// Draws the notice when there's a survey for this account, and says whether it did.
pub fn bar(app: &mut App, ui: &mut Ui) -> bool {
    let now = chrono::Utc::now().timestamp_millis();
    let Some(s) = current(app, now) else { return false };
    let (Some(key), Some(prompt), Some(cta)) = (s["key"].as_str(), s["prompt"].as_str(), s["cta"].as_str()) else { return false };
    if let Some(id) = s["id"].as_str()
        && now - app.ui.survey_seen >= DAY
    {
        app.ui.survey_seen = now;
        app.save_at = Some(std::time::Instant::now());
        app.rest("POST", format!("/users/@me/survey/{id}/seen"), None, Tag::Quiet);
    }
    let hit = super::notice::strip(ui, pal().blurple, prompt, cta, Color32::from_rgb(0x22, 0x22, 0x22));
    let go = matches!(hit, Some(Hit::Button));
    if go && let Some(url) = s["url"].as_str().filter(|u| u.starts_with("https://")) {
        let _ = open::that_detached(url);
    }
    if hit.is_some() {
        app.ui.surveys_hidden.push(key.to_string());
        app.save_at = Some(std::time::Instant::now());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Guild;
    use serde_json::json;

    fn store(guilds: &[(Id, u32, &[&str], Id)]) -> Store {
        let mut s = Store { me: 1, ..Default::default() };
        for &(id, members, features, owner) in guilds {
            s.guilds.insert(id, Guild { id, member_count: members, features: features.iter().map(|f| f.to_string()).collect(), owner, ..Default::default() });
        }
        s
    }

    #[test]
    fn a_survey_shows_only_where_its_guild_requirements_hold() {
        let s = store(&[(10, 50, &[], 2), (11, 5000, &["COMMUNITY"], 1)]);
        assert!(fits(&s, &json!({}), None));
        assert!(!fits(&s, &json!({ "guild_requirements": ["is_martian"] }), None));
        assert!(fits(&s, &json!({ "guild_requirements": ["is_owner", "is_community"] }), None));
        assert!(!fits(&s, &json!({ "guild_requirements": ["is_owner", "is_viewing"] }), Some(10)));
        assert!(fits(&s, &json!({ "guild_requirements": ["is_owner", "is_viewing"] }), Some(11)));
        assert!(fits(&s, &json!({ "guild_requirements": ["guild_size"], "guild_size": [1000, null] }), None));
        assert!(!fits(&s, &json!({ "guild_requirements": ["guild_size_all"], "guild_size": [1000, null] }), None));
        assert!(fits(&s, &json!({ "guild_requirements": ["guild_size_all"], "guild_size": [10, null] }), None));
        assert!(!fits(&store(&[]), &json!({ "guild_requirements": ["is_owner"] }), None));
    }
}
