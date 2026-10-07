use egui::{CornerRadius, Frame, Image, Margin, Rect, RichText, Sense, Ui, vec2};

use super::{App, Pick};
use crate::markdown::twemoji;
use crate::state::{Emoji, ReactEmoji};
use crate::theme::*;

const COMMON: &str = "😀 grinning|😃 smiley|😄 smile|😁 grin|😆 laughing|😅 sweat_smile|🤣 rofl|😂 joy|🙂 slight_smile|🙃 upside_down|😉 wink|😊 blush|😇 innocent|🥰 smiling_face_with_hearts|😍 heart_eyes|🤩 star_struck|😘 kissing_heart|😋 yum|😛 stuck_out_tongue|😜 stuck_out_tongue_winking_eye|🤪 zany_face|😝 stuck_out_tongue_closed_eyes|🤑 money_mouth|🤗 hugging|🤭 hand_over_mouth|🤫 shushing_face|🤔 thinking|🤐 zipper_mouth|🤨 raised_eyebrow|😐 neutral_face|😑 expressionless|😶 no_mouth|😏 smirk|😒 unamused|🙄 rolling_eyes|😬 grimacing|😮‍💨 face_exhaling|🤥 lying_face|😌 relieved|😔 pensive|😪 sleepy|🤤 drooling_face|😴 sleeping|😷 mask|🤒 thermometer_face|🤕 head_bandage|🤢 nauseated_face|🤮 vomiting|🥵 hot_face|🥶 cold_face|🥴 woozy_face|😵 dizzy_face|🤯 exploding_head|🤠 cowboy|🥳 partying_face|😎 sunglasses|🤓 nerd|🧐 monocle|😕 confused|😟 worried|🙁 slight_frown|😮 open_mouth|😯 hushed|😲 astonished|😳 flushed|🥺 pleading_face|😦 frowning|😧 anguished|😨 fearful|😰 cold_sweat|😥 disappointed_relieved|😢 cry|😭 sob|😱 scream|😖 confounded|😣 persevere|😞 disappointed|😓 sweat|😩 weary|😫 tired_face|🥱 yawning_face|😤 triumph|😡 rage|😠 angry|🤬 face_with_symbols_over_mouth|😈 smiling_imp|👿 imp|💀 skull|💩 poop|🤡 clown|👻 ghost|👽 alien|🤖 robot|😺 smiley_cat|😹 joy_cat|😻 heart_eyes_cat|🙈 see_no_evil|🙉 hear_no_evil|🙊 speak_no_evil|💋 kiss|💯 100|💢 anger|💥 boom|💫 dizzy|💦 sweat_drops|💨 dash|💬 speech_balloon|💤 zzz|👋 wave|🤚 raised_back_of_hand|✋ raised_hand|🖖 vulcan|👌 ok_hand|🤌 pinched_fingers|🤏 pinching_hand|✌️ v|🤞 fingers_crossed|🤟 love_you_gesture|🤘 metal|🤙 call_me|👈 point_left|👉 point_right|👆 point_up_2|👇 point_down|☝️ point_up|👍 thumbsup|👎 thumbsdown|✊ fist|👊 punch|👏 clap|🙌 raised_hands|👐 open_hands|🤲 palms_up_together|🤝 handshake|🙏 pray|✍️ writing_hand|💪 muscle|🧠 brain|👀 eyes|👁️ eye|👅 tongue|👄 lips|🫡 saluting_face|🫠 melting_face|🫶 heart_hands|❤️ heart|🧡 orange_heart|💛 yellow_heart|💚 green_heart|💙 blue_heart|💜 purple_heart|🖤 black_heart|🤍 white_heart|🤎 brown_heart|💔 broken_heart|❣️ heart_exclamation|💕 two_hearts|💞 revolving_hearts|💓 heartbeat|💗 heartpulse|💖 sparkling_heart|💘 cupid|💝 gift_heart|🔥 fire|✨ sparkles|⭐ star|🌟 star2|⚡ zap|☀️ sunny|🌈 rainbow|❄️ snowflake|🌙 crescent_moon|🎉 tada|🎊 confetti_ball|🎈 balloon|🎁 gift|🏆 trophy|🥇 first_place|🎮 video_game|🎵 musical_note|🎶 notes|🍕 pizza|🍔 hamburger|🍟 fries|🌮 taco|🍣 sushi|🍩 doughnut|🍪 cookie|🎂 birthday|☕ coffee|🍺 beer|🍷 wine_glass|🥤 cup_with_straw|🐶 dog|🐱 cat|🦊 fox|🐻 bear|🐼 panda|🐸 frog|🐵 monkey_face|🐧 penguin|🐢 turtle|🐍 snake|🦀 crab|🐟 fish|🦆 duck|🦉 owl|🦄 unicorn|🐝 bee|🌸 cherry_blossom|🌹 rose|🌻 sunflower|🍀 four_leaf_clover|🌵 cactus|🌲 evergreen_tree|🚀 rocket|✈️ airplane|🚗 car|🏠 house|💻 computer|📱 iphone|⌨️ keyboard|🖥️ desktop|💾 floppy_disk|📷 camera|🔔 bell|📌 pushpin|📎 paperclip|🔒 lock|🔑 key|🔨 hammer|🛠️ tools|⚙️ gear|💡 bulb|📈 chart_with_upwards_trend|📉 chart_with_downwards_trend|✅ white_check_mark|☑️ ballot_box_with_check|✔️ heavy_check_mark|❌ x|❎ negative_squared_cross_mark|➕ heavy_plus_sign|➖ heavy_minus_sign|❓ question|❗ exclamation|‼️ bangbang|⚠️ warning|🚫 no_entry_sign|⛔ no_entry|🔴 red_circle|🟢 green_circle|🔵 blue_circle|🟡 yellow_circle|⬆️ arrow_up|⬇️ arrow_down|➡️ arrow_right|⬅️ arrow_left|🔁 repeat|🆗 ok|🆕 new|🆒 cool|🏳️‍🌈 rainbow_flag|🇺🇸 flag_us|🇬🇧 flag_gb|🇯🇵 flag_jp";

fn common() -> impl Iterator<Item = (&'static str, &'static str)> {
    COMMON.split('|').filter_map(|e| e.split_once(' '))
}

enum Chosen {
    Custom(Emoji),
    Unicode(&'static str),
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some((target, at)) = &app.picker else { return };
    let at = *at;
    let guild = app.ui.guild.unwrap_or(0);
    let customs: Vec<Emoji> = app.store.guilds.get(&guild).map(|g| g.emojis.clone()).unwrap_or_default();
    let query_id = egui::Id::new("picker_q");
    let mut q: String = ctx.data_mut(|d| d.get_temp(query_id).unwrap_or_default());
    let mut chosen = None;

    let area = egui::Area::new(egui::Id::new("picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new().fill(BG_SECONDARY).corner_radius(8).inner_margin(0).show(ui, |ui| {
                ui.set_width(418.0);
                Frame::new().inner_margin(Margin::symmetric(12, 12)).show(ui, |ui| {
                    Frame::new().fill(BG_TERTIARY).corner_radius(4).inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut q)
                                .frame(Frame::NONE)
                                .hint_text(RichText::new("Find the perfect emoji").color(TEXT_MUTED))
                                .font(reg(15.0))
                                .desired_width(ui.available_width()),
                        );
                        r.request_focus();
                    });
                });
                let ql = q.to_lowercase();
                egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let customs: Vec<&Emoji> = customs.iter().filter(|e| ql.is_empty() || e.name.to_lowercase().contains(&ql)).collect();
                    if !customs.is_empty() {
                        section(ui, "SERVER EMOJI");
                        grid(ui, customs.len(), |ui, i, r| {
                            Image::from_uri(customs[i].url()).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                        }, |i| {
                            chosen = Some(Chosen::Custom(customs[i].clone()));
                        }, |i| format!(":{}:", customs[i].name));
                    }
                    let uni: Vec<(&str, &str)> = common().filter(|(_, n)| ql.is_empty() || n.contains(&ql)).collect();
                    if !uni.is_empty() {
                        section(ui, "PEOPLE & THINGS");
                        grid(ui, uni.len(), |ui, i, r| {
                            Image::from_uri(twemoji(uni[i].0)).show_loading_spinner(false).paint_at(ui, r.shrink(4.0));
                        }, |i| {
                            chosen = Some(Chosen::Unicode(uni[i].0));
                        }, |i| format!(":{}:", uni[i].1));
                    }
                    ui.add_space(8.0);
                });
            });
        });

    ctx.data_mut(|d| d.insert_temp(query_id, q));
    let clicked_out = ctx.input(|i| i.pointer.any_pressed()) && !area.response.contains_pointer();
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if let Some(c) = chosen {
        match target {
            Pick::Composer => {
                let text = match &c {
                    Chosen::Custom(e) => format!("<{}:{}:{}>", if e.animated { "a" } else { "" }, e.name, e.id),
                    Chosen::Unicode(s) => s.to_string(),
                };
                if let Some(ch) = app.cur() {
                    app.drafts.entry(ch).or_default().push_str(&text);
                }
                app.focus_composer = true;
            }
            Pick::React(ch, id) => {
                let em = match c {
                    Chosen::Custom(e) => ReactEmoji { id: e.id, name: e.name, animated: e.animated },
                    Chosen::Unicode(s) => ReactEmoji { id: 0, name: s.to_string(), animated: false },
                };
                let (ch, id) = (*ch, *id);
                let mine = app.store.find_msg(ch, id).is_some_and(|m| m.reactions.iter().any(|r| r.emoji == em && r.me));
                if !mine {
                    app.react(ch, id, em, true);
                }
            }
        }
        app.picker = None;
        ctx.data_mut(|d| d.remove::<String>(query_id));
    } else if clicked_out || esc {
        app.picker = None;
        ctx.data_mut(|d| d.remove::<String>(query_id));
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(title).font(semi(12.0)).color(HEADER_SECONDARY));
    });
    ui.add_space(6.0);
}

fn grid(
    ui: &mut Ui,
    n: usize,
    mut paint: impl FnMut(&Ui, usize, Rect),
    mut pick: impl FnMut(usize),
    tip: impl Fn(usize) -> String,
) {
    let cols = 9;
    let cell = 44.0;
    for row in 0..n.div_ceil(cols) {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            for c in 0..cols {
                let i = row * cols + c;
                if i >= n {
                    break;
                }
                let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(4), BG_MOD_HOVER);
                }
                if ui.is_rect_visible(r) {
                    paint(ui, i, r);
                }
                if resp.on_hover_text(tip(i)).clicked() {
                    pick(i);
                }
            }
        });
    }
}
