// @api/MessagePopover: buttons on the bar over a hovered message.
export const MessagePopoverButtonMap = new Map();
export const addMessagePopoverButton = (id, render, icon) => MessagePopoverButtonMap.set(id, { render, icon });
export const removeMessagePopoverButton = id => MessagePopoverButtonMap.delete(id);
