// @components/Button: Vencord's buttons, as Fastcord's.
const el = VencordCreateElement;
export const Button = ({ variant = "primary", size = "medium", onClick, disabled, children, style }) => el("fc-button", { variant, size, onClick, disabled, style, children });
export const LinkButton = ({ variant = "link", size, href, children }) => el("fc-button", { variant, size, href, children });
export const TextButton = ({ variant = "primary", onClick, disabled, children }) => el("fc-button", { variant: variant === "danger" ? "dangerSecondary" : "link", onClick, disabled, children });
const LOOK = { RED: "dangerPrimary", GREEN: "positive", BRAND: "primary", PRIMARY: "secondary", TRANSPARENT: "secondary", WHITE: "secondary", LINK: "link" };
export const ButtonCompat = Object.assign(({ color = "BRAND", look, size, ...rest }) => Button({ ...rest, size, variant: look === "LINK" ? "link" : LOOK[color] ?? "primary" }), { Colors: Object.fromEntries(Object.keys(LOOK).map(k => [k, k])), Looks: { FILLED: "FILLED", LINK: "LINK" }, Sizes: { SMALL: "small", MEDIUM: "medium", LARGE: "large" } });
