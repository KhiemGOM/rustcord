use super::*;

#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Message {
    pub id: Id,
    pub channel: Id,
    pub author: Id,
    pub content: String,
    pub kind: u8,
    pub edited: bool,
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    pub reactions: Vec<Reaction>,
    pub stickers: Vec<(Id, String, u8)>,
    #[serde(default)]
    pub components: Vec<Vec<Comp>>,
    #[serde(default)]
    pub app_id: Id,
    #[serde(default)]
    pub flags: u64,
    pub reply: Option<Id>,
    pub mentions: Vec<Id>,
    pub mention_roles: Vec<Id>,
    pub mention_everyone: bool,
    pub nonce: Option<String>,
    #[serde(default)]
    pub thread: Option<Id>,
    #[serde(default)]
    pub poll: Option<Poll>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub call: Option<Call>,
    #[serde(default)]
    pub refer: Option<(Id, Id)>,
    // a forward: when the original was sent, unix ms; its content is what the message shows
    #[serde(default)]
    pub forward: Option<i64>,
    #[serde(default)]
    pub interaction: Option<Interaction>,
    #[serde(skip)]
    pub busy: Option<Busy>,
    #[serde(default)]
    pub extra: Value,
    #[serde(skip)]
    pub pending: bool,
    #[serde(skip)]
    pub failed: bool,
}

impl Message {
    pub fn ts(&self) -> i64 {
        snowflake_ms(self.id)
    }

    pub(super) fn fill(&mut self, v: &Value) {
        if let Some(c) = v["content"].as_str() {
            self.content = c.to_string();
        }
        if !v["edited_timestamp"].is_null() && v.get("edited_timestamp").is_some() {
            self.edited = true;
        }
        if let Some(a) = v["attachments"].as_array() {
            self.attachments = a
                .iter()
                .map(|a| Attachment {
                    url: st(&a["url"]),
                    proxy: ost(&a["proxy_url"]).unwrap_or_else(|| st(&a["url"])),
                    filename: st(&a["filename"]),
                    size: a["size"].as_u64().unwrap_or(0),
                    w: a["width"].as_u64().unwrap_or(0) as u32,
                    h: a["height"].as_u64().unwrap_or(0) as u32,
                    ctype: st(&a["content_type"]),
                    duration: a["duration_secs"].as_f64().unwrap_or(0.0) as f32,
                    waveform: st(&a["waveform"]),
                    description: st(&a["description"]),
                })
                .collect();
        }
        if let Some(e) = v["embeds"].as_array() {
            self.embeds = e.iter().map(Embed::parse).collect();
        }
        if let Some(r) = v["reactions"].as_array() {
            self.reactions = r
                .iter()
                .map(|r| Reaction {
                    emoji: ReactEmoji::parse(&r["emoji"]),
                    count: r["count"].as_u64().unwrap_or(1) as u32,
                    me: r["me"].as_bool().unwrap_or(false),
                    burst: r["count_details"]["burst"].as_u64().unwrap_or(0) as u32,
                    me_burst: r["me_burst"].as_bool().unwrap_or(false),
                })
                .collect();
        }
        if let Some(s) = v["sticker_items"].as_array() {
            self.stickers = s
                .iter()
                .map(|s| (sid(&s["id"]), st(&s["name"]), s["format_type"].as_u64().unwrap_or(1) as u8))
                .collect();
        }
        if let Some(m) = v["mentions"].as_array() {
            self.mentions = m.iter().map(|u| sid(&u["id"])).collect();
        }
        if v.get("mention_roles").is_some() {
            self.mention_roles = ids(&v["mention_roles"]);
        }
        if let Some(b) = v["mention_everyone"].as_bool() {
            self.mention_everyone = b;
        }
        if v["call"].is_object() {
            self.call = Some(Call {
                with: ids(&v["call"]["participants"]),
                ended: v["call"]["ended_timestamp"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.timestamp_millis()),
            });
        }
        if let Some(b) = v["pinned"].as_bool() {
            self.pinned = b;
        }
        if v["thread"].is_object() {
            self.thread = Some(sid(&v["thread"]["id"]));
        }
        if v["poll"].is_object() {
            self.poll = Poll::parse(&v["poll"]);
        }
        if let Some(f) = v["flags"].as_u64() {
            self.flags = f;
        }
        if let Some(rows) = v["components"].as_array() {
            if self.flags & (1 << 15) != 0 {
                // components v2: the top-level array is the layout tree, each node its own row
                self.components = rows.iter().filter_map(Comp::parse_v2).map(|c| vec![c]).collect();
            } else {
                self.components = rows
                    .iter()
                    .filter(|r| r["type"].as_u64() == Some(1))
                    .map(|r| r["components"].as_array().into_iter().flatten().filter_map(Comp::parse).collect())
                    .collect();
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum Comp {
    Button { style: u8, label: String, emoji: Option<ReactEmoji>, custom_id: String, url: Option<String>, disabled: bool, #[serde(default)] sku: Option<String> },
    Select {
        kind: u8,
        custom_id: String,
        placeholder: String,
        options: Vec<(String, String, Option<String>)>,
        disabled: bool,
        #[serde(default)]
        min: u32,
        #[serde(default)]
        max: u32,
        #[serde(default)]
        picked: Vec<String>,
        #[serde(default)]
        types: Vec<u8>,
    },
    // components v2 (IS_COMPONENTS_V2): a layout tree instead of plain action rows
    Row(Vec<Comp>),
    Text(String),
    Gallery(Vec<(String, u32, u32)>),
    Thumbnail(String),
    FileCard { name: String, size: u64, url: String },
    Separator { divider: bool },
    Section { children: Vec<Comp>, accessory: Option<Box<Comp>> },
    Container { accent: Option<u32>, children: Vec<Comp> },
}

impl Comp {
    pub(super) fn parse(v: &Value) -> Option<Comp> {
        let disabled = v["disabled"].as_bool().unwrap_or(false);
        match v["type"].as_u64()? {
            2 => Some(Comp::Button {
                style: v["style"].as_u64().unwrap_or(2) as u8,
                label: st(&v["label"]),
                emoji: v["emoji"].is_object().then(|| ReactEmoji::parse(&v["emoji"])),
                custom_id: st(&v["custom_id"]),
                url: ost(&v["url"]),
                disabled,
                sku: ost(&v["sku_id"]),
            }),
            k @ (3 | 5..=8) => Some(Comp::Select {
                kind: k as u8,
                custom_id: st(&v["custom_id"]),
                placeholder: ost(&v["placeholder"]).unwrap_or_else(|| "Make a selection".into()),
                options: v["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|o| (st(&o["label"]), st(&o["value"]), ost(&o["description"])))
                    .collect(),
                disabled,
                min: v["min_values"].as_u64().unwrap_or(1) as u32,
                max: v["max_values"].as_u64().unwrap_or(1) as u32,
                picked: v["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|o| o["default"].as_bool() == Some(true))
                    .map(|o| st(&o["value"]))
                    .chain(v["default_values"].as_array().into_iter().flatten().map(|o| st(&o["id"])))
                    .collect(),
                types: v["channel_types"].as_array().into_iter().flatten().filter_map(|t| t.as_u64().map(|t| t as u8)).collect(),
            }),
            _ => None,
        }
    }

    /// The components-v2 layout tree. Each node carries its own children, so a container or
    /// section nests the way discord lays it out.
    pub(super) fn parse_v2(v: &Value) -> Option<Comp> {
        let kids = |v: &Value| v["components"].as_array().into_iter().flatten().filter_map(Comp::parse_v2).collect();
        match v["type"].as_u64()? {
            1 => Some(Comp::Row(v["components"].as_array().into_iter().flatten().filter_map(Comp::parse).collect())),
            2 | 3 | 5..=8 => Comp::parse(v),
            9 => Some(Comp::Section {
                children: kids(v),
                accessory: v["accessory"].is_object().then(|| Comp::parse_v2(&v["accessory"]).map(Box::new)).flatten(),
            }),
            10 => Some(Comp::Text(st(&v["content"]))),
            11 => Some(Comp::Thumbnail(media_src(&v["media"]))),
            12 => Some(Comp::Gallery(
                v["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|it| {
                        let m = &it["media"];
                        (media_src(m), m["width"].as_u64().unwrap_or(0) as u32, m["height"].as_u64().unwrap_or(0) as u32)
                    })
                    .filter(|(u, _, _)| !u.is_empty())
                    .collect(),
            )),
            13 => Some(Comp::FileCard {
                name: media_src(&v["file"]).rsplit('/').next().unwrap_or("file").split('?').next().unwrap_or("file").to_string(),
                size: v["size"].as_u64().unwrap_or(0),
                url: media_src(&v["file"]),
            }),
            14 => Some(Comp::Separator { divider: v["divider"].as_bool().unwrap_or(true) }),
            17 => Some(Comp::Container { accent: v["accent_color"].as_u64().map(|c| c as u32), children: kids(v) }),
            _ => None,
        }
    }
}

/// A v2 media node's real url: the proxied one discord serves, else the raw url.
pub(super) fn media_src(v: &Value) -> String {
    ost(&v["proxy_url"]).or_else(|| ost(&v["url"])).unwrap_or_default()
}

/// A running activity in a call: who has it open, by the id the server gave the launch.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Instance {
    pub app: Id,
    pub id: String,
    pub users: Vec<Id>,
}

#[derive(Clone)]
pub(crate) struct Sound {
    pub id: Id,
    pub guild: Id,
    pub name: String,
    pub emoji: Option<ReactEmoji>,
}

impl Sound {
    pub fn parse(v: &Value) -> Sound {
        let emoji = if v["emoji_id"].as_str().is_some() || v["emoji_name"].as_str().is_some() {
            Some(ReactEmoji {
                id: sid(&v["emoji_id"]),
                name: st(&v["emoji_name"]),
                animated: false,
            })
        } else {
            None
        };
        Sound { id: sid(&v["sound_id"]), guild: sid(&v["guild_id"]), name: st(&v["name"]), emoji }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Poll {
    pub question: String,
    pub answers: Vec<PollAnswer>,
    pub multi: bool,
    // unix ms the poll closes, 0 if unknown
    pub expiry: i64,
    pub finalized: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PollAnswer {
    pub id: u8,
    pub text: String,
    pub emoji: Option<ReactEmoji>,
    pub count: u32,
    pub me: bool,
}

impl Poll {
    pub(super) fn parse(v: &Value) -> Option<Poll> {
        let answers = v["answers"].as_array()?;
        let mut answers: Vec<PollAnswer> = answers
            .iter()
            .map(|a| {
                let media = &a["poll_media"];
                PollAnswer {
                    id: a["answer_id"].as_u64().unwrap_or(0) as u8,
                    text: st(&media["text"]),
                    emoji: media["emoji"].is_object().then(|| ReactEmoji::parse(&media["emoji"])),
                    count: 0,
                    me: false,
                }
            })
            .collect();
        for c in v["results"]["answer_counts"].as_array().into_iter().flatten() {
            let id = c["id"].as_u64().unwrap_or(0) as u8;
            if let Some(a) = answers.iter_mut().find(|a| a.id == id) {
                a.count = c["count"].as_u64().unwrap_or(0) as u32;
                a.me = c["me_voted"].as_bool().unwrap_or(false);
            }
        }
        Some(Poll {
            question: st(&v["question"]["text"]),
            answers,
            multi: v["allow_multiselect"].as_bool().unwrap_or(false),
            expiry: iso_ms(&v["expiry"]),
            finalized: v["results"]["is_finalized"].as_bool().unwrap_or(false),
        })
    }

    pub fn total(&self) -> u32 {
        self.answers.iter().map(|a| a.count).sum()
    }
}
