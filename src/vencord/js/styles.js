// @api/Styles: managed stylesheets. Fastcord draws its own UI, so there's nothing to style.
export const styleMap = window.VencordStyles ??= new Map();
export const managedStyleRootNode = document.documentElement;
export const requireStyle = name => ({ name, source: "", classNames: {}, dom: null });
export const isStyleEnabled = () => false;
export const enableStyle = () => false;
export const disableStyle = () => false;
export const toggleStyle = () => {};
export const setStyleClassNames = () => {};
export const compileStyle = () => {};
export const classNameToSelector = (name, prefix = "") => name.split(" ").map(n => `.${prefix}${n}`).join("");
export const classNameFactory = (prefix = "") => (...args) => args.flatMap(a => typeof a === "object" && a ? Object.entries(a).filter(([, v]) => v).map(([k]) => k) : a ? [a] : []).map(n => prefix + n).join(" ");
