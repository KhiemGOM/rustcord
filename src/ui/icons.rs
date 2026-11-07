//! Icons. Discord's own SVGs (assets/icons, white so a tint colours them), rasterised once
//! per size; the hand-drawn vectors below cover the first frame before a texture exists
//! and the few glyphs Discord has no icon for (window buttons).
//! Coordinates are on a 24x24 grid and scale with the target rect.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::f32::consts::{FRAC_PI_4, PI, TAU};

use crate::theme::pal;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Hash,
    Speaker,
    Megaphone,
    Forum,
    Stage,
    Rules,
    Thread,
    People,
    Pin,
    Gear,
    Mic,
    Headphones,
    Plus,
    Smile,
    AddReaction,
    Search,
    Reply,
    Pencil,
    Trash,
    Close,
    Check,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Folder,
    File,
    Play,
    Image,
    Gem,
    ArrowRight,
    ArrowLeft,
    Bubble,
    Send,
    Inbox,
    Help,
    Minimize,
    Maximize,
    Restore,
    PersonAdd,
    Bell,
    BellOff,
    Gift,
    Gif,
    Sticker,
    Apps,
    More,
    Forward,
    MicOff,
    HeadphonesOff,
    Video,
    Screen,
    Friends,
    Nitro,
    Shop,
    Quests,
    Discover,
    Download,
    Verified,
    Discord,
    Events,
    ServerGuide,
    ChannelsRoles,
    NewDm,
    Media,
    Crown,
    HangUp,
    Phone,
    MoreVertical,
    CirclePlus,
    Copy,
    Link,
    MarkUnread,
    Id,
    Flag,
    Insights,
    FolderPlus,
    CalendarPlus,
    Shield,
    Leave,
    EyeOff,
    Lock,
    PopOut,
    Fullscreen,
    Hand,
    Waveform,
    Person,
    At,
    Clock,
    Attach,
    UserProfile,
    BadgeCommunity,
    BadgeCommunityBoosted,
    BadgePartner,
    Sort,
    Controller,
    Trophy,
    Streak,
    Fire,
    Retry,
    Launched,
    Marathon,
    NewRelease,
    NewPlayer,
    GemOutline,
    GemSlash,
    CircleCheck,
    Soundboard,
    Star,
    UserCircle,
    Picture,
    Upload,
    ScreenArrow,
    Location,
    NavBack,
    VideoSlash,
    Camera,
    BackgroundNone,
    BackgroundBlur,
    BackgroundCustom,
    FlowerStar,
    BoostGem,
    Bug,
    Refresh,
    NavForward,
    SideAccount,
    SidePrivacy,
    SideMessaging,
    SideBell,
    SideNitro,
    SideBoost,
    SideSubs,
    SideGift,
    SideBilling,
    SideVoice,
    SideAppearance,
    SideAccess,
    SideSystem,
    Bookmark,
    Bookmarked,
    ThreadPlus,
    Bold,
    Italic,
    Underline,
    Strike,
    Eye,
    Tag,
    ArrowUp,
    CheckAll,
    Jump,
    SideLanguage,
    SideActivity,
    SideApps,
    SideDev,
    SideLogout,
    Poll,
    Clips,
    Timer,
    FileUpload,
    Filter,
    LinkExternal,
    Pause,
    Volume,
    VolumeOff,
    Expand,
    ExpandExit,
    Code,
    Heart,
}

macro_rules! svg {
    ($name:literal) => {
        (concat!("bytes://icon/", $name, ".svg"), include_bytes!(concat!("../../assets/icons/", $name, ".svg")).as_slice())
    };
}

type Svg = (&'static str, &'static [u8]);

fn discord(icon: Icon) -> Option<Svg> {
    Some(match icon {
        Icon::Hash => svg!("hash"),
        Icon::Speaker => svg!("voice"),
        Icon::Megaphone => svg!("megaphone"),
        Icon::Forum => svg!("forum"),
        Icon::Stage => svg!("stage"),
        Icon::Rules => svg!("rules"),
        Icon::Thread => svg!("thread"),
        Icon::People => svg!("people"),
        Icon::Pin => svg!("pin"),
        Icon::Gear => svg!("gear"),
        Icon::Mic => svg!("mic"),
        Icon::Headphones => svg!("headphones"),
        Icon::Plus => svg!("plus"),
        Icon::Smile => svg!("smile"),
        Icon::AddReaction => svg!("add_reaction"),
        Icon::Search => svg!("search"),
        Icon::Reply => svg!("reply"),
        Icon::Pencil => svg!("pencil"),
        Icon::Trash => svg!("trash"),
        Icon::Close => svg!("close"),
        Icon::Check => svg!("check"),
        Icon::ChevronDown => svg!("chevron_down"),
        Icon::ChevronRight => svg!("chevron_right"),
        Icon::ChevronUp => svg!("chevron_up"),
        Icon::Folder => svg!("folder"),
        Icon::File => svg!("file"),
        Icon::Play => svg!("play"),
        Icon::Image => svg!("image"),
        Icon::Gem => svg!("boosts"),
        Icon::ArrowRight => svg!("arrow_right"),
        Icon::ArrowLeft => svg!("arrow_left"),
        Icon::Send => svg!("send"),
        Icon::Inbox => svg!("inbox"),
        Icon::Help => svg!("help"),
        Icon::PersonAdd => svg!("person_add"),
        Icon::Bell => svg!("bell"),
        Icon::BellOff => svg!("bell_off"),
        Icon::Gift => svg!("gift"),
        Icon::Heart => svg!("heart"),
        Icon::Gif => svg!("gif"),
        Icon::Sticker => svg!("sticker"),
        Icon::Apps => svg!("apps"),
        Icon::More => svg!("more"),
        Icon::Forward => svg!("forward"),
        Icon::MicOff => svg!("mic_off"),
        Icon::HeadphonesOff => svg!("headphones_off"),
        Icon::Video => svg!("video"),
        Icon::Screen => svg!("screen"),
        Icon::Friends => svg!("friends"),
        Icon::Nitro => svg!("nitro"),
        Icon::Shop => svg!("shop"),
        Icon::Quests => svg!("quests"),
        Icon::Discover => svg!("discover"),
        Icon::Download => svg!("download"),
        Icon::Verified => svg!("verified"),
        Icon::Discord => svg!("discord"),
        Icon::Events => svg!("events"),
        Icon::ServerGuide => svg!("server_guide"),
        Icon::ChannelsRoles => svg!("channels_roles"),
        Icon::NewDm => svg!("new_dm"),
        Icon::Media => svg!("media"),
        Icon::HangUp => svg!("hang_up"),
        Icon::Phone => svg!("phone"),
        Icon::Crown => svg!("crown"),
        Icon::MoreVertical => svg!("more_vertical"),
        Icon::CirclePlus => svg!("circle_plus"),
        Icon::Copy => svg!("copy"),
        Icon::Link => svg!("link"),
        Icon::MarkUnread => svg!("mark_unread"),
        Icon::Id => svg!("id"),
        Icon::Flag => svg!("flag"),
        Icon::Insights => svg!("insights"),
        Icon::FolderPlus => svg!("folder_plus"),
        Icon::CalendarPlus => svg!("calendar_plus"),
        Icon::Shield => svg!("shield"),
        Icon::Leave => svg!("leave"),
        Icon::EyeOff => svg!("eye_off"),
        Icon::Lock => svg!("lock"),
        Icon::Person => svg!("person"),
        Icon::At => svg!("at"),
        Icon::Clock => svg!("clock"),
        Icon::Attach => svg!("attach"),
        Icon::UserProfile => svg!("user_profile"),
        Icon::BadgeCommunity => svg!("badge_community"),
        Icon::BadgeCommunityBoosted => svg!("badge_community_boosted"),
        Icon::BadgePartner => svg!("badge_partner"),
        Icon::Sort => svg!("sort"),
        Icon::Controller => svg!("controller"),
        Icon::PopOut => svg!("popout"),
        Icon::Fullscreen => svg!("fullscreen"),
        Icon::Hand => svg!("hand"),
        Icon::Waveform => svg!("waveform"),
        Icon::Trophy => svg!("trophy"),
        Icon::Streak => svg!("streak"),
        Icon::Fire => svg!("fire"),
        Icon::Retry => svg!("retry"),
        Icon::Filter => svg!("filter"),
        Icon::LinkExternal => svg!("link_external"),
        Icon::Launched => svg!("launched"),
        Icon::Marathon => svg!("marathon"),
        Icon::NewRelease => svg!("new_release"),
        Icon::NewPlayer => svg!("new_player"),
        Icon::GemOutline => svg!("gem_outline"),
        Icon::GemSlash => svg!("gem_slash"),
        Icon::CircleCheck => svg!("circle_check"),
        Icon::Soundboard => svg!("soundboard"),
        Icon::Star => svg!("star"),
        Icon::UserCircle => svg!("user_circle"),
        Icon::Picture => svg!("picture"),
        Icon::Upload => svg!("upload"),
        Icon::ScreenArrow => svg!("screen_arrow"),
        Icon::Location => svg!("location"),
        Icon::NavBack => svg!("nav_back"),
        Icon::VideoSlash => svg!("video_slash"),
        Icon::Camera => svg!("camera"),
        Icon::BackgroundNone => svg!("background_none"),
        Icon::BackgroundBlur => svg!("background_blur"),
        Icon::BackgroundCustom => svg!("background_custom"),
        Icon::FlowerStar => svg!("flower_star"),
        Icon::BoostGem => svg!("boost_gem"),
        Icon::Bug => svg!("bug"),
        Icon::Refresh => svg!("refresh"),
        Icon::NavForward => svg!("nav_forward"),
        Icon::Bubble => svg!("bubble"),
        Icon::SideAccount => svg!("s_account"),
        Icon::SidePrivacy => svg!("s_privacy"),
        Icon::SideMessaging => svg!("s_messaging"),
        Icon::SideBell => svg!("s_bell"),
        Icon::SideNitro => svg!("s_nitro"),
        Icon::SideBoost => svg!("s_boost"),
        Icon::SideSubs => svg!("s_subs"),
        Icon::SideGift => svg!("s_gift"),
        Icon::SideBilling => svg!("s_billing"),
        Icon::SideVoice => svg!("s_voice"),
        Icon::SideAppearance => svg!("s_appearance"),
        Icon::SideAccess => svg!("s_access"),
        Icon::SideSystem => svg!("s_system"),
        Icon::Bookmark => svg!("bookmark_outline"),
        Icon::Bookmarked => svg!("bookmark"),
        Icon::ThreadPlus => svg!("thread_plus"),
        Icon::Bold => svg!("bold"),
        Icon::Italic => svg!("italic"),
        Icon::Underline => svg!("underline"),
        Icon::Strike => svg!("strike"),
        Icon::Eye => svg!("eye"),
        Icon::Tag => svg!("tag"),
        Icon::ArrowUp => svg!("arrow_up"),
        Icon::CheckAll => svg!("check_all"),
        Icon::Jump => svg!("jump"),
        Icon::SideLanguage => svg!("s_language"),
        Icon::SideActivity => svg!("s_activity"),
        Icon::SideApps => svg!("s_apps"),
        Icon::SideDev => svg!("s_dev"),
        Icon::SideLogout => svg!("s_logout"),
        Icon::Poll => svg!("poll"),
        Icon::Clips => svg!("clips"),
        Icon::Timer => svg!("timer"),
        Icon::FileUpload => svg!("file_upload"),
        Icon::Pause => svg!("pause"),
        Icon::Volume => svg!("volume"),
        Icon::VolumeOff => svg!("volume_off"),
        Icon::Expand => svg!("expand"),
        Icon::ExpandExit => svg!("expand_exit"),
        Icon::Code => svg!("code"),
        Icon::Minimize | Icon::Maximize | Icon::Restore => return None,
    })
}

/// Paint one of Discord's icons tinted `c`, square and centred in `rect`. False until the
/// texture for this size exists.
fn paint_svg(p: &Painter, (uri, bytes): Svg, rect: Rect, c: Color32) -> bool {
    let side = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), vec2(side, side));
    let src = egui::ImageSource::Bytes { uri: uri.into(), bytes: egui::load::Bytes::Static(bytes) };
    match egui::Image::new(src).load_for_size(p.ctx(), r.size()) {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            p.image(texture.id, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), c);
            true
        }
        _ => false,
    }
}

struct G<'a> {
    p: &'a Painter,
    o: Pos2,
    s: f32,
    c: Color32,
}

impl G<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.o + vec2(x, y) * self.s
    }
    fn stroke(&self, w: f32) -> Stroke {
        Stroke::new(w * self.s, self.c)
    }
    fn line(&self, pts: &[(f32, f32)], w: f32) {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::line(pts, self.stroke(w)));
    }
    fn fill(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::convex_polygon(pts, self.c, Stroke::NONE));
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.pt(x, y), r * self.s, self.c);
    }
    fn ring(&self, x: f32, y: f32, r: f32, w: f32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s, self.stroke(w));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, round: f32) {
        let r = Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1));
        self.p.rect_filled(r, round * self.s, self.c);
    }
    fn rect_line(&self, x0: f32, y0: f32, x1: f32, y1: f32, round: f32, w: f32) {
        let r = Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1));
        self.p.rect_stroke(r, round * self.s, self.stroke(w), egui::StrokeKind::Middle);
    }
    // angles in radians, y pointing down like the screen
    fn arc(&self, x: f32, y: f32, r: f32, a0: f32, a1: f32, w: f32) {
        let n = 16;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.pt(x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke(w)));
    }
    fn half_disc(&self, x: f32, y: f32, r: f32) {
        // the top half of a disc, for shoulders
        let n = 16;
        let mut pts = vec![self.pt(x + r, y)];
        for i in 0..=n {
            let a = -(i as f32) * PI / n as f32;
            pts.push(self.pt(x + r * a.cos(), y + r * a.sin()));
        }
        self.p.add(Shape::convex_polygon(pts, self.c, Stroke::NONE));
    }
}

fn rotate(pts: &[(f32, f32)], a: f32) -> Vec<(f32, f32)> {
    let (s, c) = a.sin_cos();
    pts.iter()
        .map(|(x, y)| {
            let (dx, dy) = (x - 12.0, y - 12.0);
            (12.0 + dx * c - dy * s, 12.0 + dx * s + dy * c)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mark {
    Lock,
    Warn,
}

/// A channel glyph with Discord's lock or warning variant; the cut-corner drawing below is
/// the fallback.
pub fn draw_marked(p: &Painter, icon: Icon, mark: Mark, rect: Rect, c: Color32) {
    let named = match (icon, mark) {
        (Icon::Hash, Mark::Lock) => Some(svg!("hash_lock")),
        (Icon::Hash, Mark::Warn) => Some(svg!("hash_nsfw")),
        (Icon::Speaker, Mark::Lock) => Some(svg!("voice_lock")),
        (Icon::Speaker, Mark::Warn) => Some(svg!("voice_nsfw")),
        (Icon::Megaphone, Mark::Lock) => Some(svg!("megaphone_lock")),
        (Icon::Megaphone, Mark::Warn) => Some(svg!("megaphone_nsfw")),
        (Icon::Forum, Mark::Lock) => Some(svg!("forum_lock")),
        (Icon::Forum, Mark::Warn) => Some(svg!("forum_nsfw")),
        (Icon::Stage, Mark::Lock) => Some(svg!("stage_lock")),
        _ => None,
    };
    if let Some(s) = named
        && paint_svg(p, s, rect, c)
    {
        return;
    }
    let side = rect.width().min(rect.height());
    let o = rect.center() - vec2(side, side) / 2.0;
    let k = side / 24.0;
    for clip in [
        Rect::from_min_size(o, vec2(13.0 * k, side)),
        Rect::from_min_max(o + vec2(0.0, 11.5 * k), o + vec2(side, side)),
    ] {
        draw(&p.with_clip_rect(clip.intersect(p.clip_rect())), icon, rect, c);
    }
    let g = G { p, o, s: k, c };
    match mark {
        Mark::Lock => {
            g.rect(14.0, 5.5, 23.0, 11.5, 1.5);
            g.arc(18.5, 5.5, 2.6, PI, TAU, 1.8);
        }
        Mark::Warn => {
            g.fill(&[(18.5, 0.5), (23.5, 10.5), (13.5, 10.5)]);
        }
    }
}

/// `draw`, turned by `angle` radians (clockwise) about its centre.
pub fn draw_turned(p: &Painter, icon: Icon, rect: Rect, c: Color32, angle: f32) {
    if angle != 0.0
        && let Some((uri, bytes)) = discord(icon)
    {
        let side = rect.width().min(rect.height());
        let r = Rect::from_center_size(rect.center(), vec2(side, side));
        let src = egui::ImageSource::Bytes { uri: uri.into(), bytes: egui::load::Bytes::Static(bytes) };
        if let Ok(egui::load::TexturePoll::Ready { texture }) = egui::Image::new(src).load_for_size(p.ctx(), r.size()) {
            let mut m = egui::Mesh::with_texture(texture.id);
            m.add_rect_with_uv(r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), c);
            m.rotate(egui::emath::Rot2::from_angle(angle), r.center());
            p.add(Shape::mesh(m));
            return;
        }
    }
    draw(p, icon, rect, c);
}

pub fn draw(p: &Painter, icon: Icon, rect: Rect, c: Color32) {
    if let Some(s) = discord(icon)
        && paint_svg(p, s, rect, c)
    {
        return;
    }
    let side = rect.width().min(rect.height());
    let g = G { p, o: rect.center() - vec2(side, side) / 2.0, s: side / 24.0, c };
    match icon {
        Icon::Hash => {
            g.line(&[(10.0, 3.5), (8.0, 20.5)], 2.0);
            g.line(&[(16.5, 3.5), (14.5, 20.5)], 2.0);
            g.line(&[(4.5, 8.5), (20.0, 8.5)], 2.0);
            g.line(&[(3.8, 15.5), (19.3, 15.5)], 2.0);
        }
        Icon::Speaker => {
            g.fill(&[(2.0, 9.0), (6.0, 9.0), (11.0, 4.5), (11.0, 19.5), (6.0, 15.0), (2.0, 15.0)]);
            g.arc(11.0, 12.0, 5.0, -FRAC_PI_4, FRAC_PI_4, 2.0);
            g.arc(11.0, 12.0, 9.0, -FRAC_PI_4, FRAC_PI_4, 2.0);
        }
        Icon::Megaphone => {
            g.fill(&[(6.0, 9.0), (18.0, 4.0), (18.0, 20.0), (6.0, 15.0)]);
            g.rect(3.0, 9.0, 7.0, 15.0, 1.0);
            g.line(&[(8.0, 15.0), (9.5, 20.5)], 2.5);
            g.line(&[(20.5, 10.0), (20.5, 14.0)], 2.0);
        }
        Icon::Forum => {
            g.rect_line(3.0, 4.0, 16.0, 13.5, 3.0, 2.0);
            g.fill(&[(5.0, 13.0), (8.5, 13.0), (5.0, 16.5)]);
            g.line(&[(18.0, 8.5), (21.0, 8.5), (21.0, 18.0), (19.0, 18.0), (19.0, 20.5), (16.0, 18.0), (9.0, 18.0), (9.0, 15.5)], 2.0);
        }
        Icon::Stage => {
            g.circle(12.0, 12.0, 2.5);
            g.arc(12.0, 12.0, 5.5, -PI * 0.75, -PI * 0.25, 2.0);
            g.arc(12.0, 12.0, 5.5, PI * 0.25, PI * 0.75, 2.0);
            g.arc(12.0, 12.0, 9.0, -PI * 0.75, -PI * 0.25, 2.0);
            g.arc(12.0, 12.0, 9.0, PI * 0.25, PI * 0.75, 2.0);
        }
        Icon::Rules => {
            g.rect_line(4.5, 3.5, 19.5, 20.5, 2.5, 2.0);
            g.line(&[(8.5, 12.5), (11.0, 15.0), (15.5, 9.5)], 2.0);
        }
        Icon::Thread => {
            g.line(&[(8.5, 3.0), (7.0, 15.0)], 2.0);
            g.line(&[(13.5, 3.0), (12.0, 10.0)], 2.0);
            g.line(&[(3.5, 7.0), (16.0, 7.0)], 2.0);
            g.line(&[(3.0, 12.0), (10.0, 12.0)], 2.0);
            g.rect(12.0, 12.0, 21.0, 19.0, 3.0);
            g.fill(&[(14.0, 18.5), (17.0, 18.5), (14.0, 21.5)]);
        }
        Icon::People => {
            g.circle(9.0, 8.0, 3.5);
            g.half_disc(9.0, 19.5, 6.0);
            g.circle(16.5, 8.5, 3.0);
            g.fill(&[(15.5, 13.5), (17.5, 13.5), (21.5, 16.5), (21.5, 19.5), (16.0, 19.5)]);
        }
        Icon::Pin => {
            let shape = [
                (9.0, 3.0), (15.0, 3.0), (15.0, 5.0), (14.0, 5.0), (15.5, 12.0), (18.0, 13.5),
                (18.0, 15.0), (6.0, 15.0), (6.0, 13.5), (8.5, 12.0), (10.0, 5.0), (9.0, 5.0),
            ];
            let r = rotate(&shape, FRAC_PI_4);
            // the outline isn't convex, so fill it in two convex halves
            g.fill(&[r[0], r[1], r[2], r[3], r[10], r[11]]);
            g.fill(&[r[3], r[4], r[5], r[6], r[7], r[8], r[9], r[10]]);
            let needle = rotate(&[(12.0, 15.0), (12.0, 21.5)], FRAC_PI_4);
            g.line(&needle, 2.0);
        }
        Icon::Gear => {
            g.ring(12.0, 12.0, 5.0, 3.0);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                g.line(&[(12.0 + 6.0 * a.cos(), 12.0 + 6.0 * a.sin()), (12.0 + 9.0 * a.cos(), 12.0 + 9.0 * a.sin())], 3.2);
            }
        }
        Icon::Mic => {
            g.rect(9.0, 2.5, 15.0, 14.0, 3.0);
            g.arc(12.0, 11.0, 6.0, 0.0, PI, 2.0);
            g.line(&[(12.0, 17.0), (12.0, 21.0)], 2.0);
            g.line(&[(8.5, 21.0), (15.5, 21.0)], 2.0);
        }
        Icon::Headphones => {
            g.arc(12.0, 13.0, 8.0, PI, TAU, 2.0);
            g.rect(3.0, 13.0, 8.0, 20.5, 2.0);
            g.rect(16.0, 13.0, 21.0, 20.5, 2.0);
        }
        Icon::PersonAdd => {
            g.circle(10.0, 8.0, 4.0);
            g.half_disc(10.0, 20.5, 7.0);
            g.line(&[(19.0, 4.0), (19.0, 12.0)], 2.0);
            g.line(&[(15.0, 8.0), (23.0, 8.0)], 2.0);
        }
        Icon::Plus => {
            g.line(&[(12.0, 5.0), (12.0, 19.0)], 2.5);
            g.line(&[(5.0, 12.0), (19.0, 12.0)], 2.5);
        }
        Icon::Smile | Icon::AddReaction => {
            let (cx, cy, r) = if icon == Icon::AddReaction { (11.0, 13.0, 8.0) } else { (12.0, 12.0, 9.0) };
            g.ring(cx, cy, r, 2.0);
            g.circle(cx - r * 0.35, cy - r * 0.2, 1.4);
            g.circle(cx + r * 0.35, cy - r * 0.2, 1.4);
            g.arc(cx, cy, r * 0.5, PI * 0.15, PI * 0.85, 2.0);
            if icon == Icon::AddReaction {
                g.line(&[(19.5, 1.5), (19.5, 8.5)], 2.0);
                g.line(&[(16.0, 5.0), (23.0, 5.0)], 2.0);
            }
        }
        Icon::Search => {
            g.ring(10.5, 10.5, 6.0, 2.0);
            g.line(&[(15.0, 15.0), (20.5, 20.5)], 2.5);
        }
        Icon::Reply => {
            g.line(&[(9.5, 4.5), (4.0, 10.0), (9.5, 15.5)], 2.0);
            let mut pts = vec![(4.0, 10.0), (14.0, 10.0)];
            for i in 1..=8 {
                let a = -PI / 2.0 + i as f32 * (PI / 2.0) / 8.0;
                pts.push((14.0 + 6.0 * a.cos(), 16.0 + 6.0 * a.sin()));
            }
            pts.push((20.0, 20.0));
            g.line(&pts, 2.0);
        }
        Icon::Pencil => {
            g.line(&[(7.0, 17.0), (17.0, 7.0)], 3.5);
            g.line(&[(18.0, 6.0), (19.0, 5.0)], 3.5);
            g.fill(&[(3.5, 20.5), (5.0, 15.5), (8.5, 19.0)]);
        }
        Icon::Trash => {
            g.line(&[(4.5, 6.0), (19.5, 6.0)], 2.0);
            g.line(&[(9.5, 6.0), (9.5, 3.5), (14.5, 3.5), (14.5, 6.0)], 2.0);
            g.line(&[(6.5, 6.0), (7.5, 20.5), (16.5, 20.5), (17.5, 6.0)], 2.0);
            g.line(&[(10.0, 10.0), (10.0, 17.0)], 1.8);
            g.line(&[(14.0, 10.0), (14.0, 17.0)], 1.8);
        }
        Icon::Close => {
            g.line(&[(6.0, 6.0), (18.0, 18.0)], 2.0);
            g.line(&[(18.0, 6.0), (6.0, 18.0)], 2.0);
        }
        Icon::Check => g.line(&[(5.0, 12.5), (10.0, 17.5), (19.0, 7.0)], 2.5),
        Icon::ChevronDown => g.line(&[(7.0, 9.5), (12.0, 14.5), (17.0, 9.5)], 2.0),
        Icon::ChevronRight => g.line(&[(9.5, 7.0), (14.5, 12.0), (9.5, 17.0)], 2.0),
        Icon::Folder => {
            g.fill(&[(3.0, 5.5), (9.0, 5.5), (11.0, 7.5), (11.0, 9.0), (3.0, 9.0)]);
            g.rect(3.0, 8.0, 21.0, 19.0, 2.0);
        }
        Icon::File => {
            g.line(&[(14.0, 2.5), (5.0, 2.5), (5.0, 21.5), (19.0, 21.5), (19.0, 7.5), (14.0, 2.5), (14.0, 7.5), (19.0, 7.5)], 1.8);
        }
        Icon::Play => g.fill(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]),
        Icon::Image => {
            g.rect_line(3.0, 4.5, 21.0, 19.5, 2.0, 2.0);
            g.fill(&[(5.5, 17.5), (10.0, 11.0), (13.5, 15.5)]);
            g.fill(&[(11.5, 17.5), (15.5, 12.5), (19.0, 17.5)]);
            g.circle(16.0, 8.5, 1.6);
        }
        Icon::Gem => {
            g.fill(&[(6.0, 4.0), (18.0, 4.0), (22.0, 9.0), (12.0, 21.0), (2.0, 9.0)]);
        }
        Icon::ArrowRight => {
            g.line(&[(4.0, 12.0), (19.0, 12.0)], 2.0);
            g.line(&[(13.0, 6.0), (19.0, 12.0), (13.0, 18.0)], 2.0);
        }
        Icon::ArrowLeft => {
            g.line(&[(20.0, 12.0), (5.0, 12.0)], 2.0);
            g.line(&[(11.0, 6.0), (5.0, 12.0), (11.0, 18.0)], 2.0);
        }
        Icon::Send => {
            g.fill(&[(3.0, 4.0), (21.0, 12.0), (3.0, 20.0), (6.0, 12.0)]);
        }
        Icon::Inbox => {
            g.rect(3.0, 3.0, 21.0, 5.0, 1.0);
            g.rect(3.0, 3.0, 5.0, 21.0, 1.0);
            g.rect(19.0, 3.0, 21.0, 21.0, 1.0);
            g.rect(3.0, 15.0, 9.0, 21.0, 1.0);
            g.rect(15.0, 15.0, 21.0, 21.0, 1.0);
            // the tray dips into the bottom edge, so fill what's left under the curve
            let dip = |i: usize| {
                let a = PI * i as f32 / 8.0;
                (12.0 - 3.0 * a.cos(), 15.0 + 3.0 * a.sin())
            };
            for i in 0..8 {
                let (a, b) = (dip(i), dip(i + 1));
                g.fill(&[a, b, (b.0, 21.0), (a.0, 21.0)]);
            }
        }
        Icon::Help => {
            // the question mark is a hole in the disc, which one colour can't do; this only ever sits on the title bar
            g.circle(12.0, 12.0, 10.0);
            let h = G { c: pal().frame_bg, ..g };
            h.arc(12.0, 9.5, 3.2, -PI, FRAC_PI_4, 2.4);
            h.line(&[(14.3, 11.8), (12.0, 14.0), (12.0, 15.5)], 2.4);
            h.circle(12.0, 18.4, 1.3);
        }
        Icon::Minimize => g.line(&[(5.0, 12.0), (19.0, 12.0)], 1.8),
        Icon::Maximize => g.rect_line(5.5, 5.5, 18.5, 18.5, 0.0, 1.8),
        Icon::Restore => {
            g.rect_line(4.5, 8.5, 15.5, 19.5, 0.0, 1.8);
            g.line(&[(8.5, 8.0), (8.5, 4.5), (19.5, 4.5), (19.5, 15.5), (16.0, 15.5)], 1.8);
        }
        Icon::Bubble => {
            g.rect(3.0, 4.0, 21.0, 17.0, 4.0);
            g.fill(&[(6.0, 16.0), (11.0, 16.0), (6.0, 21.0)]);
        }
        _ => {}
    }
}
