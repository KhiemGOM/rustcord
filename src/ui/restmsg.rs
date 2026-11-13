use super::*;

impl App {
    pub(super) fn on_rest(&mut self, tag: Tag, res: Result<serde_json::Value, String>) {
        if let Tag::Link(k) = tag {
            return self.on_link_rest(k, res);
        }
        if let Tag::Buy(k, g) = tag {
            return buy::reply(self, k, g, res);
        }
        if let (Tag::Remote(k), Err(e)) = (&tag, &res) {
            return settings::on_remote(self, k, Err(e.clone()));
        }
        if let Tag::Act(a) = &tag {
            return self.acted(a, res.map(drop));
        }
        let v = match res {
            Ok(v) => v,
            Err(e) => {
                match tag {
                    Tag::Activity(k) => return activity::failed(self, &k, e),
                    Tag::AddApp(_) => return addapp::done(self, Err(e)),
                    Tag::InvitePreview(_) => return,
                    Tag::ChatInvite(code) => return self.chat_invites.failed(&code),
                    Tag::PollVotes(..) | Tag::TopEmojis(_) => {}
                    Tag::Source(s, id) => {
                        self.sources.insert((s, id), serde_json::json!({}));
                        return;
                    }
                    Tag::ChatJoin(code) => self.chat_invites.joined(&code),
                    Tag::Pins(_) => self.pins = None,
                    Tag::Report(_) => return self.report_failed(e),
                    Tag::Inbox => self.inbox = None,
                    // the feed is a nicety; when it fails there's just no activity section
                    Tag::Keyed(k) => {
                        self.keyed.insert(k, Some(Err(e)));
                        return;
                    }
                    Tag::Feed | Tag::Favs | Tag::AppIcons(_) | Tag::Product(_) | Tag::Saved(_) | Tag::Note(_) | Tag::Outbox(_) | Tag::GuildProfile(_) | Tag::ModView(_) => return,
                    Tag::Boosts(g, k) => {
                        if let Some(i) = self.boost_info.get_mut(&g) {
                            i.loaded |= 1 << k;
                        }
                    }
                    Tag::Created => {
                        if let Some(f) = &mut self.create {
                            f.busy = false;
                        }
                    }
                    Tag::NewServer => {
                        if let Some(w) = &mut self.wizard {
                            w.busy = false;
                        }
                    }
                    // an empty list rather than a spinner that never stops
                    Tag::Directory(k) => directory::loaded(self, k, serde_json::json!([])),
                    Tag::Server(k) => {
                        if let Some(s) = &mut self.server {
                            s.errors = self.fail.as_ref().map(|f| f.0.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| vec![(String::new(), e.clone())]);
                        }
                        if k == "automod_regex" {
                            return server::loaded(self, k, serde_json::json!([]));
                        }
                        server::loaded(self, k, serde_json::json!([]))
                    }
                    Tag::Chan("saved") => {
                        if let Some(s) = &mut self.chan_settings {
                            s.saving = false;
                        }
                    }
                    Tag::Chan(k) if k != "channel" => channel::loaded(self, k, serde_json::json!([])),
                    Tag::Home("members") => community::loaded(self, "members", serde_json::json!({ "members": [] })),
                    Tag::Events(g, k) => events::loaded(self, g, k, &serde_json::json!([])),
                    Tag::Joins(g, i) => applications::failed(self, g, i),
                    Tag::JoinsFor(..) => {}
                    Tag::JoinsAct(g) => applications::done(self, g, false, &serde_json::Value::Null),
                    Tag::EventUsers(g, e) => events::users_loaded(self, g, e, &serde_json::json!([])),
                    Tag::Quests(k) => quests::loaded(self, k, &serde_json::json!({})),
                    Tag::Follow => follow::done(self, false),
                    Tag::AddFriend(_) => {
                        let why = if e.contains("DiscordTag") { "No one with that name could be found." } else { "Hm, didn’t work. Double check that the username is correct." };
                        return self.add_friend_note = Some((false, why.into()));
                    }
                    _ => {}
                }
                return self.error_toast(e);
            }
        };
        match tag {
            Tag::Link(_) | Tag::Buy(..) => {}
            Tag::Pins(ch) => {
                let list: Vec<Message> = v["items"].as_array().into_iter().flatten().map(|p| self.store.parse_message(&p["message"])).collect();
                if self.pins.as_ref().is_some_and(|(c, _)| *c == ch) {
                    self.pins = Some((ch, Some(list)));
                }
            }
            Tag::Report(submit) => self.report_loaded(submit, v),
            Tag::Reactors(_, msg, key, burst) => self.store.set_reactors((msg, key, burst), &v),
            Tag::Soundboard(g) => {
                let items = if g == 0 { v.as_array() } else { v["items"].as_array() };
                let list: Vec<crate::state::Sound> = items.into_iter().flatten().map(crate::state::Sound::parse).collect();
                if let Some((_, slot)) = self.soundboard.as_mut() {
                    let all = slot.get_or_insert_with(Vec::new);
                    if g == 0 {
                        all.extend(list);
                    } else {
                        all.splice(0..0, list);
                    }
                }
            }
            Tag::Search(_) => {
                let list: Vec<Message> = v["messages"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|group| group.as_array().and_then(|g| g.first()))
                    .map(|m| self.store.parse_message(m))
                    .collect();
                if let Some(s) = &mut self.search {
                    s.total = v["total_results"].as_u64().unwrap_or(list.len() as u64);
                    s.results = Some(list);
                }
            }
            Tag::Profile(u, g) => {
                if v["user"].is_object() {
                    self.store.apply("USER_UPDATE", &v["user"]);
                }
                for f in v["mutual_friends"].as_array().into_iter().flatten() {
                    self.store.apply("USER_UPDATE", f);
                }
                if g != 0 {
                    self.member_profiles.insert((u, g), v);
                } else {
                    self.profiles.insert(u, v);
                }
            }
            Tag::Outbox(u) => {
                self.outbox.insert(u, profile::recents(&v));
            }
            Tag::GuildProfile(g) => {
                self.guild_profiles.insert(g, v);
            }
            Tag::ModView(u) => modview::loaded(self, u, &v),
            Tag::Inbox => {
                let list: Vec<Message> = v.as_array().into_iter().flatten().map(|m| self.store.parse_message(m)).collect();
                if let Some(i) = &mut self.inbox {
                    i.mentions = Some(list);
                }
            }
            Tag::Forum(f) => {
                self.store.load_forum(f, &v);
                self.request_members(f);
            }
            Tag::Boosts(g, k) => boosts::load(self, g, k, &v),
            Tag::Created => create::created(self, &v),
            Tag::NewServer => newserver::created(self, &v),
            Tag::Lurk(g) => {
                self.store.lurking.insert(g);
            }
            Tag::Joined(g) => {
                self.store.lurking.remove(&g);
            }
            Tag::Directory(k) => directory::loaded(self, k, v),
            Tag::Server(k) => server::loaded(self, k, v),
            Tag::Chan(k) => channel::loaded(self, k, v),
            Tag::Home(k) => community::loaded(self, k, v),
            Tag::Events(g, k) => events::loaded(self, g, k, &v),
            Tag::Joins(g, i) => applications::loaded(self, g, i, &v),
            Tag::JoinsFor(g, u) => applications::others(self, g, u, &v),
            Tag::JoinsAct(g) => applications::done(self, g, true, &v),
            Tag::EventUsers(g, e) => events::users_loaded(self, g, e, &v),
            Tag::Quests(k) => quests::loaded(self, k, &v),
            Tag::Follow => follow::done(self, true),
            Tag::Feed => {
                self.store.set_feed(&v);
                let mut want: Vec<Id> = self.store.feed.iter().map(|e| e.app).filter(|a| *a != 0 && !self.store.app_icons.contains_key(a)).collect();
                want.sort_unstable();
                want.dedup();
                want.truncate(50);
                if !want.is_empty() {
                    let q: Vec<String> = want.iter().map(|a| format!("application_ids={a}")).collect();
                    self.rest("GET", format!("/applications/public?{}", q.join("&")), None, Tag::AppIcons(want));
                }
            }
            Tag::AppIcons(asked) => {
                for a in asked {
                    self.store.app_icons.insert(a, None);
                    self.app_covers.insert(a, None);
                }
                for a in v.as_array().into_iter().flatten() {
                    self.store.app_icons.insert(crate::state::sid(&a["id"]), a["icon"].as_str().map(str::to_string));
                    self.app_covers.insert(crate::state::sid(&a["id"]), a["cover_image"].as_str().map(str::to_string));
                }
            }
            Tag::InvitePreview(code) => self.invite_card.got(&code, &v),
            Tag::ChatInvite(code) => self.chat_invites.got(&code, &v),
            Tag::TopEmojis(g) => {
                let mut items: Vec<(u64, Id)> = v["items"].as_array().into_iter().flatten().map(|i| (i["emoji_rank"].as_u64().unwrap_or(u64::MAX), crate::state::sid(&i["emoji_id"]))).collect();
                items.sort();
                self.top_emojis.insert(g, items.into_iter().map(|(_, id)| id).collect());
            }
            Tag::Source(s, id) => {
                self.sources.insert((s, id), v);
            }
            Tag::PollVotes(_, msg, ans) => {
                let users: Vec<Id> = v["users"].as_array().into_iter().flatten().map(|u| self.store.upsert_user(u)).collect();
                if let Some(w) = self.votes.as_mut().filter(|w| w.msg() == msg) {
                    w.got(ans, users);
                }
            }
            Tag::ChatJoin(code) => {
                self.chat_invites.joined(&code);
                let name = v["guild"]["name"].as_str().unwrap_or("the server").to_string();
                self.toast(format!("Joined {name}"));
            }
            Tag::Invite => {
                let name = v["guild"]["name"].as_str().unwrap_or("the server").to_string();
                self.toast(format!("Joined {name}"));
            }
            Tag::MadeInvite => {
                if let (Some(inv), Some(c)) = (&mut self.made_invite, v["code"].as_str()) {
                    inv.code = Some(c.to_string());
                }
            }
            Tag::InviteDm(link) => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.rest("POST", format!("/channels/{id}/messages"), Some(json!({ "content": link })), Tag::fail(SEND_FAILED));
            }
            Tag::CallDm => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.ui.guild = None;
                self.open(id);
                self.join_voice(id, true);
            }
            Tag::InviteTo(uid) => {
                if let Some(code) = v["code"].as_str() {
                    self.send_invite(uid, code);
                }
            }
            Tag::OpenDm(text) => {
                self.store.apply("CHANNEL_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.ui.guild = None;
                self.open(id);
                if !text.is_empty() {
                    self.send_text(id, text);
                }
            }
            Tag::Thread(first) => {
                self.store.apply("THREAD_CREATE", &v);
                let id = crate::state::sid(&v["id"]);
                self.open(id);
                if !first.trim().is_empty() {
                    self.drafts.insert(id, first);
                    self.send_message(id);
                }
            }
            Tag::Saved(k) => titlebar::saved(self, k, &v),
            Tag::Favs => favs::loaded(self, &v),
            Tag::Note(u) => {
                if let Some(n) = v["note"].as_str() {
                    self.notes.insert(u, n.to_string());
                }
            }
            Tag::Friend => self.toast("Friend request sent."),
            Tag::AddFriend(name) => self.add_friend_note = Some((true, format!("Success! Your friend request to **{name}** was sent."))),
            Tag::Commands(scope) => self.set_commands(scope, &v),
            Tag::Gifs(q) => self.set_gifs(q, &v),
            Tag::GifCats => self.set_gif_cats(&v),
            Tag::StickerPacks => self.set_packs(&v),
            Tag::Welcome(g) => {
                let on = v["enabled"].as_bool().unwrap_or(false);
                let actions = v["new_member_actions"].as_array().into_iter().flatten().map(|a| crate::state::sid(&a["channel_id"]));
                let e = self.guides.entry(g).or_default();
                e.0 = if on { actions.filter(|c| *c != 0).collect() } else { Vec::new() };
                self.guide_raw.insert(g, v);
            }
            Tag::MemberActions(g) => {
                let done = v["channel_actions"].as_object().into_iter().flatten().filter(|(_, a)| a["completed"].as_bool() == Some(true));
                self.guides.entry(g).or_default().1 = done.filter_map(|(c, _)| c.parse().ok()).collect();
            }
            Tag::Remote(k) => settings::on_remote(self, k, Ok(v)),
            Tag::Product(_) => self.product_loaded(&v),
            Tag::Keyed(k) => {
                self.keyed.insert(k, Some(Ok(v)));
            }
            Tag::Activity(k) => activity::on_rest(self, &k, v),
            Tag::AddApp(true) => addapp::info(self, &v),
            Tag::AddApp(false) => addapp::done(self, Ok(())),
            Tag::Quiet | Tag::Act(_) => {}
        }
    }

    /// Images and copied files on the clipboard become attachments.
    pub fn paste_files(&mut self) {
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        if let Ok(list) = cb.get().file_list()
            && !list.is_empty()
        {
            self.attach_files(list);
            return;
        }
        let Ok(img) = cb.get_image() else { return };
        let Some(buf) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else { return };
        let dir = std::env::temp_dir().join("rustcord");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("image-{}.png", chrono::Utc::now().timestamp_millis()));
        if buf.save(&path).is_ok() {
            let mut u = composer::Upload::new(path);
            u.name = "image.png".into();
            self.files.push(u);
            self.focus_composer = true;
        }
    }

    pub fn sign_in(&mut self, t: String) {
        cache::set_token(Some(&t));
        self.net.send(Cmd::Start(t.clone()));
        self.token = Some(t);
        self.login = Default::default();
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
        self.toast_detail = None;
    }

    pub fn error_toast(&mut self, e: String) {
        let detail = self.fail.take().map_or_else(|| e.clone(), |f| f.1);
        if e == crate::api::RATE_LIMITED || e.starts_with("Oh no, it looks like the Discord servers") {
            return self.toast(e);
        }
        if e.is_empty() || crate::api::offline(&e) {
            self.toast("Something went wrong. Shift Click this notification to see error details.");
        } else {
            self.toast(format!("{}\nShift Click this notification to see error details.", e.trim_end()));
        }
        self.toast_detail = Some(detail);
    }

    pub fn logout(&mut self) {
        if self.store.me != 0 {
            cache::forget_account(self.store.me);
            self.accounts = cache::accounts();
        }
        self.reset_session();
    }

    /// Drops the signed-in session and its cached data, back to the login screen.
    pub fn reset_session(&mut self) {
        self.voice.leave(&self.net);
        self.net.send(Cmd::Stop);
        crate::rpc::signed_out();
        self.links.clear();
        self.refresh_game();
        cache::set_token(None);
        self.token = None;
        self.store = Tracked::default();
        self.ui = UiState::default();
        self.nav = Default::default();
        self.inbox = None;
    }
}
