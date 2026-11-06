//! Discord's standardSidebarView (all.css, `.standardSidebarView__23e6b`): the sidebar region and
//! the content region split what the view has left over, and the page's column sits at the start
//! of the content region, capped by its kind, with the tools (the ESC button) right after it.

/// `--custom-standard-sidebar-view-sidebar-total-width`
pub const SIDEBAR: f32 = 264.0;
/// `.contentRegion` flex-basis
const REGION: f32 = 800.0;
/// `.contentColumn` max-width
const MAX_COLUMN: f32 = 740.0;
const MIN_COLUMN: f32 = 460.0;
/// `.noticeRegion` padding-inline, which is also how far it stands off the bottom
pub const NOTICE_PAD: f32 = 20.0;
/// `.toolsContainer`: its 36px, the 21px margin and the scroller's 14px scrollbar
const TOOLS: f32 = 36.0 + 21.0 + 14.0;
/// `.contentColumnDefault` padding-inline
const PAD: f32 = 40.0;
/// `.contentColumnWide` padding-inline-end
const PAD_WIDE_END: f32 = 10.0;

#[derive(Clone, Copy)]
pub enum Kind {
    // padding 40 on both sides, 740 at most
    Default,
    // 40 in front and 10 behind, as wide as the region allows
    Fill,
    // 40 in front and 10 behind, 740 at most (user settings)
    Wide,
}

pub struct Column {
    // where the content region starts
    pub region: f32,
    // the page's content, between the column's paddings
    pub left: f32,
    pub right: f32,
    // where the tools column begins
    pub tools: f32,
    // the unsaved-changes bar (`.noticeRegion`): left edge and width
    pub notice: (f32, f32),
}

/// The sidebar region is as wide as the content region leaves it: both grow from their bases,
/// only the content region shrinks.
pub fn region_left(width: f32, sidebar: f32) -> f32 {
    sidebar + ((width - sidebar - REGION) / 2.0).max(0.0)
}

/// The column for a view `width` wide whose x runs from `origin`. `tools` is false where the
/// close button lives elsewhere (the user settings header).
pub fn column(origin: f32, width: f32, sidebar: f32, kind: Kind, tools: bool) -> Column {
    let region = region_left(width, sidebar);
    let room = width - region - if tools { TOOLS } else { 0.0 };
    let w = match kind {
        Kind::Fill => room,
        _ => room.min(MAX_COLUMN),
    }
    .max(MIN_COLUMN);
    let end = match kind {
        Kind::Default => PAD,
        _ => PAD_WIDE_END,
    };
    let edge = origin + width;
    let notice = ((width - region - TOOLS).min(MAX_COLUMN) - 2.0 * NOTICE_PAD).max(0.0);
    Column {
        region: origin + region,
        left: origin + region + PAD,
        right: (origin + region + w - end).min(edge),
        tools: (origin + region + w).min(edge),
        notice: (origin + region + NOTICE_PAD, notice),
    }
}
