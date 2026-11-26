use super::{Block, Boxed, Cx, Inline, Link, Marker, Spoilers, bold_item, fence, item, parse, reply_preview};
use crate::state::{Id, Set};

fn texts(text: &str, open: &Set<Id>) -> Vec<Inline> {
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::clicked(open, 7u64), edited: false, embed: false, heavy: false };
    parse(text, &cx).into_iter().filter_map(|b| if let Block::Text(t) = b { Some(t) } else { None }).collect()
}

fn covered(text: &str, open: &Set<Id>) -> Vec<(String, Id)> {
    let mut out = Vec::new();
    for t in texts(text, open) {
        for (r, l) in &t.links {
            if let Link::Spoiler(key) = l {
                out.push((t.job.text.chars().skip(r.start).take(r.len()).collect(), *key));
            }
        }
    }
    out
}

#[test]
fn a_click_opens_one_spoiler_and_leaves_the_others_covered() {
    let text = "||one|| and ||two||\n||three||";
    let mut open = Set::default();
    let all = covered(text, &open);
    assert_eq!(all.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["one", "two", "three"]);
    open.insert(all[1].1);
    let left = covered(text, &open);
    assert_eq!(left, [all[0].clone(), all[2].clone()]);
}

#[test]
fn a_covered_spoiler_takes_the_click_before_the_link_inside_it() {
    let t = &texts("||https://example.test/||", &Set::default())[0];
    assert!(matches!(t.links.first(), Some((_, Link::Spoiler(_)))));
    assert!(t.links.iter().any(|(_, l)| matches!(l, Link::Url(_))));
}

#[test]
fn a_covered_spoiler_shows_nothing_of_what_is_inside() {
    let text = "||<t:0:R> `code` words|| after";
    let mut open = Set::default();
    let t = &texts(text, &open)[0];
    let (inside, Boxed::Spoiler(Some(key))) = (t.boxes[0].0.clone(), t.boxes[0].1) else { panic!("not covered") };
    assert!(t.pills.is_empty() && t.boxes.len() == 1);
    let mut at = 0;
    for s in &t.job.sections {
        let chars = t.job.text[s.byte_range.start.0..s.byte_range.end.0].chars().count();
        if at < inside.end {
            assert_eq!(s.format.color, egui::Color32::TRANSPARENT);
        } else {
            assert_eq!(s.format.color, egui::Color32::WHITE);
        }
        at += chars;
    }
    open.insert(key);
    let t = &texts(text, &open)[0];
    assert_eq!(t.pills.len(), 1);
    assert_eq!(t.boxes.iter().map(|b| b.1).collect::<Vec<_>>(), [Boxed::Spoiler(None), Boxed::Code(16.0 * 0.85 * 0.2 + 1.0)]);
}

fn plain(blocks: &[Block]) -> String {
    blocks.iter().map(|b| if let Block::Text(t) = b { t.job.text.clone() } else { "?".into() }).collect()
}

fn blocks(text: &str) -> Vec<Block> {
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::all(false), edited: false, embed: false, heavy: false };
    parse(text, &cx)
}

#[test]
fn plain_text_is_medium_where_discord_makes_it_so() {
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let fonts = |heavy: bool| {
        let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::all(false), edited: false, embed: false, heavy };
        let b = parse("plain **bold**", &cx);
        let Block::Text(t) = &b[0] else { panic!("no text") };
        t.job.sections.iter().map(|s| s.format.font_id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(fonts(false), [crate::theme::reg(16.0), crate::theme::bold(16.0)]);
    assert_eq!(fonts(true), [crate::theme::medium(16.0), crate::theme::bold(16.0)]);
}

#[test]
fn a_list_line_is_a_bullet_and_a_space() {
    assert_eq!(item("- one"), Some((0, "-", "one")));
    assert_eq!(item("  *   two"), Some((2, "*", "two")));
    assert_eq!(item("12. twelve"), Some((0, "12.", "twelve")));
    for not in ["-# small", "*italic* text", "1.5 stars", "2023 was long", "-", "- ", "text - more"] {
        assert_eq!(item(not), None, "{not}");
    }
}

#[test]
fn a_list_numbers_from_its_first_item_and_sits_in_by_its_digits() {
    let b = blocks("before\n9. nine\n1. ten\n- eleven\nafter");
    let [Block::Text(before), Block::List(list), Block::Text(after)] = &b[..] else { panic!("not text, list, text") };
    assert_eq!((before.job.text.as_str(), after.job.text.as_str()), ("before", "after"));
    assert_eq!(list.items.iter().map(|(m, _)| *m).collect::<Vec<_>>(), [Marker::Number(9), Marker::Number(10), Marker::Number(11)]);
    assert_eq!(list.items.iter().map(|(_, b)| plain(b)).collect::<Vec<_>>(), ["nine", "ten", "eleven"]);
    assert_eq!((list.indent, list.line), ((0.4 + 0.6 * 2.0) * 16.0, 22.0));
}

#[test]
fn deeper_lines_stay_with_the_item_above_them() {
    let b = blocks("- one\n  more of one\n  - inside\n- two");
    let [Block::List(list)] = &b[..] else { panic!("not one list") };
    assert_eq!((list.indent, list.items.len()), (16.0, 2));
    let (Marker::Disc, [Block::Text(one), Block::List(inside)]) = (list.items[0].0, &list.items[0].1[..]) else { panic!("no list inside the first item") };
    assert_eq!(one.job.text, "one\nmore of one");
    assert_eq!((inside.items[0].0, plain(&inside.items[0].1)), (Marker::Circle, "inside".to_string()));
    assert_eq!(plain(&list.items[1].1), "two");
}

#[test]
fn a_list_item_holds_no_heading() {
    let b = blocks("- # not a title");
    let [Block::List(list)] = &b[..] else { panic!("not one list") };
    assert_eq!(plain(&list.items[0].1), "# not a title");
}

#[test]
fn a_heading_keeps_its_margins() {
    let none = Set::default();
    let t = &texts("above\n## Title\nbelow\n# One\n### Two", &none)[0];
    assert_eq!(t.job.text, "above\n\nTitle\n\nbelow\n\nOne\n\nTwo");
    let gaps: Vec<f32> = t.job.sections.iter().filter(|s| s.format.font_id.size == 1.0).map(|s| s.format.line_height.unwrap()).collect();
    assert_eq!((gaps, t.lead), (vec![16.0, 8.0, 16.0, 16.0], 0.0));
    assert_eq!(texts("# Title", &none)[0].lead, 8.0);
    assert_eq!(texts("### Title\nbelow", &none)[0].lead, 4.0);
    assert_eq!(texts("```\ncode\n```\n# Title", &none)[0].lead, 16.0);
}

#[test]
fn quotes_and_subtext_are_in_the_subtle_colour() {
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::all(false), edited: false, embed: false, heavy: false };
    let subtle = crate::theme::pal().header_secondary;
    let blocks = parse("> quoted\n-# small", &cx);
    let [Block::Quote(q), Block::Text(small)] = &blocks[..] else { panic!("not a quote and a line") };
    let Block::Text(quoted) = &q[0] else { panic!("no text in the quote") };
    assert_eq!(quoted.job.sections[0].format.color, subtle);
    assert_eq!((small.job.sections[0].format.color, small.job.sections[0].format.font_id.size), (subtle, 14.0));
}

#[test]
fn inline_code_sits_in_a_box_with_room_either_side() {
    let t = &texts("run `cargo test` now", &Set::default())[0];
    let (r, kind, line, h) = t.boxes[0].clone();
    assert_eq!(t.job.text.chars().skip(r.start).take(r.len()).collect::<String>(), "cargo test");
    let pad = 16.0 * 0.85 * 0.2 + 1.0;
    assert_eq!((kind, line, h), (Boxed::Code(pad), 22.0, 20.0));
    let pads: Vec<f32> = t.job.sections.iter().map(|s| s.leading_space).collect();
    assert_eq!(pads, [0.0, pad, pad]);
}

#[test]
fn what_follows_a_bolded_list_item_drops_to_its_own_line() {
    assert_eq!(bold_item("**11. Respect All Players** Be respectful and kind."), Some(("**11. Respect All Players**", "Be respectful and kind.")));
    assert_eq!(bold_item("**- Rules** apply"), Some(("**- Rules**", "apply")));
    assert_eq!(bold_item("**11. Respect All Players**"), None);
    assert_eq!(bold_item("**Respect** everyone"), None);
    assert_eq!(bold_item("**3.5 stars** out of five"), None);
    assert_eq!(bold_item("11. **Respect** everyone"), None);
}

#[test]
fn a_fence_names_its_language_only_on_its_own_line() {
    assert_eq!(fence("rust\nfn main() {}\n"), ("rust", "fn main() {}"));
    assert_eq!(fence("c++\n\nint x;"), ("c++", "int x;"));
    assert_eq!(fence("rust"), ("", "rust"));
    assert_eq!(fence("rust\n"), ("", "rust"));
    assert_eq!(fence("\nfn main() {}"), ("", "fn main() {}"));
    assert_eq!(fence("two words\nx"), ("", "two words\nx"));
}

#[test]
fn deeply_nested_quotes_and_lists_do_not_overflow() {
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::all(false), edited: false, embed: false, heavy: false };
    assert!(!parse(&format!("{}x", "> ".repeat(500)), &cx).is_empty());
    assert!(!parse(&format!("{}x", "- ".repeat(500)), &cx).is_empty());
    assert!(!parse(&format!("{}x", "**".repeat(1000)), &cx).is_empty());
}

#[test]
fn reply_preview_preserves_mention_pills_and_strips_delimiters() {
    let mut store = crate::state::Store::default();
    let mut guild = crate::state::Guild { id: 10, ..Default::default() };
    guild.roles.insert(20, crate::state::Role { id: 20, name: "Mods".into(), color: 0x5865f2, ..Default::default() });
    store.guilds.insert(10, guild);
    store.users.insert(30, crate::state::User { id: 30, username: "alice".into(), global_name: Some("Alice".into()), ..Default::default() });
    store.channels.insert(40, crate::state::Channel { id: 40, guild: 10, name: "general".into(), ..Default::default() });

    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let cx = Cx { store: &store, ctx: &ctx, guild: 10, size: 14.0, color: egui::Color32::WHITE, spoilers: Spoilers::all(true), edited: false, embed: true, heavy: false };

    let prev = reply_preview("**hi** <@30>\ncheck <#40> and <@&20> @here", &cx, 240.0);
    assert_eq!(prev.job.text, "hi @Alice check #general and @Mods @here");
    assert_eq!(prev.pills.len(), 4);
    assert_eq!(prev.job.wrap.max_rows, 1);
    assert_eq!(prev.job.wrap.overflow_character, Some('…'));
    assert_eq!(prev.job.wrap.max_width, 240.0);
}

#[test]
fn mention_pills_substitute_twemoji_while_keeping_pill_and_link_ranges() {
    let mut store = crate::state::Store::default();
    let mut guild = crate::state::Guild { id: 10, ..Default::default() };
    guild.roles.insert(20, crate::state::Role { id: 20, name: "Restricted Lol 😂😂😂".into(), color: 0x5865f2, ..Default::default() });
    store.guilds.insert(10, guild);
    store.users.insert(30, crate::state::User { id: 30, username: "bob".into(), global_name: Some("🔥 *Bob*".into()), ..Default::default() });
    store.channels.insert(40, crate::state::Channel { id: 40, guild: 10, name: "🎉-party".into(), ..Default::default() });

    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let open = Set::default();
    let cx = Cx { store: &store, ctx: &ctx, guild: 10, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::clicked(&open, 9u64), edited: false, embed: false, heavy: false };

    let b = parse("<@&20> <@30> <#40>", &cx);
    let [Block::Text(t)] = &b[..] else { panic!("expected one text block") };
    assert_eq!(t.pills.len(), 3);
    assert_eq!(t.links.len(), 3);
    assert_eq!(t.emojis.len(), 5);
    assert!(t.emojis[..3].iter().all(|(_, u, size, _)| u.ends_with("/1f602.png") && *size == 18.0));
    assert!(t.emojis[3].1.ends_with("/1f525.png"));
    assert!(t.emojis[4].1.ends_with("/1f389.png"));
    // every Twemoji placeholder sits inside its mention's pill and link ranges
    for (idx, ..) in &t.emojis[..3] {
        assert!(t.pills[0].0.contains(idx) && t.links[0].0.contains(idx));
    }
    assert!(matches!(t.links[0].1, Link::Role(20)));
    assert!(t.pills[1].0.contains(&t.emojis[3].0) && t.links[1].0.contains(&t.emojis[3].0));
    assert!(matches!(t.links[1].1, Link::User(30)));
    assert!(t.job.text.contains("*Bob*"), "mention label must not parse markdown");
    assert!(t.pills[2].0.contains(&t.emojis[4].0) && t.links[2].0.contains(&t.emojis[4].0));
    assert!(matches!(t.links[2].1, Link::Channel(40)));

    // covered spoiler hides both pill and Twemoji until revealed
    let b = parse("||<@&20>||", &cx);
    let [Block::Text(s)] = &b[..] else { panic!("expected one spoiler block") };
    assert!(s.pills.is_empty() && s.emojis.is_empty());
    assert!(matches!(s.links.first(), Some((_, Link::Spoiler(_)))));
}
