// @api/Notifications: Fastcord's own toast, and a Windows notification when it asks for one.
export const requestPermission = async () => true;

export async function showNotification(data) {
    __out({ op: "notify", title: String(data.title ?? ""), body: String(data.body ?? ""), permanent: !!data.permanent });
}
