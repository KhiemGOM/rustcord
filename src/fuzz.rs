use egui::{Color32, FontId};

#[allow(dead_code)]
#[path = "ui/settings/wire.rs"]
mod wire;

fn next(seed: &mut u64) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as usize
}

#[test]
fn fuzz_settings_lengths() {
    let mut seed = 0x83a2_f794_c516_b301;
    for case in 0..4096 {
        let mut bytes = vec![0x0a];
        let len = if case == 0 { u64::MAX } else { next(&mut seed) as u64 };
        let mut n = len;
        while n >= 128 {
            bytes.push(n as u8 | 128);
            n >>= 7;
        }
        bytes.push(n as u8);
        for _ in 0..case % 32 {
            bytes.push(next(&mut seed) as u8);
        }
        let parsed = wire::decode(&bytes);
        assert!(parsed.is_empty(), "case {case}: accepted length {len}");
    }
}

#[test]
fn fuzz_relative_timestamps() {
    for seconds in [i64::MIN, i64::MIN + 1, -1, 0, i64::MAX - 1, i64::MAX] {
        let text = crate::markdown::fmt_ts(seconds, 'R');
        assert!(!text.contains('-'), "{seconds}: {text}");
    }
}

#[test]
fn fuzz_message_text() {
    let mut seed = 0x9126_b35e_42ca_7d01;
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    let mut frame = ctx.run_ui(Default::default(), |_| {});
    frame.textures_delta.clear();
    let cx = crate::markdown::Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: Color32::WHITE, revealed: false, edited: false, embed: false };
    let tokens = ["a", "é", "中", "🦀", "\u{200d}", "\u{fe0f}", "\0", "\n", "\\", "*", "**", "_", "~~", "||", "`", "```", "> ", "<", ">", "[", "](", ")", ":", "<@1>", "<t:0:R>", "https://a.b/", "<:x:1>"];
    for case in 0..4096 {
        let mut text = String::new();
        for _ in 0..1 + case % 64 {
            text.push_str(tokens[next(&mut seed) % tokens.len()]);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = crate::markdown::parse(&text, &cx);
            let _ = crate::markdown::plain(&text, &store, 0);
            for lang in ["rust", "html", "python", "json"] {
                let job = crate::highlight::fence(lang, &text, FontId::monospace(14.0), Color32::WHITE, Color32::GRAY);
                assert_eq!(job.text, text);
                for section in &job.sections {
                    let _ = &job.text[section.byte_range.start.0..section.byte_range.end.0];
                }
            }
        }));
        assert!(result.is_ok(), "case {case}, text {text:?}");
    }
}

#[test]
fn fuzz_gateway_events() {
    let mut seed = 0x7183_feca_2b19_d405;
    let mut store = crate::state::Store::default();
    let events = ["MESSAGE_CREATE", "MESSAGE_UPDATE", "MESSAGE_DELETE", "GUILD_MEMBER_UPDATE", "GUILD_ROLE_UPDATE", "CHANNEL_UPDATE", "PRESENCE_UPDATE", "TYPING_START", "VOICE_STATE_UPDATE", "RELATIONSHIP_ADD", "MESSAGE_REACTION_ADD", "MESSAGE_REACTION_REMOVE"];
    let values = [serde_json::Value::Null, serde_json::json!(false), serde_json::json!(0), serde_json::json!(u64::MAX), serde_json::json!("🦀"), serde_json::json!([]), serde_json::json!({})];
    let fields = ["id", "channel_id", "guild_id", "user_id", "author", "content", "roles", "member", "emoji", "status", "timestamp", "mentions", "attachments", "embeds"];
    for case in 0..4096 {
        let mut payload = serde_json::Map::new();
        for field in fields {
            if next(&mut seed) % 3 != 0 {
                payload.insert(field.into(), values[next(&mut seed) % values.len()].clone());
            }
        }
        let event = events[next(&mut seed) % events.len()];
        let payload = serde_json::Value::Object(payload);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store.apply(event, &payload)));
        assert!(result.is_ok(), "case {case}, {event}: {payload}");
    }
}

#[test]
fn suppressed_mentions_do_not_raise_badges() {
    use crate::state::{Channel, Guild, Store, Tracked};
    use serde_json::json;

    for suppression in 0..4 {
        for mentions in 0..8 {
            let mut store = Store::default();
            store.me = 1;
            store.channels.insert(20, Channel { id: 20, guild: 10, ..Default::default() });
            store.guilds.insert(10, Guild { id: 10, channels: vec![20], my_roles: vec![7], ..Default::default() });
            let mut store = Tracked::from(store);
            store.apply("USER_GUILD_SETTINGS_UPDATE", &json!({ "guild_id": "10",
                "suppress_everyone": suppression & 1 != 0, "suppress_roles": suppression & 2 != 0 }));
            assert_eq!(store.mention_count(), 0);
            let message = json!({ "id": "100", "channel_id": "20", "guild_id": "10", "author": { "id": "2" },
                "mentions": if mentions & 1 != 0 { json!([{ "id": "1" }]) } else { json!([]) },
                "mention_everyone": mentions & 2 != 0,
                "mention_roles": if mentions & 4 != 0 { json!(["7"]) } else { json!([]) } });
            let expected = mentions & 1 != 0 || (mentions & 2 != 0 && suppression & 1 == 0) || (mentions & 4 != 0 && suppression & 2 == 0);
            let parsed = store.parse_message(&message);
            assert_eq!(store.mentions_me(&parsed), expected, "suppression {suppression}, mentions {mentions}");
            store.apply("MESSAGE_CREATE", &message);
            assert_eq!(store.mention_count(), u32::from(expected));
            assert_eq!(store.guild_badge(10).1, u32::from(expected));
        }
    }
}

#[test]
fn partial_notification_updates_keep_suppression() {
    use serde_json::json;

    let mut store = crate::state::Store::default();
    store.apply("USER_GUILD_SETTINGS_UPDATE", &json!({ "guild_id": "10", "suppress_everyone": true, "suppress_roles": true }));
    store.apply("USER_GUILD_SETTINGS_UPDATE", &json!({ "guild_id": "10", "channel_overrides": [{ "channel_id": "20", "muted": false }] }));
    assert!(store.gprefs[&10].suppress_everyone);
    assert!(store.gprefs[&10].suppress_roles);
    store.apply("USER_GUILD_SETTINGS_UPDATE", &json!({ "guild_id": "10", "suppress_everyone": false }));
    assert!(!store.gprefs[&10].suppress_everyone);
    assert!(store.gprefs[&10].suppress_roles);
}

#[test]
fn notification_updates_keep_channel_flags() {
    let mut store = crate::state::Store::default();
    store.apply("USER_GUILD_SETTINGS_UPDATE", &serde_json::json!({"guild_id":"1", "flags":16384,
        "channel_overrides":[{"channel_id":"2", "flags":6144}]}));
    store.apply("USER_GUILD_SETTINGS_UPDATE", &serde_json::json!({"guild_id":"1",
        "channel_overrides":[{"channel_id":"2", "muted":false}]}));
    assert_eq!(store.chan_flags.get(&2), Some(&6144));
    assert!(store.opted.contains(&2));
    assert!(store.opt_in.contains(&1));
    store.apply("USER_GUILD_SETTINGS_UPDATE", &serde_json::json!({"guild_id":"1", "flags":0,
        "channel_overrides":[{"channel_id":"2", "flags":0}]}));
    assert_eq!(store.chan_flags.get(&2), Some(&0));
    assert!(!store.opted.contains(&2));
    assert!(!store.opt_in.contains(&1));
}

#[test]
fn worn_decorations_request_animation() {
    let user = crate::state::User { decoration: Some("a_decoration".into()), ..Default::default() };
    assert!(user.decoration_url().unwrap().contains("passthrough=true"));
}
