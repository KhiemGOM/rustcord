const assert = require('node:assert/strict');
const { test } = require('node:test');
const { httpAllowed, gatewayAllowed, actionsFor, fingerprint, installGuard, inspect, resolveTarget, contextMatches, checkOutcome } = require('./discord_reference.cjs');
const { options, seeds, parseState, priority, noveltyKey } = require('./discord_explore.cjs');

test('repeated controls lose priority across routes without being dropped', () => {
    const task = { family: 'home', priority: 3, novelty_key: 'same-menu' };
    assert.ok(priority(task, {}, { 'same-menu': 2 }) > priority(task, {}, {}));
    assert.ok(priority(task, { home: 10 }, {}) > priority(task, {}, {}));
});

test('rows of one list count as one thing tried per family, a named control as itself everywhere', () => {
    const row = name => ({ role: 'channel', name, scope: 'server', item: 'channels___' + name });
    assert.equal(noveltyKey('a', row('general'), 'right-click'), noveltyKey('a', row('art'), 'right-click'));
    assert.notEqual(noveltyKey('a', row('general'), 'right-click'), noveltyKey('b', row('general'), 'right-click'));
    assert.notEqual(noveltyKey('a', row('general'), 'right-click'), noveltyKey('a', row('general'), 'hover'));
    assert.equal(noveltyKey('a', { role: 'guild', name: 'one' }, 'right-click'), noveltyKey('b', { role: 'guild', name: 'two' }, 'right-click'));
    const button = name => ({ role: 'button', name, scope: 'page' });
    assert.equal(noveltyKey('a', button('Inbox'), 'click'), noveltyKey('b', button('Inbox'), 'click'));
    assert.notEqual(noveltyKey('a', button('Inbox'), 'click'), noveltyKey('a', button('Pins'), 'click'));
});

test('local numeric snowflakes retain every digit before constructing Discord routes', () => {
    const store = parseState('{"guild":1339586800519221298,"name":"1339586800519221298","kind":0}');
    assert.equal(store.guild, '1339586800519221298');
    assert.equal(store.name, '1339586800519221298');
    assert.equal(store.kind, 0);
});

test('seed coverage separates channel kinds and owner/member contexts without sensitive routes', () => {
    const store = { me: { id: 'me' }, guilds: { a: { owner: 'me' }, b: { owner: 'other' } }, channels: {
        a: { id: '1', guild: 'a', kind: 0 }, b: { id: '2', guild: 'b', kind: 0 }, c: { id: '3', guild: 'b', kind: 15 },
        d: { id: '4', kind: 1 }, e: { id: '5', guild: 'a', kind: 0, nsfw: true }, f: { id: '6', guild: 'a', kind: 11, archived: true },
    } };
    assert.deepEqual(seeds(store).map(row => row.family), ['home', 'channel-0-owner', 'channel-0-member', 'channel-15-member', 'channel-1-private']);
    assert.throws(() => options(['--out', 'x', '--playwright', 'p', '--hours', 'NaN']), /invalid/);
    assert.throws(() => options(['--out', 'x']), /required/);
});

test('reference transport allows reads and denies every write, even analytics and challenges', () => {
    for (const method of ['GET', 'HEAD', 'OPTIONS']) assert.equal(httpAllowed(method, 'https://discord.com/api/v9/users/@me'), true);
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE']) {
        assert.equal(httpAllowed(method, 'https://discord.com/api/v9/channels/1/messages'), false);
        assert.equal(httpAllowed(method, 'https://discord.com/cdn-cgi/challenge-platform/'), false);
    }
    assert.equal(httpAllowed('GET', 'https://example.com/'), false);
    assert.equal(httpAllowed('GET', 'discord://channels/@me'), false);
    assert.equal(httpAllowed('GET', 'https://discord.com.evil.test/'), false);
});

test('gateway guard blocks presence, voice, unknown and malformed frames', () => {
    for (const op of [1, 2, 6, 8, 14, 37, 40, 41, 43]) assert.equal(gatewayAllowed(JSON.stringify({ op })), true);
    for (const op of [0, 3, 4, 5, 18, 999]) assert.equal(gatewayAllowed(JSON.stringify({ op })), false);
    for (const frame of ['bad', '{}', 'null', '{"op":"2"}']) assert.equal(gatewayAllowed(frame), false);
});

test('disabled controls and dangerous actions remain visible in the backlog', () => {
    assert.deepEqual(actionsFor({ role: 'button', name: 'Delete Server', disabled: false }), ['hover']);
    assert.deepEqual(actionsFor({ role: 'button', name: 'Save Changes', disabled: true }), ['hover']);
    assert.deepEqual(actionsFor({ role: 'button', name: 'Unknown New Feature' }), ['click', 'hover']);
    assert.deepEqual(actionsFor({ role: 'button', name: 'Confirm' }, [{ role: 'dialog', text: 'Delete this server permanently?' }]), ['hover']);
    assert.deepEqual(actionsFor({ role: 'button', name: 'Cancel' }, [{ role: 'dialog', text: 'Delete this server permanently?' }]), ['click', 'hover']);
    assert.deepEqual(actionsFor({ role: 'link', name: 'Unknown', href: 'https://example.com' }), ['hover']);
    assert.deepEqual(actionsFor({ role: 'menuitem', name: 'Apps', submenu: true }), ['hover']);
    assert.deepEqual(actionsFor({ role: 'tab', name: 'Advanced' }), ['click', 'hover']);
    assert.deepEqual(actionsFor({ role: 'message', name: 'message' }), ['right-click', 'hover']);
});

test('fingerprints distinguish semantic state, context and checked state but ignore position', () => {
    const state = { route: '/channels/@me', controls: [{ role: 'menuitemcheckbox', name: 'Mute', scope: 'server', checked: 'false', box: [1, 2, 3, 4] }] };
    const move = structuredClone(state); move.controls[0].box[0] = 10;
    assert.equal(fingerprint(state), fingerprint(move));
    for (const [key, value] of [['checked', 'true'], ['scope', 'dm'], ['name', 'Unmute']]) {
        const changed = structuredClone(state); changed.controls[0][key] = value;
        assert.notEqual(fingerprint(state), fingerprint(changed));
    }
    const scrolled = structuredClone(state);
    scrolled.scrolls = [{ id: 'guilds', name: 'Servers', top: 0, height: 100, box: [0, 0, 100, 100] }];
    const resized = structuredClone(scrolled); resized.scrolls[0].box[0] = 20;
    assert.equal(fingerprint(scrolled), fingerprint(resized));
    resized.scrolls[0].top = 500;
    assert.notEqual(fingerprint(scrolled), fingerprint(resized));
});

test('replay context ignores unrelated live list churn but rejects changed target state and overlays', () => {
    const target = { role: 'menuitemcheckbox', name: 'Mute', scope: 'server-menu', id: 'mute', item: '', href: null, checked: 'false' };
    const expected = { route: '/channels/1/2', layers: [{ role: 'menu', name: '', text: 'Mute' }], controls: [target], textboxes: [] };
    const current = structuredClone(expected);
    current.controls.push({ role: 'member', name: 'New arrival' });
    assert.equal(contextMatches(expected, current, target), true);
    current.controls[0].checked = 'true';
    assert.equal(contextMatches(expected, current, target), false);
    current.controls[0] = target;
    current.layers = [];
    assert.equal(contextMatches(expected, current, target), false);
    current.layers = [{ role: 'menu', name: '', text: 'Mute, and a member who just came online' }, { role: 'tooltip', name: '', text: 'Inbox' }];
    assert.equal(contextMatches(expected, current, target), true);
});

test('a list row is found by its Discord id when live state rewrites its label', () => {
    const row = { role: 'channel', name: 'Room 3 (voice channel), unread, 2 of 4 users', scope: 'server', id: '', item: 'channels___1555206820203724861', href: '/channels/9/1555206820203724861', box: [80, 400, 286, 32] };
    const later = { ...row, name: 'Room 3 (voice channel)', box: [80, 432, 286, 32] };
    const other = { ...row, name: 'Room 4 (voice channel)', item: 'channels___1555206820203724862' };
    assert.equal(resolveTarget({ controls: [other, later] }, row), later);
    assert.throws(() => resolveTarget({ controls: [other] }, row), /missing/);
    const dm = { role: 'link', name: 'someone (direct message)', scope: 'Private channels', id: '', item: 'private-channels-uid_11___1351802151936200769', href: '/channels/@me/1351802151936200769', box: [80, 200, 224, 42] };
    assert.equal(resolveTarget({ controls: [{ ...dm, name: 'unread, someone (direct message)', item: 'private-channels-uid_52___1351802151936200769' }] }, dm).href, dm.href);
    const member = { role: 'member', name: 'eggily Playing a game', text: 'eggily', scope: 'page', id: '', item: 'members-1___10', href: null, box: [1450, 300, 240, 42] };
    assert.equal(resolveTarget({ controls: [{ ...member, name: 'someone else', text: 'someone else' }, { ...member, name: 'eggily', item: 'members-1___12' }] }, member).item, 'members-1___12');
    const message = { role: 'message', name: 'someone 6:48 AM hello', scope: 'page', id: 'chat-messages-1-1562858694314557616', item: '', href: null, box: [376, 315, 1331, 48] };
    assert.equal(resolveTarget({ controls: [{ ...message, name: 'someone, Server Tag: x 6:48 AM hello' }] }, message).id, message.id);
    const twin = { role: 'button', name: 'More', scope: 'page', id: ':r1:', item: '', href: null, box: [10, 10, 24, 24] };
    assert.equal(resolveTarget({ controls: [{ ...twin, id: ':r9:', box: [10, 300, 24, 24] }, { ...twin, id: ':r8:', box: [10, 12, 24, 24] }] }, twin).id, ':r8:');
});

test('outcome checks cannot pass on unrelated state churn or an input attempt alone', () => {
    const target = { role: 'tab', name: 'Advanced', scope: 'settings', id: '', item: '', href: null, selected: 'false' };
    const before = { layers: [], controls: [target], textboxes: [], scrolls: [] };
    const unrelated = structuredClone(before);
    unrelated.controls.push({ role: 'member', name: 'Someone arrived' });
    assert.equal(checkOutcome({ method: 'click', target }, before, unrelated).status, 'failed');
    unrelated.controls[0].selected = 'true';
    assert.equal(checkOutcome({ method: 'click', target }, before, unrelated).status, 'passed');
    assert.equal(checkOutcome({ method: 'hover', target }, before, unrelated).status, 'unreviewed');
    assert.equal(checkOutcome({ method: 'right-click', target }, before, unrelated).status, 'failed');
    unrelated.layers.push({ role: 'menu', name: '', text: 'Copy' });
    assert.equal(checkOutcome({ method: 'right-click', target }, before, unrelated).status, 'passed');
    assert.equal(checkOutcome({ method: 'key', key: 'Escape' }, unrelated, unrelated).status, 'failed');
    assert.equal(checkOutcome({ method: 'key', key: 'Escape' }, unrelated, before).status, 'passed');
    const sidebar = { id: 'sidebar', name: 'Servers', scope: 'Servers', top: 0 };
    const main = { id: 'main', name: 'Chat', scope: 'Chat', top: 0 };
    const scrollBefore = { ...before, scrolls: [sidebar, main] };
    const scrollAfter = { ...before, scrolls: [sidebar, { ...main, top: 500 }] };
    assert.equal(checkOutcome({ method: 'wheel', scroll: sidebar }, scrollBefore, scrollAfter).status, 'failed');
});

test('guard never records credentials or write bodies and only forwards allowed gateway frames', async () => {
    const handlers = {}, events = [], forwarded = [];
    const context = {
        route: async (_, cb) => { handlers.http = cb; },
        routeWebSocket: async (_, cb) => { handlers.ws = cb; },
        on: () => {},
    };
    await installGuard(context, events);
    let aborted = false;
    await handlers.http({ request: () => ({ method: () => 'POST', url: () => 'https://discord.com/api/v9/users/@me?token=secret', postData: () => 'private body' }), abort: async () => { aborted = true; } });
    assert.equal(aborted, true);
    assert.equal(JSON.stringify(events).includes('secret'), false);
    assert.equal(JSON.stringify(events).includes('private body'), false);
    handlers.ws({ url: () => 'wss://gateway.discord.gg/?token=secret', connectToServer: () => ({ onMessage: () => {}, send: m => forwarded.push(m) }), onMessage: cb => { handlers.message = cb; }, send: () => {} });
    for (const op of [2, 3, 4, 1]) handlers.message(JSON.stringify({ op, d: { token: 'secret' } }));
    assert.deepEqual(forwarded.map(m => JSON.parse(m).op), [2, 1]);
    assert.equal(JSON.stringify(events).includes('secret'), false);
});

test('browser inspection and targeting preserve nested state and exclude covered controls', { skip: !process.env.DISCORD_AUDIT_PLAYWRIGHT }, async () => {
    const { chromium } = require(process.env.DISCORD_AUDIT_PLAYWRIGHT);
    const browser = await chromium.launch({ headless: true });
    try {
        const context = await browser.newContext({ serviceWorkers: 'block' });
        const events = [];
        await installGuard(context, events);
        const page = await context.newPage();
        await page.setContent('<button>Behind overlay</button><div role="dialog" aria-label="Settings" style="position:fixed;inset:0;background:white"><button aria-label="Close">x</button><div role="tablist"><button role="tab" aria-selected="false">Advanced</button></div><div role="menu" id="m"><div role="menuitemcheckbox" aria-checked="true">Mute</div></div><input type="password" value="secret"></div>');
        const state = await page.evaluate(inspect);
        assert.equal(state.controls.some(node => node.name === 'Behind overlay'), false);
        assert.equal(JSON.stringify(state).includes('secret'), false);
        const mute = state.controls.find(node => node.name === 'Mute');
        assert.equal(mute.checked, 'true');
        assert.equal(mute.scope, 'm');
        assert.equal(resolveTarget(state, mute).key, mute.key);
        assert.equal(mute.layer, true);
        const drawn = state.texts.find(text => text.t === 'Advanced');
        assert.ok(drawn.size > 0 && drawn.b[2] > 0 && /^#[0-9a-f]{8}$/.test(drawn.col));
        assert.equal(state.blocks[drawn.block].t.includes('Advanced'), true);
        await page.evaluate(() => { document.querySelector('[aria-label=Close]').hidden = true; document.querySelector('[role=dialog]').insertAdjacentHTML('afterbegin', '<button>New button</button><button>Duplicate</button><button>Duplicate</button>'); });
        const changed = await page.evaluate(inspect);
        assert.equal(await page.locator('[data-discord-audit="0"]').count(), 1);
        const duplicates = changed.controls.filter(node => node.name === 'Duplicate');
        assert.equal(duplicates.length, 2);
        assert.equal(resolveTarget(changed, duplicates[1]).key, duplicates[1].key);
        await page.evaluate(async () => { await fetch('https://discord.com/api/v9/audit-fixture', { method: 'POST', body: 'must not leave' }).catch(() => {}); });
        assert.equal(events.some(event => event.type === 'blocked-http' && event.method === 'POST'), true);
    } finally { await browser.close(); }
});
