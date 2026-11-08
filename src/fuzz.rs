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
