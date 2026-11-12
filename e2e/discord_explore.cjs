// Record actual Discord transitions with all account writes blocked. See e2e/PARITY.md.
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const netModule = require('node:net');
const { installGuard, inspect, fingerprint, actionsFor, resolveTarget, resolveScroll, contextMatches, checkOutcome } = require('./discord_reference.cjs');

function options(argv) {
    const args = { hours: 16, actions: 10000, depth: 4, width: 1707, height: 913 };
    for (let i = 0; i < argv.length; i++) {
        const name = argv[i].replace(/^--/, '');
        if (['resume', 'headless', 'help'].includes(name)) args[name] = true;
        else if (['out', 'playwright', 'routes'].includes(name)) args[name] = argv[++i];
        else if (['hours', 'actions', 'depth', 'width', 'height'].includes(name)) args[name] = Number(argv[++i]);
        else throw new Error('unknown option: ' + name);
    }
    if (args.help) return args;
    if (!args.out || !args.playwright) throw new Error('--out and --playwright are required');
    for (const name of ['hours', 'actions', 'depth', 'width', 'height']) if (!Number.isFinite(args[name]) || args[name] <= 0) throw new Error('invalid --' + name);
    return args;
}

function seeds(store) {
    const routes = [{ route: '/channels/@me', family: 'home' }], chosen = new Set();
    for (const channel of Object.values(store.channels || {})) {
        if (![0, 1, 2, 3, 5, 11, 12, 13, 15, 16].includes(channel.kind) || channel.nsfw || channel.archived) continue;
        const guild = store.guilds[channel.guild];
        const ownership = guild ? guild.owner === (store.me?.id ?? store.me) ? 'owner' : 'member' : 'private';
        const family = `channel-${channel.kind}-${ownership}`;
        if (chosen.has(family)) continue;
        chosen.add(family);
        routes.push({ route: `/channels/${guild ? channel.guild : '@me'}/${channel.id}`, family });
    }
    return routes;
}

function parseState(text) {
    return JSON.parse(text, (key, value, context) => typeof value === 'number' && !Number.isSafeInteger(value) ? context.source : value);
}

const listRoles = ['message', 'member', 'channel', 'guild', 'friend', 'option', 'treeitem'];

// Rows of one list open the same menu, so a kind of row is one thing tried per family (the server rail is the same list in all of them);
// a named control is itself wherever it shows.
function noveltyKey(family, target, method) {
    const identity = listRoles.includes(target.role) ? [target.role === 'guild' ? '' : family, target.role, method] : [target.role, target.name, target.scope, method];
    return createHash('sha256').update(JSON.stringify(identity)).digest('hex').slice(0, 24);
}

function priority(task, families, visits) {
    return task.priority + (families[task.family] || 0) * .25 + (visits[task.novelty_key] || 0) * 2;
}

const plainTitle = title => title.replace(/^(?:\(\d+\+?\)|•)\s*/, '');

async function run(args) {
    const out = path.resolve(args.out);
    if (!args.resume) fs.mkdirSync(out);
    const lock = netModule.createServer();
    const lockName = '\\\\.\\pipe\\discord-reference-' + createHash('sha256').update(out.toLowerCase()).digest('hex').slice(0, 24);
    await new Promise((resolve, reject) => { lock.once('error', reject); lock.listen(lockName, resolve); });
    lock.unref();
    const manifestFile = path.join(out, 'run.json');
    const write = (name, value) => {
        const file = path.join(out, name);
        fs.writeFileSync(file + '.tmp', JSON.stringify(value, null, 2));
        fs.renameSync(file + '.tmp', file);
    };
    const read = name => JSON.parse(fs.readFileSync(path.join(out, name), 'utf8'));
    const token = fs.readFileSync(path.join(process.env.LOCALAPPDATA, 'Rustcord', 'token'), 'utf8').trim();
    const store = parseState(fs.readFileSync(path.join(process.env.LOCALAPPDATA, 'Rustcord', 'state.json'), 'utf8')).store;
    const roots = args.routes ? JSON.parse(fs.readFileSync(args.routes, 'utf8')) : seeds(store);
    for (const root of roots) if (!/^\/channels\/(?:@me|\d+)(?:\/\d+)?$/.test(root.route)) throw new Error('invalid seed route');
    const previous = args.resume ? read('run.json') : null;
    const manifest = previous || { version: 1, app: 'discord-web', status: 'starting', started: new Date().toISOString(), pid: process.pid,
        scope: 'Actual Discord, outbound writes and presence/voice changes blocked. UI outcomes are reference evidence, not Rustcord parity.',
        limits: { hours: args.hours, actions: args.actions, depth: args.depth }, roots, actions: 0, states: 0, elapsed_seconds: 0,
        failures: 0, replay_failures: 0, no_change: 0, source_sha256: createHash('sha256').update(fs.readFileSync(__filename)).digest('hex'),
        policy_sha256: createHash('sha256').update(fs.readFileSync(path.join(__dirname, 'discord_reference.cjs'))).digest('hex'), viewport: [args.width, args.height] };
    if (previous && (manifest.source_sha256 !== createHash('sha256').update(fs.readFileSync(__filename)).digest('hex') || manifest.policy_sha256 !== createHash('sha256').update(fs.readFileSync(path.join(__dirname, 'discord_reference.cjs'))).digest('hex'))) throw new Error('recorder changed; start a new run');
    args.depth = manifest.limits.depth;
    [args.width, args.height] = manifest.viewport;
    manifest.pid = process.pid;
    manifest.status = 'starting';
    write('run.json', manifest);
    let queue = args.resume ? read('queue.json') : roots.map(root => ({ ...root, steps: [], kind: 'root', priority: 0 }));
    const known = new Set(args.resume ? read('states.json') : []);
    const attempted = new Set(args.resume ? read('attempted.json') : []);
    const frontier = new Map(args.resume ? read('frontier.json').map(row => [row.key, row]) : []);
    const targetVisits = args.resume ? read('visits.json') : {};
    const titles = args.resume ? read('titles.json') : {};
    const { chromium } = require(path.resolve(args.playwright));
    const browser = await chromium.launch({ headless: !!args.headless, args: ['--mute-audio', '--use-fake-device-for-media-stream', '--disable-backgrounding-occluded-windows', '--disable-renderer-backgrounding'] });
    const context = await browser.newContext({ viewport: { width: args.width, height: args.height }, deviceScaleFactor: 1.5, serviceWorkers: 'block', acceptDownloads: false,
        recordVideo: { dir: path.join(out, 'video'), size: { width: 1280, height: 685 } } });
    const net = [];
    await installGuard(context, net);
    await context.addInitScript(value => { if (location.hostname === 'discord.com') localStorage.setItem('token', JSON.stringify(value)); }, token);
    const page = await context.newPage();
    page.on('filechooser', chooser => chooser.setFiles([]));
    context.on('page', popup => { if (popup !== page) popup.close(); });
    page.on('console', message => {
        if (message.text().includes('[BUILD INFO]')) { manifest.discord_build = message.text().replace(/\s+/g, ' ').slice(0, 350); write('run.json', manifest); }
    });
    page.on('pageerror', () => { manifest.page_errors = (manifest.page_errors || 0) + 1; });
    manifest.browser = browser.version();
    const started = performance.now(), oldElapsed = manifest.elapsed_seconds;
    let currentRoute = null;
    const familyAttempts = {};
    let consecutiveLoadFailures = 0;
    const checkpoint = () => {
        manifest.elapsed_seconds = oldElapsed + (performance.now() - started) / 1000;
        manifest.states = known.size;
        manifest.queued = queue.length;
        manifest.frontier = frontier.size;
        manifest.updated = new Date().toISOString();
        write('queue.json', queue);
        write('states.json', [...known]);
        write('attempted.json', [...attempted]);
        write('frontier.json', [...frontier.values()]);
        write('visits.json', targetVisits);
        write('titles.json', titles);
        write('run.json', manifest);
    };
    const stopped = () => fs.existsSync(path.join(out, 'stop')) || manifest.actions >= manifest.limits.actions || oldElapsed + (performance.now() - started) / 1000 >= manifest.limits.hours * 3600;
    async function snapshot(label) {
        const state = await page.evaluate(inspect);
        state.fingerprint = fingerprint(state);
        state.recorded = new Date().toISOString();
        const file = 'state-' + state.fingerprint, fresh = !fs.existsSync(path.join(out, file + '.json'));
        const name = label || file;
        if (label || fresh) {
            await page.screenshot({ path: path.join(out, name + '.png') });
            fs.writeFileSync(path.join(out, name + '.aria.yml'), await page.locator('body').ariaSnapshot({ timeout: 5000 }));
        }
        if (label) write(label + '.json', state);
        if (fresh) {
            const { texts, blocks, ...slim } = state;
            write(file + '.json', slim);
            if (label) for (const kind of ['.png', '.aria.yml']) fs.copyFileSync(path.join(out, label + kind), path.join(out, file + kind));
        }
        return state;
    }
    async function input(step) {
        if (step.method === 'key') return page.keyboard.press(step.key);
        if (step.method === 'text') return page.keyboard.insertText(step.text);
        const state = await page.evaluate(inspect);
        if (step.method === 'wheel') {
            const scroll = resolveScroll(state, step.scroll);
            await page.mouse.move(scroll.box[0] + scroll.box[2] / 2, scroll.box[1] + scroll.box[3] / 2);
            return page.mouse.wheel(0, step.delta);
        }
        const node = resolveTarget(state, step.target);
        const locator = page.locator(`[data-discord-audit="${node.key}"]`);
        if (step.method === 'hover') return locator.hover({ timeout: 4000 });
        return locator.click({ timeout: 4000, button: step.method === 'right-click' ? 'right' : 'left' });
    }
    // Every page load is a new gateway session on the account, so a load is the fallback: the app's own
    // router takes the route, and the tab title it sets says the route really changed. Discord shows a
    // server's first channel when the one asked for can't be viewed, so the path to return to is the one it settled on.
    async function restore(task, load) {
        if (load || currentRoute === null || !titles[task.route]) {
            try {
                await page.goto('https://discord.com' + task.route, { waitUntil: 'domcontentloaded', timeout: 60000 });
                await page.locator('[aria-label="User Settings"]').waitFor({ timeout: 60000 });
            } catch (error) {
                currentRoute = null;
                throw Object.assign(error, { load: true });
            }
            manifest.loads = (manifest.loads || 0) + 1;
            await page.waitForTimeout(1200);
            titles[task.route] = { title: plainTitle(await page.title()), path: new URL(page.url()).pathname };
        } else {
            for (let i = 0; i < 5 && await page.locator('[role=dialog],[role=menu],[role=listbox]').count(); i++) {
                await page.keyboard.press('Escape');
                await page.waitForTimeout(250);
            }
            await page.mouse.move(args.width / 2, 5);
            await page.evaluate(route => {
                if (location.pathname === route) return;
                history.pushState({}, '', route);
                dispatchEvent(new PopStateEvent('popstate', { state: {} }));
            }, titles[task.route].path);
            await page.waitForFunction(([route, title]) => location.pathname === route && document.title.replace(/^(?:\(\d+\+?\)|•)\s*/, '') === title, [titles[task.route].path, titles[task.route].title], { timeout: 5000 });
            await page.waitForTimeout(600);
        }
        currentRoute = task.route;
        for (const step of task.steps) { await input(step); await page.waitForTimeout(400); }
        if (!task.steps.length) await page.mouse.move(args.width / 2, 5);
        await page.waitForTimeout(300);
    }
    function expand(state, task) {
        const fresh = !known.has(state.fingerprint);
        known.add(state.fingerprint);
        if (!fresh) return;
        const steps = task.kind === 'action' ? [...task.steps, task.input] : task.steps;
        const covered = state.layers.some(layer => ['menu', 'dialog', 'listbox'].includes(layer.role)), rows = {};
        for (const target of state.controls) {
            const identity = [task.family, state.route, target.role, target.name, target.scope, target.id, target.item, target.href];
            const methods = actionsFor(target, state.layers);
            const key = createHash('sha256').update(JSON.stringify(identity)).digest('hex').slice(0, 24);
            const row = frontier.get(key) || { key, family: task.family, target, states: [], methods: {}, click_policy: methods.includes('click') ? 'allowed' : target.disabled ? 'disabled' : 'unreviewed-or-excluded' };
            if (!row.states.includes(state.fingerprint)) row.states.push(state.fingerprint);
            frontier.set(key, row);
            if (steps.length >= args.depth || covered && !target.layer) continue;
            for (const method of methods) {
                if (listRoles.includes(target.role) && (rows[target.role + method] = (rows[target.role + method] || 0) + 1) > 2) continue;
                const attemptKey = state.fingerprint + ':' + key + ':' + method;
                if (attempted.has(attemptKey)) continue;
                attempted.add(attemptKey);
                queue.push({ route: task.route, family: task.family, kind: 'action', steps, before: state.fingerprint, key, input: { method, target },
                    novelty_key: noveltyKey(task.family, target, method),
                    priority: (method === 'click' && target.role === 'tab' ? 2 : method === 'right-click' ? 3 : method === 'click' ? 4 : target.submenu ? 5 : 8) + steps.length * .1 });
            }
        }
        if (state.layers.length && steps.length < args.depth) queue.push({ route: task.route, family: task.family, kind: 'action', steps, before: state.fingerprint, input: { method: 'key', key: 'Escape' }, priority: 6 });
        if (steps.length < args.depth) for (const scroll of state.scrolls) {
            for (const delta of [550, -550, 1800]) queue.push({ route: task.route, family: task.family, kind: 'action', steps, before: state.fingerprint, input: { method: 'wheel', scroll, delta }, priority: 5.5 });
        }
        if (task.kind === 'root') {
            const settings = state.controls.find(node => node.name === 'User Settings');
            if (task.family === 'home' && settings) queue.push({ route: task.route, family: 'user-settings', kind: 'root', steps: [{ method: 'click', target: settings }], priority: 1 });
            const composer = state.controls.find(node => node.role === 'textbox' && /^Message/.test(node.name));
            if (composer) {
                const setup = [...steps, { method: 'click', target: composer }, { method: 'key', key: 'Control+a' }, { method: 'key', key: 'Backspace' }];
                for (const text of ['@', '#', ':sm', '/']) queue.push({ route: task.route, family: task.family, kind: 'action', steps: setup,
                    required_text: { name: composer.name, value: '' }, input: { method: 'text', text, expect: 'autocomplete' }, priority: 4.5 });
                queue.push({ route: task.route, family: task.family, kind: 'action', steps: [...setup, { method: 'text', text: 'atlas local probe' }],
                    required_text: { name: composer.name, value: 'atlas local probe' }, input: { method: 'key', key: 'Shift+Enter', expect: 'newline' }, priority: 4.5 });
            }
        }
        if (task.input?.expect === 'autocomplete') for (const key of ['ArrowDown', 'Tab', 'Escape']) queue.push({ route: task.route, family: task.family,
            kind: 'action', steps, before: state.fingerprint, input: { method: 'key', key, ...(key === 'Tab' ? { expect: 'autocomplete-insert' } : {}) }, priority: 4.5 });
    }
    try {
        checkpoint();
        while (queue.length && !stopped()) {
            const disk = fs.statfsSync(out);
            if (disk.bavail * disk.bsize < 1024 ** 3) { manifest.status = 'blocked'; manifest.error = 'less than 1 GiB free'; break; }
            queue.sort((a, b) => priority(a, familyAttempts, targetVisits) - priority(b, familyAttempts, targetVisits));
            const task = queue[0];
            const matches = state => task.required_text
                ? state.route === task.route && state.textboxes.some(box => box.name === task.required_text.name && box.value === task.required_text.value)
                : contextMatches(read('state-' + task.before + '.json'), state, task.input.target, task.input.scroll);
            let fault = null, matched = false, reloaded = null;
            for (const load of task.kind === 'root' ? [true] : [false, true]) {
                try {
                    await restore(task, load);
                    fault = null;
                    matched = task.kind === 'root' || matches(await page.evaluate(inspect));
                    if (matched) break;
                } catch (error) {
                    fault = error;
                    if (error.load) break;
                }
                if (!load) reloaded = fault ? fault.message.split('\n')[0].slice(0, 120) : 'context differed';
            }
            if (fault?.load) {
                if (++consecutiveLoadFailures >= 3) { manifest.status = 'blocked'; manifest.error = 'three consecutive page load failures'; break; }
                continue;
            }
            consecutiveLoadFailures = 0;
            if (fault) {
                const setup = JSON.stringify(task.steps), kept = queue.filter(other => other.route !== task.route || JSON.stringify(other.steps) !== setup);
                manifest.replay_failures++;
                manifest.unreachable = (manifest.unreachable || 0) + queue.length - kept.length;
                fs.appendFileSync(path.join(out, 'failures.jsonl'), JSON.stringify({ family: task.family, route: task.route, steps: task.steps, error: fault.message.split('\n')[0].slice(0, 200), dropped: queue.length - kept.length }) + '\n');
                queue = kept;
                checkpoint();
                continue;
            }
            const capture = 'action-' + String(manifest.actions + 1).padStart(6, '0');
            const before = await snapshot(task.kind === 'root' ? undefined : capture + '-before');
            if (task.kind === 'root') { expand(before, task); queue.shift(); checkpoint(); continue; }
            const row = { index: manifest.actions + 1, at: new Date().toISOString(), family: task.family, route: task.route, setup: task.steps, input: task.input,
                expected_before: task.before, before: before.fingerprint, parity_status: 'unpaired', before_capture: capture + '-before', replay_context_matched: matched && matches(before), reloaded_after: reloaded,
                context_check: 'route, open layers, target identity and target state; keyboard/scroll also require matching textboxes' };
            const netStart = net.length;
            try {
                if (!row.replay_context_matched) throw new Error('reference context changed during replay');
                await input(task.input);
                row.input_executed = true;
                await page.waitForTimeout(task.input.method === 'hover' ? 700 : 1000);
                const after = await snapshot(capture + '-after');
                row.after = after.fingerprint;
                row.after_capture = capture + '-after';
                row.status = row.before === row.after ? 'no_observable_change' : 'observed_transition';
                row.check = checkOutcome(task.input, before, after);
                // a switch or a show/hide button stays flipped across page loads, and every later start would inherit it
                const target = task.input.target;
                const flipped = task.input.method === 'click' && after.controls.find(node => node.role === target.role && node.box[0] === target.box[0] && node.box[1] === target.box[1]
                    && (node.checked !== target.checked || node.name !== target.name && !node.layer));
                if (flipped) {
                    await page.mouse.click(target.box[0] + target.box[2] / 2, target.box[1] + target.box[3] / 2);
                    await page.waitForTimeout(300);
                    row.flipped_back = true;
                }
                if (row.status === 'no_observable_change') manifest.no_change++;
                expand(after, task);
            } catch (error) {
                row.status = row.replay_context_matched ? 'input_failed' : 'reference_context_mismatch';
                row.error = error.message.split('\n')[0].slice(0, 200);
                manifest.failures++;
            }
            row.blocked_requests = net.slice(netStart);
            fs.appendFileSync(path.join(out, 'actions.jsonl'), JSON.stringify(row) + '\n');
            manifest.actions++;
            familyAttempts[task.family] = (familyAttempts[task.family] || 0) + 1;
            const entry = frontier.get(task.key);
            if (entry && row.input_executed) entry.methods[task.input.method] = (entry.methods[task.input.method] || 0) + 1;
            if (task.novelty_key && row.input_executed) targetVisits[task.novelty_key] = (targetVisits[task.novelty_key] || 0) + 1;
            manifest.status = 'running';
            queue.shift();
            checkpoint();
            if (manifest.actions % 10 === 0) console.log(JSON.stringify({ actions: manifest.actions, states: manifest.states, queued: manifest.queued, failures: manifest.failures, replay_failures: manifest.replay_failures }));
        }
        if (manifest.status !== 'blocked') manifest.status = fs.existsSync(path.join(out, 'stop')) ? 'stopped' : queue.length ? 'budget_finished' : 'frontier_exhausted';
    } finally {
        checkpoint();
        write('blocked-requests.json', net);
        await context.close();
        await browser.close();
        lock.close();
    }
    console.log(JSON.stringify({ report: manifestFile, status: manifest.status, actions: manifest.actions, states: manifest.states }));
    return manifest.status === 'blocked' ? 2 : manifest.failures ? 1 : 0;
}

if (require.main === module) {
    const args = options(process.argv.slice(2));
    if (args.help) console.log('node e2e/discord_explore.cjs --playwright <module-directory> --out <new-directory> [--hours 16] [--actions 10000] [--depth 4] [--routes file.json] [--resume] [--headless]');
    else run(args).then(code => { process.exitCode = code; }).catch(error => { console.error(error.message); process.exitCode = 2; });
}

module.exports = { options, seeds, parseState, priority, noveltyKey };
