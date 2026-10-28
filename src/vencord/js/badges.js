// @api/Badges: profile badges plugins add. Kept so plugins load; Fastcord's profiles don't draw them yet.
export const BadgePosition = { START: 0, END: 1 };
const badges = new Set();
export const addProfileBadge = b => badges.add(b);
export const removeProfileBadge = b => badges.delete(b);
export const _getBadges = () => [];
