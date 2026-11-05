// @api/Notifications: Vencord's notifications, drawn by Rustcord as Vencord draws them (or as a
// Windows notification, as its settings say), and kept in the notification log.
import * as DataStore from "@api/DataStore";
import { Settings } from "@api/Settings";

const KEY = "notification-log";
export const handlers = globalThis.__fcNotify ??= new Map();
let next = 1;

export const requestPermission = async () => true;

export async function persistNotification(n) {
    const limit = Settings.notifications.logLimit;
    if (n.noPersist || limit === 0) return;
    await DataStore.update(KEY, old => {
        const log = old ?? [];
        log.unshift({ title: n.title, body: n.body, icon: n.icon, image: n.image, color: n.color, timestamp: Date.now(), id: Math.random().toString(36).slice(2) });
        if (log.length > limit && limit !== 200) log.length = limit;
        return log;
    });
}

export const getLog = async () => (await DataStore.get(KEY)) ?? [];
export const deleteNotification = id => DataStore.update(KEY, old => (old ?? []).filter(n => n.id !== id));
export const clearLog = () => DataStore.set(KEY, []);

export async function showNotification(data) {
    persistNotification(data);
    const id = next++;
    handlers.set(id, { onClick: data.onClick, onClose: data.onClose });
    __out({
        op: "notify", id, title: String(data.title ?? ""), body: String(data.body ?? ""), icon: data.icon ?? null, image: data.image ?? null,
        color: typeof data.color === "string" ? data.color : null, permanent: !!data.permanent, dismissOnClick: data.dismissOnClick !== false,
    });
}
