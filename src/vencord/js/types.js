// @utils/types: definePlugin and the option and start-time enums.
export default function definePlugin(p) {
    return p;
}

export const makeRange = (start, end, step = 1) => {
    const out = [];
    for (let i = start; i <= end; i += step) out.push(Math.round(i * 100) / 100);
    return out;
};

export const defineDefault = v => v;

export const OptionType = { STRING: 0, NUMBER: 1, BIGINT: 2, BOOLEAN: 3, SELECT: 4, SLIDER: 5, COMPONENT: 6, CUSTOM: 7 };
export const StartAt = { Init: "Init", DOMContentLoaded: "DOMContentLoaded", WebpackReady: "WebpackReady" };
export const ReporterTestable = { None: 2, Start: 4, Patches: 8, FluxEvents: 16 };
export const PluginTags = [];
