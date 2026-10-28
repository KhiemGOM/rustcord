// @utils/clipboard.
export function copyToClipboard(text) {
    __out({ op: "copy", text: String(text) });
}
