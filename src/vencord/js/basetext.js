// @components/BaseText: text at Vencord's sizes and weights.
const SIZES = { xxs: "10px", xs: "12px", sm: "14px", md: "16px", lg: "20px", xl: "24px", xxl: "32px" };
const WEIGHTS = { thin: 100, extralight: 200, light: 300, normal: 400, medium: 500, semibold: 600, bold: 700, extrabold: 800 };
export function BaseText({ size = "md", weight = "normal", tag = "div", style, children, ...rest }) {
    return VencordCreateElement(tag, { ...rest, style: { fontSize: SIZES[size], fontWeight: WEIGHTS[weight], ...style }, children });
}
