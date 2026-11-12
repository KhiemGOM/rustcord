const { createHash } = require('node:crypto');

const readOps = new Set([1, 2, 6, 8, 14, 37, 40, 41, 43]);
const hosts = ['discord.com', 'discord.gg', 'discordapp.com', 'discordapp.net', 'discordcdn.com', 'discord.media'];

function httpAllowed(method, address) {
    if (!['GET', 'HEAD', 'OPTIONS'].includes(method)) return false;
    const url = new URL(address);
    return url.protocol === 'https:' && hosts.some(host => url.hostname === host || url.hostname.endsWith('.' + host));
}

function gatewayAllowed(message) {
    try { return readOps.has(JSON.parse(message)?.op); } catch { return false; }
}

async function installGuard(context, events) {
    await context.route('**/*', async route => {
        const req = route.request();
        if (httpAllowed(req.method(), req.url())) return route.continue();
        const url = new URL(req.url());
        events.push({ type: 'blocked-http', method: req.method(), url: url.origin + url.pathname });
        await route.abort();
    });
    await context.routeWebSocket(/.*/, ws => {
        const url = new URL(ws.url());
        if (!(url.protocol === 'wss:' && (url.hostname === 'gateway.discord.gg' || url.hostname === 'gateway-us-east1-b.discord.gg'))) {
            events.push({ type: 'blocked-socket', host: url.hostname });
            ws.close();
            return;
        }
        const server = ws.connectToServer();
        ws.onMessage(message => {
            if (gatewayAllowed(message)) server.send(message);
            else {
                let op = null;
                try { op = JSON.parse(message)?.op ?? null; } catch {}
                events.push({ type: 'blocked-gateway', op });
            }
        });
        server.onMessage(message => ws.send(message));
    });
    context.on('page', page => {
        page.on('download', download => download.cancel());
        page.on('dialog', dialog => dialog.dismiss());
    });
}

function actionsFor(node, layers = []) {
    if (node.disabled) return ['hover'];
    if (node.role === 'home') return ['click', 'hover'];
    if (/^(Close|Cancel|Back)$/i.test(node.name)) return ['click', 'hover'];
    if (layers.some(layer => layer.role === 'dialog' && /delete|permanently|purchase|billing|payment|credit card|password|two.factor|security code|authorization|authorize/i.test(layer.text))) return ['hover'];
    if (['message', 'member', 'channel', 'guild', 'friend'].includes(node.role)) return ['right-click', 'hover'];
    if (node.submenu) return ['hover'];
    if (node.role === 'tab') return ['click', 'hover'];
    if (/delete|remove friend|unfriend|block|report|purchase|buy|subscribe|gift|boost|join|connect|disconnect|call|logout|log out|invite|authorize|save changes|reset|sync|transfer|ban|kick|upload|download|open app|browser|notification permission|enable notifications/i.test(node.name)) return ['hover'];
    if (node.href && /^\/channels\//.test(node.href)) return ['click', 'hover'];
    if (/^(User Settings|Close|Cancel|Back|Friends|Online|All|Pending|Blocked|Add Friend|Inbox|Mentions|For You|Unreads|Threads|Pinned Messages|Pins|Search|Search Messages|Show Member List|Hide Member List|Open Chat|Open in Full View|More|More Options|Add Emoji|Open GIF picker|Open sticker picker|Apps|Reply|Edit Message|Copy Message Link|Copy Text|Copy User ID|View Profile|Profile|Server Settings|Channel Settings|Edit Channel|Browse Channels|Channels & Roles|Events|Notification Settings|Privacy Settings|Create Channel|Create Event|Create Thread|Create Poll|New Post|Sort & View|Filter|Add Tags|Show All Tags|Show All|Guidelines|About|Roles|Members|Overview|Permissions|Soundboard|Activity Status|Advanced|Voice & Video)$/i.test(node.name)) return ['click', 'hover'];
    if (node.scope === 'settings-nav' && ['link', 'button'].includes(node.role)) return ['click', 'hover'];
    if (['button', 'menuitem', 'menuitemcheckbox', 'menuitemradio', 'checkbox', 'switch', 'radio', 'option', 'combobox', 'slider'].includes(node.role)) return ['click', 'hover'];
    return ['hover'];
}

function fingerprint(state) {
    const controls = state.controls.map(({ key, box, selector, text, ...node }) => node);
    const scrolls = state.scrolls?.map(({ key, box, ...scroll }) => scroll);
    return createHash('sha256').update(JSON.stringify({ route: state.route, layers: state.layers, textboxes: state.textboxes, scrolls, controls })).digest('hex').slice(0, 24);
}

function inspect() {
    const visible = e => {
        const r = e.getBoundingClientRect(), s = getComputedStyle(e);
        return r.width > 2 && r.height > 2 && r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth && s.visibility !== 'hidden' && s.display !== 'none' && +s.opacity > 0;
    };
    const controls = [];
    for (const e of document.querySelectorAll('[data-discord-audit]')) e.removeAttribute('data-discord-audit');
    const query = 'button,a[href],select,input:not([type=password]):not([type=email]):not([type=tel]),[role=button],[role=tab],[role=textbox],textarea,[role^=menuitem],[role=checkbox],[role=switch],[role=radio],[role=option],[role=combobox],[role=slider],[role=treeitem],[data-list-item-id],li[id^=chat-messages-]';
    for (const e of document.querySelectorAll(query)) {
        if (!visible(e)) continue;
        const r = e.getBoundingClientRect();
        const hit = document.elementFromPoint(Math.min(innerWidth - 1, Math.max(0, r.x + r.width / 2)), Math.min(innerHeight - 1, Math.max(0, r.y + r.height / 2)));
        if (hit && !e.contains(hit) && !hit.contains(e)) continue;
        const item = e.getAttribute('data-list-item-id') || '';
        let role = e.getAttribute('role') || (e.tagName === 'A' ? 'link' : 'button');
        if (e.tagName === 'INPUT') role = ['checkbox', 'radio'].includes(e.type) ? e.type : e.type === 'range' ? 'slider' : 'textbox';
        if (e.tagName === 'SELECT') role = 'combobox';
        if (e.id.startsWith('chat-messages-')) role = 'message';
        else if (item.startsWith('members')) role = 'member';
        else if (item === 'guildsnav___home') role = 'home';
        else if (item.startsWith('guildsnav')) role = 'guild';
        else if (item.startsWith('channels')) role = 'channel';
        else if (item.startsWith('people')) role = 'friend';
        const name = (e.getAttribute('aria-label') || e.getAttribute('data-name') || e.innerText || e.getAttribute('title') || '').trim().replace(/\s+/g, ' ').slice(0, 180);
        if (!name && !e.id && !item) continue;
        const root = e.closest('[role=menu],[role=dialog],nav,[role=navigation],[role=tablist]');
        let scope = root ? (root.getAttribute('aria-label') || root.id || root.getAttribute('role') || root.tagName.toLowerCase()) : 'page';
        if (root?.matches('nav[aria-label="Settings pages"]')) scope = 'settings-nav';
        const href = e.getAttribute('href');
        const key = String(controls.length);
        e.setAttribute('data-discord-audit', key);
        controls.push({ key, role, name, scope, id: e.id, item, href, layer: !!e.closest('[role=menu],[role=dialog],[role=listbox]'),
            text: (e.innerText || '').trim().split('\n')[0].slice(0, 120),
            checked: e.getAttribute('aria-checked') ?? (['checkbox', 'radio'].includes(e.type) ? String(e.checked) : null), selected: e.getAttribute('aria-selected'), expanded: e.getAttribute('aria-expanded'),
            value: e.getAttribute('aria-valuenow') ?? (role === 'slider' ? e.value : null),
            disabled: e.hasAttribute('disabled') || e.getAttribute('aria-disabled') === 'true',
            submenu: e.getAttribute('aria-haspopup') === 'true' || !!e.querySelector('[class*=caret]'),
            box: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)] });
    }
    const layers = [...document.querySelectorAll('[role=dialog],[role=menu],[role=listbox],[role=tooltip]')].filter(visible).map(e => ({ role: e.getAttribute('role'), name: e.getAttribute('aria-label') || '', text: (e.innerText || '').slice(0, 1800) }));
    const textboxes = [...document.querySelectorAll('[role=textbox],textarea,input:not([type=password]):not([type=email]):not([type=tel])')].filter(visible).map(e => ({ name: e.getAttribute('aria-label') || e.getAttribute('placeholder') || '', value: (e.value ?? e.innerText ?? '').slice(0, 500), focused: e === document.activeElement }));
    for (const e of document.querySelectorAll('[data-discord-scroll]')) e.removeAttribute('data-discord-scroll');
    const scrolls = [];
    for (const e of document.querySelectorAll('[class*=scroller],[class*=scrollbar],main,nav')) {
        if (!visible(e) || e.scrollHeight <= e.clientHeight + 2 || !/auto|scroll/.test(getComputedStyle(e).overflowY)) continue;
        const r = e.getBoundingClientRect(), hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
        if (hit && !e.contains(hit) && !hit.contains(e)) continue;
        const root = e.closest('nav,aside,main,[role=dialog],[role=listbox],[role=menu]');
        const scope = root ? root.getAttribute('aria-label') || root.id || root.getAttribute('role') || root.tagName.toLowerCase() : 'page';
        const key = String(scrolls.length);
        e.setAttribute('data-discord-scroll', key);
        scrolls.push({ key, id: e.id, name: e.getAttribute('aria-label') || String(e.className).split(' ')[0], scope, top: Math.round(e.scrollTop), height: e.clientHeight,
            box: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)] });
    }
    const texts = [], blocks = new Map(), walk = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT), range = document.createRange();
    const ink = document.createElement('canvas').getContext('2d', { willReadFrequently: true }), inks = new Map();
    // computed colours come back as oklab() wherever the stylesheet mixes them
    const srgb = value => {
        if (!inks.has(value)) {
            ink.clearRect(0, 0, 1, 1);
            ink.fillStyle = value;
            ink.fillRect(0, 0, 1, 1);
            inks.set(value, '#' + [...ink.getImageData(0, 0, 1, 1).data].map(n => n.toString(16).padStart(2, '0')).join(''));
        }
        return inks.get(value);
    };
    const readerOnly = e => {
        for (let i = 0; e && i < 4; i++, e = e.parentElement) {
            const r = e.getBoundingClientRect();
            if (r.width <= 1 || r.height <= 1 || getComputedStyle(e).clip !== 'auto') return true;
        }
        return false;
    };
    for (let node = walk.nextNode(); node && texts.length < 4000; node = walk.nextNode()) {
        const t = node.textContent.replace(/\s+/g, ' ').trim(), parent = node.parentElement;
        if (!t || !parent || readerOnly(parent)) continue;
        range.selectNodeContents(node);
        const r = range.getBoundingClientRect(), s = getComputedStyle(parent);
        if (r.width < 1 || r.height < 1 || r.bottom <= 0 || r.top >= innerHeight || r.right <= 0 || r.left >= innerWidth || s.visibility === 'hidden' || +s.opacity === 0) continue;
        const hit = document.elementFromPoint(Math.min(innerWidth - 1, Math.max(0, r.x + r.width / 2)), Math.min(innerHeight - 1, Math.max(0, r.y + r.height / 2)));
        let block = parent;
        while (block.parentElement && /^inline/.test(getComputedStyle(block).display)) block = block.parentElement;
        if (!blocks.has(block)) {
            const b = block.getBoundingClientRect();
            blocks.set(block, { i: blocks.size, t: (block.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 400), b: [b.x, b.y, b.width, b.height].map(n => Math.round(n * 10) / 10) });
        }
        texts.push({ t: t.slice(0, 400), b: [r.x, r.y, r.width, r.height].map(n => Math.round(n * 10) / 10), size: parseFloat(s.fontSize), weight: s.fontWeight, col: srgb(s.color),
            font: s.fontFamily.split(',')[0].replace(/"/g, ''), line: s.lineHeight, covered: !!hit && !parent.contains(hit) && !hit.contains(parent), block: blocks.get(block).i });
    }
    return { route: location.pathname, title: document.title, viewport: [innerWidth, innerHeight], dpr: devicePixelRatio, layers, textboxes, scrolls, controls, texts, blocks: [...blocks.values()] };
}

// A channel's label carries its unread state and who is in voice, and generated element ids change
// on every load, so a list row or a message is the same one when its Discord id is.
function sameControl(node, target) {
    if (node.role !== target.role || node.scope !== target.scope) return false;
    const flake = value => /\d{15,}$/.exec(value || '')?.[0];
    if (flake(target.item)) return flake(node.item) === flake(target.item);
    if (flake(target.id)) return node.id === target.id;
    if (target.role === 'member') return node.text === target.text;
    return node.name === target.name && node.href === target.href;
}

function resolveTarget(state, target) {
    const candidates = state.controls.filter(node => sameControl(node, target));
    if (!candidates.length) throw new Error('reference target missing');
    const far = node => Math.hypot(node.box[0] - target.box[0], node.box[1] - target.box[1]);
    return candidates.reduce((best, node) => far(node) < far(best) ? node : best);
}

function resolveScroll(state, target) {
    const matches = state.scrolls.filter(scroll => scroll.id === target.id && scroll.name === target.name && scroll.scope === target.scope);
    if (matches.length !== 1) throw new Error('reference scroller missing or ambiguous');
    return matches[0];
}

function contextMatches(expected, current, target, scroll) {
    const open = state => JSON.stringify(state.layers.filter(layer => layer.role !== 'tooltip').map(layer => [layer.role, layer.name]));
    if (expected.route !== current.route || open(expected) !== open(current)) return false;
    if (scroll) {
        try { if (resolveScroll(current, scroll).top !== scroll.top) return false; } catch { return false; }
    }
    if (!target) return JSON.stringify(expected.textboxes) === JSON.stringify(current.textboxes);
    let actual;
    try { actual = resolveTarget(current, target); } catch { return false; }
    return ['disabled', 'checked', 'selected', 'expanded'].every(key => actual[key] === target[key]);
}

function checkOutcome(step, before, after) {
    const result = (name, passed) => ({ name, status: passed ? 'passed' : 'failed' });
    const menus = state => state.layers.filter(layer => layer.role === 'menu').length;
    if (step.method === 'click' && step.target.role === 'tab') {
        let selected = false;
        try { selected = resolveTarget(after, step.target).selected === 'true'; } catch {}
        return result('clicked tab becomes selected', selected);
    }
    if (step.method === 'right-click') return result('context menu appears', menus(after) > menus(before));
    if (step.method === 'hover' && step.target.submenu) return result('submenu appears', menus(after) > menus(before));
    if (step.method === 'key' && step.key === 'Escape' && before.layers.length) return result('Escape dismisses a layer', after.layers.length < before.layers.length);
    if (step.method === 'wheel') {
        let moved = false;
        try { moved = resolveScroll(before, step.scroll).top !== resolveScroll(after, step.scroll).top; } catch {}
        return result('wheel changes the targeted scroller position', moved);
    }
    if (step.expect === 'autocomplete') return result('autocomplete suggestions appear', after.layers.some(layer => layer.role === 'listbox') || after.controls.some(node => node.role === 'option'));
    if (step.expect === 'newline') return result('Shift+Enter inserts a newline', after.textboxes.some(box => box.value.includes('\n')));
    if (step.expect === 'autocomplete-insert') return result('autocomplete selection changes composer text', JSON.stringify(before.textboxes.map(box => box.value)) !== JSON.stringify(after.textboxes.map(box => box.value)));
    return { name: 'no declared outcome check', status: 'unreviewed' };
}

module.exports = { httpAllowed, gatewayAllowed, installGuard, actionsFor, fingerprint, inspect, sameControl, resolveTarget, resolveScroll, contextMatches, checkOutcome };
