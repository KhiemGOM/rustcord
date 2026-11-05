// @api/ChatButtons: buttons beside Rustcord's composer.
export const ChatBarButtonMap = new Map();
export const addChatBarButton = (id, render, icon) => ChatBarButtonMap.set(id, { render, icon });
export const removeChatBarButton = id => ChatBarButtonMap.delete(id);

export const ChatBarButton = props => <fc-chatbar tooltip={props.tooltip} onClick={props.onClick} onContextMenu={props.onContextMenu}>{props.children}</fc-chatbar>;
