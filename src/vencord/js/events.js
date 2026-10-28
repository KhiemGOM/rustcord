// @api/MessageEvents: what plugins do to a message on its way out, and to clicks on one.
export const sendListeners = new Set();
export const editListeners = new Set();
export const clickListeners = new Set();

export const addMessagePreSendListener = l => (sendListeners.add(l), l);
export const removeMessagePreSendListener = l => sendListeners.delete(l);
export const addMessagePreEditListener = l => (editListeners.add(l), l);
export const removeMessagePreEditListener = l => editListeners.delete(l);
export const addMessageClickListener = l => (clickListeners.add(l), l);
export const removeMessageClickListener = l => clickListeners.delete(l);

export async function _handlePreSend(channelId, messageObj, options, props) {
    for (const l of sendListeners) {
        try {
            if ((await l(channelId, messageObj, options, props))?.cancel) return true;
        } catch (e) {
            console.error("a send listener threw:", e);
        }
    }
    return false;
}

export async function _handlePreEdit(channelId, messageId, messageObj) {
    for (const l of editListeners) {
        try {
            if ((await l(channelId, messageId, messageObj))?.cancel) return true;
        } catch (e) {
            console.error("an edit listener threw:", e);
        }
    }
    return false;
}

export function _handleClick(message, channel, event) {
    for (const l of clickListeners) {
        try { l(message, channel, event); } catch (e) { console.error("a click listener threw:", e); }
    }
}
