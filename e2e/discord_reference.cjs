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
    const controls = state.controls.map(({ key, box, selector, ...node }) => node);
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
        controls.push({ key, role, name, scope, id: e.id, item, href,
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
    return { route: location.pathname, title: document.title, viewport: [innerWidth, innerHeight], dpr: devicePixelRatio, layers, textboxes, scrolls, controls };
}

function resolveTarget(state, target) {
    const candidates = state.controls.filter(node => node.role === target.role && node.name === target.name && node.scope === target.scope
        && node.id === target.id && node.item === target.item && node.href === target.href);
    if (candidates.length !== 1) throw new Error('reference target missing or ambiguous');
    return candidates[0];
}

function resolveScroll(state, target) {
    const matches = state.scrolls.filter(scroll => scroll.id === target.id && scroll.name === target.name && scroll.scope === target.scope);
    if (matches.length !== 1) throw new Error('reference scroller missing or ambiguous');
    return matches[0];
}

function contextMatches(expected, current, target, scroll) {
    if (expected.route !== current.route || JSON.stringify(expected.layers) !== JSON.stringify(current.layers)) return false;
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

module.exports = { httpAllowed, gatewayAllowed, installGuard, actionsFor, fingerprint, inspect, resolveTarget, resolveScroll, contextMatches, checkOutcome };
