import { writeFile } from 'node:fs/promises';

const [endpoint, output] = process.argv.slice(2);
const native = process.argv.includes('--native');
const pages = await (await fetch(`${endpoint}/json/list`)).json();
const page = pages.find(p => p.type === 'page' && p.url.startsWith('https://discord.com/channels/'));
if (!page) throw new Error('open a Discord channel in the reference instance');
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let seq = 0;
const pending = new Map();
ws.onmessage = event => {
    const r = JSON.parse(event.data);
    const p = pending.get(r.id);
    if (!p) return;
    pending.delete(r.id);
    clearTimeout(p.timer);
    if (r.error) p.reject(new Error(JSON.stringify(r.error)));
    else p.resolve(r.result);
};
function call(method, params = {}) {
    return new Promise((resolve, reject) => {
        const id = ++seq;
        const timer = setTimeout(() => { pending.delete(id); reject(new Error(`timeout: ${method}`)); }, 15000);
        pending.set(id, { resolve, reject, timer });
        ws.send(JSON.stringify({ id, method, params }));
    });
}
async function evaluate(expression) {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
    return r.result.value;
}
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
    for (let i = 0; i < 50; i++) {
        if (await evaluate('!!document.querySelector(\'[data-list-id="chat-messages"]\')')) break;
        await sleep(100);
    }
    await call('Page.bringToFront');
    const target = await evaluate(`(() => {
        const list = document.querySelector('[data-list-id="chat-messages"]');
        if (!list) throw new Error('channel messages are not loaded');
        let e = list;
        while (e && !(e.scrollHeight > e.clientHeight && /auto|scroll/.test(getComputedStyle(e).overflowY))) e = e.parentElement;
        if (!e) throw new Error('no chat scroller');
        window.__scrollReference = { e, trials: [] };
        const r = e.getBoundingClientRect();
        return { x: r.x + r.width / 2, y: r.y + r.height / 2, height: e.clientHeight, zoom: devicePixelRatio, ua: navigator.userAgent };
    })()`);
    let seed = 42;
    const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
    const trials = native ? [null] : [[100], [-100], [100,100,100,100], [100,-100], ...Array.from({ length: 12 }, () => Array.from({ length: 2 + Math.floor(random() * 8) }, () => random() < .25 ? -100 : 100))];
    for (const deltas of trials) {
        await evaluate(`(() => { const s = window.__scrollReference; s.e.scrollTop = Math.max(0, (s.e.scrollHeight - s.e.clientHeight) / 2); })()`);
        await sleep(1500);
        await evaluate(`(() => {
            const s = window.__scrollReference;
            const r = s.e.getBoundingClientRect();
            const anchor = [...s.e.querySelectorAll('li[id^="chat-messages-"]')].find(e => e.getBoundingClientRect().top > r.top + 80);
            const t = { at: performance.now(), start: s.e.scrollTop, anchor: anchor?.id, anchorY: anchor?.getBoundingClientRect().top, frames: [], wheel: [] };
            s.trials.push(t);
            t.listener = e => t.wheel.push([performance.now() - t.at, e.deltaY, e.deltaMode]);
            document.addEventListener('wheel', t.listener, { passive: true, capture: true });
            function frame(now) {
                let current = document.querySelector('[data-list-id="chat-messages"]');
                while (current && !(current.scrollHeight > current.clientHeight && /auto|scroll/.test(getComputedStyle(current).overflowY))) current = current.parentElement;
                if (current) s.e = current;
                const a = t.anchor ? document.getElementById(t.anchor) : null;
                t.frames.push([now - t.at, s.e.scrollTop - t.start, a ? t.anchorY - a.getBoundingClientRect().top : null, s.e.scrollHeight]);
                if (!t.done) requestAnimationFrame(frame);
            }
            requestAnimationFrame(frame);
        })()`);
        if (native) { console.log('ready for native wheel input'); await sleep(60000); }
        for (const deltaY of deltas || []) {
            await call('Input.dispatchMouseEvent', { type: 'mouseWheel', x: target.x, y: target.y, deltaX: 0, deltaY });
            await sleep(40);
        }
        await sleep(600);
        await evaluate(`(() => { const s = window.__scrollReference; const t = s.trials.at(-1); t.done = true; document.removeEventListener('wheel', t.listener, true); delete t.listener; })()`);
    }
    const result = { target, seed: 42, mode: native ? 'native-wheel' : 'cdp-precise-wheel', input: trials, trials: await evaluate('window.__scrollReference.trials') };
    await writeFile(output, JSON.stringify(result, null, 2));
    console.log(JSON.stringify({ output, trials: result.trials.length, single: result.trials.slice(0,2).map(t => ({ wheel: t.wheel, end: t.frames.at(-1) })) }));
} finally {
    await evaluate('delete window.__scrollReference').catch(() => {});
    ws.close();
}
