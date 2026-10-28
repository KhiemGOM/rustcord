// @utils/Logger, printing where Fastcord's log goes.
export class Logger {
    static makeTitle = (color, title) => [`[${title}]`];
    constructor(name, color) { this.name = name; this.color = color; }
    log(...a) { console.log(`[${this.name}]`, ...a); }
    info(...a) { console.info(`[${this.name}]`, ...a); }
    warn(...a) { console.warn(`[${this.name}]`, ...a); }
    error(...a) { console.error(`[${this.name}]`, ...a); }
    debug(...a) { console.debug(`[${this.name}]`, ...a); }
    errorCustomFmt(_, ...a) { console.error(`[${this.name}]`, ...a); }
}
