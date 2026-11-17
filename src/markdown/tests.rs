use super::{Block, Boxed, Cx, Inline, Link, Spoilers, bold_item, fence, parse};
use crate::state::{Id, Set};

fn texts(text: &str, open: &Set<Id>) -> Vec<Inline> {
    let store = crate::state::Store::default();
    let ctx = egui::Context::default();
    ctx.set_fonts(crate::theme::fonts(false));
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::clicked(open, 7u64), edited: false, embed: false };
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
    let cx = Cx { store: &store, ctx: &ctx, guild: 0, size: 16.0, color: egui::Color32::WHITE, spoilers: Spoilers::all(false), edited: false, embed: false };
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
