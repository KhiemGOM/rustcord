// @api/Badges: profile badges plugins add, drawn in Rustcord's profile cards.
export const BadgePosition = { START: 0, END: 1 };
export const Badges = globalThis.__fcBadges ??= new Set();
export const addProfileBadge = b => Badges.add(b);
export const removeProfileBadge = b => Badges.delete(b);
export function _getBadges(args) {
    const out = [];
    for (const b of Badges) {
        try {
            if (b.shouldShow && !b.shouldShow(args)) continue;
            const list = b.getBadges ? b.getBadges(args).map(x => ({ ...args, ...x })) : [{ ...args, ...b }];
            b.position === BadgePosition.START ? out.unshift(...list) : out.push(...list);
        } catch (e) {
            console.error("a profile badge threw:", e);
        }
    }
    return out;
}
