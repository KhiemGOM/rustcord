//! QoL steps from QOL.md that came after the first pass: every dialog closing on a click outside,
//! @time, the error toast's details, select keys, the cropper, Modify Attachment.

use egui::{Event, Key, Modifiers, pos2};

use super::Run;
use crate::ui::{App, attach, composer, create, crop, dmpick, keys, popups};


const DIALOGS: [&str; 11] = [
    "a confirmation", "Join a Server", "Leaving Discord", "Forward To", "Create Channel", "the quick switcher", "All Active Threads", "Notification Settings",
    "Keyboard Combos", "Edit Image", "Modify Attachment",
];

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = image::DynamicImage::new_rgb8(w, h).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
    out
}

impl Run {
    fn chord(&mut self, key: Key, modifiers: Modifiers) {
        let k = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        self.queue.push_back(vec![k(true)]);
        self.queue.push_back(vec![k(false)]);
    }

    pub(super) fn more_steps(&mut self, app: &mut App, ctx: &egui::Context) {
        let general = self.target.1;
        let guild = self.target.0;
        match self.step {
            1900 => {
                app.select_guild(Some(guild));
                app.open(general);
                self.loaded = 0;
                self.next(600);
            }
            // each dialog opened, then the dark round it clicked: it has to close
            1901 => {
                let last = app.store.msgs.get(&general).and_then(|e| e.list.last()).map_or(0, |m| m.id);
                match self.loaded {
                    0 => app.confirm = Some(crate::ui::Confirm::new("Sure?", "Really?", "Yes", |_: &mut App| {})),
                    1 => app.invite = Some(String::new()),
                    2 => app.leaving = Some(("https://example.org/out".into(), false)),
                    3 => app.forward = Some(popups::Forward::new(general, last)),
                    4 => app.create = Some(create::Form::new(create::Kind::Channel, guild, None)),
                    5 => app.switcher = Some(String::new()),
                    6 => app.active_threads = Some(guild),
                    7 => app.guild_notifs = Some(guild),
                    8 => app.combos = Some(keys::Combos::default()),
                    9 => crop::open(app, png(64, 32), 1.0, |_: &mut App, _: String| {}),
                    10 => {
                        let dir = std::env::temp_dir().join(format!("rustcord_e2e_{}", std::process::id()));
                        let _ = std::fs::create_dir_all(&dir);
                        let path = dir.join("modify.png");
                        let _ = std::fs::write(&path, png(16, 16));
                        app.files.push(composer::Upload::new(path));
                        app.modify = app.files.first().map(|f| attach::Modify::new(0, f));
                    }
                    _ => {
                        self.step = 1904;
                        return;
                    }
                }
                self.next(300);
            }
            1902 => {
                let s = ctx.content_rect();
                self.click(pos2(s.left() + 8.0, s.bottom() - 8.0));
                self.next(400);
            }
            1903 => {
                let open = match self.loaded {
                    0 => app.confirm.is_some(),
                    1 => app.invite.is_some(),
                    2 => app.leaving.is_some(),
                    3 => app.forward.is_some(),
                    4 => app.create.is_some(),
                    5 => app.switcher.is_some(),
                    6 => app.active_threads.is_some(),
                    7 => app.guild_notifs.is_some(),
                    8 => app.combos.is_some(),
                    9 => app.crop.is_some(),
                    _ => app.modify.is_some(),
                };
                if open {
                    let name = DIALOGS[self.loaded];
                    return self.fail(format!("clicking outside {name} didn't close it"));
                }
                app.files.clear();
                self.loaded += 1;
                self.step = 1901;
                self.wait = std::time::Instant::now() + std::time::Duration::from_millis(200);
            }
            1904 => {
                self.mark("qol_backdrops");
                self.next(200);
            }
            // @time then a time: the list of styles, and Enter writes the first as a timestamp
            1905 => {
                app.drafts.insert(general, "@time 5pm".into());
                app.focus_composer = true;
                app.caret_end = true;
                self.next(600);
            }
            1906 => {
                if app.ac_open {
                    self.mark("qol_time_list");
                    self.key(Key::Enter);
                    self.next(400);
                }
            }
            1907 => {
                let when = crate::ui::when::parse("5pm", chrono::Local::now()).map_or(0, |t| t.timestamp());
                if app.drafts.get(&general).is_some_and(|d| *d == format!("<t:{when}:t> ")) {
                    self.mark("qol_time");
                    app.drafts.remove(&general);
                    self.next(200);
                }
            }
            // an error toast says how to see what went wrong; shift+click shows it
            1908 => {
                app.error_toast("something broke for the e2e run".into());
                self.next(500);
            }
            1909 => {
                let Some(r) = app.rects.get("toast").copied() else { return };
                self.shift = true;
                self.click(r.center());
                self.next(500);
            }
            1910 => {
                self.shift = false;
                if app.confirm.as_ref().is_some_and(|c| c.title == "Error Details" && c.body.contains("e2e run")) {
                    self.mark("qol_error_details");
                    app.confirm = None;
                    self.next(300);
                }
            }
            // Enter on Leaving Discord visits the site
            1911 => {
                app.leaving = Some(("https://example.org/enter".into(), false));
                self.next(400);
            }
            1912 => {
                self.key(Key::Enter);
                self.next(400);
            }
            1913 => {
                if app.leaving.is_none() && self.opened.iter().any(|u| u == "https://example.org/enter") {
                    self.mark("qol_enter_dialog");
                    self.next(200);
                }
            }
            // a select: letters jump to a row, Enter picks it and shuts the list
            1914 => {
                app.open_chan_settings(general);
                self.next(1200);
            }
            1915 => {
                let Some(r) = Self::at(ctx, "dropdown:Off").filter(|_| app.chan_settings.as_ref().is_some_and(|s| !s.draft.is_null())) else { return };
                self.click(r.center());
                self.next(500);
            }
            1916 => {
                self.queue.push_back(vec![Event::Text("1".into())]);
                self.next(400);
            }
            1917 => {
                self.key(Key::Enter);
                self.next(500);
            }
            1918 => {
                if app.chan_settings.as_ref().is_some_and(|s| s.draft["rate_limit_per_user"] == 10) && !egui::Popup::is_any_open(ctx) {
                    self.mark("qol_select_keys");
                    self.key(Key::Escape);
                    self.next(500);
                }
            }
            // the cropper: keys move and turn the picture, Apply hands back a data uri
            1919 => {
                if app.chan_settings.is_some() {
                    app.chan_settings = None;
                }
                crop::open(app, png(160, 80), 1.0, |app: &mut App, uri: String| app.toast(format!("cropped {}", uri.starts_with("data:image/png;base64,"))));
                self.next(500);
            }
            1920 => {
                let Some(v) = Self::at(ctx, "crop_view").filter(|_| app.crop.is_some()) else { return };
                self.chord(Key::ArrowLeft, Modifiers::SHIFT);
                self.chord(Key::ArrowRight, Modifiers::ALT);
                let at = v.center();
                self.queue.push_back(vec![Event::PointerMoved(at)]);
                self.queue.push_back(vec![Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }]);
                self.queue.push_back(vec![Event::PointerMoved(at + egui::vec2(-20.0, 0.0))]);
                self.queue.push_back(vec![Event::PointerButton { pos: at + egui::vec2(-20.0, 0.0), button: egui::PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE }]);
                self.next(500);
            }
            1921 => {
                let Some(r) = Self::at(ctx, "crop_apply") else { return };
                self.click(r.center());
                self.next(800);
            }
            1922 => {
                if app.crop.is_none() && app.toast.as_ref().is_some_and(|(t, _)| t == "cropped true") {
                    self.mark("qol_crop");
                    self.next(200);
                }
            }
            // an upload card with the keyboard: E opens Modify Attachment, D takes the card off
            1923 => {
                let dir = std::env::temp_dir().join(format!("rustcord_e2e_{}", std::process::id()));
                let _ = std::fs::create_dir_all(&dir);
                let path = dir.join("card.png");
                let _ = std::fs::write(&path, png(16, 16));
                app.files.push(composer::Upload::new(path));
                self.next(600);
            }
            1924 => {
                let Some(r) = Self::at(ctx, "card:0") else { return };
                self.click(pos2(r.center().x, r.center().y + 20.0));
                self.next(300);
            }
            1925 => {
                self.key(Key::E);
                self.next(500);
            }
            1926 => {
                if app.modify.is_none() {
                    return;
                }
                self.chord(Key::A, Modifiers::COMMAND);
                self.queue.push_back(vec![Event::Text("renamed.png".into())]);
                self.next(400);
            }
            1927 => {
                let Some(r) = Self::at(ctx, "modify:save") else { return };
                self.click(r.center());
                self.next(500);
            }
            1928 => {
                if app.modify.is_none() && app.files.first().is_some_and(|f| f.name == "renamed.png") {
                    self.mark("qol_modify");
                    self.next(200);
                }
            }
            1929 => {
                let Some(r) = Self::at(ctx, "card:0") else { return };
                self.click(pos2(r.center().x, r.center().y + 20.0));
                self.next(300);
            }
            1930 => {
                self.key(Key::D);
                self.next(500);
            }
            1931 => {
                if app.files.is_empty() {
                    self.mark("qol_card_keys");
                    self.next(200);
                }
            }
            // Select Friends: two picked, Backspace takes the last off, picked again, Create Group DM
            1932 => {
                app.group_pick = Some(dmpick::Pick::new(None));
                self.next(600);
            }
            1933 => {
                let Some(a) = Self::at(ctx, "pick:101") else { return };
                self.click(a.center());
                self.next(400);
            }
            1934 => {
                let Some(b) = Self::at(ctx, "pick:102") else { return };
                self.click(b.center());
                self.next(500);
            }
            1935 => {
                if app.group_pick.as_ref().is_some_and(|s| s.picked.len() == 2) {
                    self.mark("qol_newdm_pick");
                    self.key(Key::Backspace);
                    self.next(400);
                }
            }
            1936 => {
                if app.group_pick.as_ref().is_some_and(|s| s.picked == [101]) {
                    self.mark("qol_newdm_backspace");
                    let Some(b) = Self::at(ctx, "pick:102") else { return };
                    self.click(b.center());
                    self.next(400);
                }
            }
            1937 => {
                if app.group_pick.as_ref().is_some_and(|s| s.picked.len() == 2) {
                    let Some(r) = Self::at(ctx, "pick:go") else { return };
                    self.click(r.center());
                    self.next(800);
                }
            }
            1938 => {
                if app.group_pick.is_none() && app.cur() == Some(3011) && app.store.channels.get(&3011).is_some_and(|c| c.kind == 3) {
                    self.mark("qol_group_dm");
                    self.next(200);
                }
            }
            // the custom status: Shift+Enter does nothing, Enter saves
            1939 => {
                app.status_text = Some(crate::ui::cstatus::State::from(None));
                self.next(500);
            }
            1940 => {
                self.queue.push_back(vec![Event::Text("e2e status".into())]);
                self.shift = true;
                self.chord(Key::Enter, Modifiers::SHIFT);
                self.next(500);
            }
            1941 => {
                self.shift = false;
                if app.status_text.is_some() {
                    self.mark("qol_status_shift_enter");
                    self.key(Key::Enter);
                    self.next(500);
                }
            }
            1942 => {
                if app.status_text.is_none() && app.store.custom_status.as_deref() == Some("e2e status") {
                    self.mark("qol_custom_status");
                    app.store.custom_status = None;
                    self.next(200);
                }
            }
            _ => self.step = 2300,
        }
    }
}
