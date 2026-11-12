"""Set Rustcord against what Discord's own client showed for the same inputs on the same account.

    python e2e/discord_pair.py journeys --live <recorder-directory> --out <journeys.json>
    python e2e/discord_pair.py replay --journeys <journeys.json> --out <new-directory>
    python e2e/discord_pair.py diff --live <recorder-directory> --replay <replay-directory> --out <report-directory>
    python e2e/discord_pair.py show --live <recorder-directory> --replay <replay-directory> --state action-000014 [--side before] [--box x,y,w,h] [--crop out.png]

`show` prints one paired state's numbers for both clients inside a box, and with --crop saves the two
screenshots' box side by side, Discord on the left. `journeys` turns each input e2e/discord_explore.cjs recorded into one Rustcord can repeat. `replay`
runs them in Rustcord, read-only, on the saved login. `diff` pairs what each client drew after each
input, text by text, and lists what is missing, extra, moved, resized or recoloured. See e2e/PARITY.md.
"""

import argparse
import html
import json
import os
import re
import shutil
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MARKS = {'User Settings': 'gear', 'Inbox': 'title:inbox', 'Help': 'title:help', 'Pinned Messages': 'pins_button', 'Threads': 'threads_button',
         'Notification Settings': 'bell', 'Search': 'search', 'Mute': 'voice:mute', 'Unmute': 'voice:mute', 'Deafen': 'voice:deafen', 'Undeafen': 'voice:deafen',
         'Open GIF picker': 'gif_button', 'Open sticker picker': 'sticker_button', 'Select emoji': 'emoji_button', 'Apps': 'apps_button',
         'Add Friends to DM': 'dm:add', 'Start Voice Call': 'dm:call', 'Start Video Call': 'dm:video'}
WEIGHTS = {'400': 'Proportional', '500': 'medium', '600': 'semi', '700': 'bold'}
PLAIN = ('gg sans', 'gg mono', 'ABC Ginto')


def flake(value):
    found = re.search(r'(\d{15,})$', value or '')
    return found and found.group(1)


def anchor(target):
    item, message = target.get('item') or '', flake(target.get('id'))
    if target['role'] == 'message':
        return f'msg:{message}' if message else None
    if not flake(item):
        return MARKS.get(target['name'])
    kind = {'channels': 'chan', 'guildsnav': 'guild', 'people-list': 'friend'}.get(item.split('___')[0], 'chan' if item.startswith('private-channels') else None)
    if kind == 'chan' and target['name'].endswith('(category)'):
        kind = 'cat'
    return kind and f'{kind}:{flake(item)}'


def step(recorded):
    method = recorded['method']
    if method == 'key':
        return {'do': 'key', 'key': recorded['key']}
    if method == 'text':
        return {'do': 'text', 'value': recorded['text']}
    box = (recorded.get('scroll') or recorded['target'])['box']
    out = {'do': {'right-click': 'rclick'}.get(method, method), 'at': [box[0] + box[2] / 2, box[1] + box[3] / 2]}
    if method == 'wheel':
        out['delta'] = recorded['delta']
        return out
    target = recorded['target']
    out.update({key: value for key, value in [('rect', anchor(target)), ('text', target.get('text'))] if value})
    return out


def landed(live, actions):
    """Where each asked route really ended: Discord shows a server's first channel when the one asked for can't be viewed."""
    out = {}
    for action in actions:
        if not action['setup'] and action['route'] not in out and action.get('before_capture'):
            out[action['route']] = json.loads((live / (action['before_capture'] + '.json')).read_text(encoding='utf-8'))['route']
    return out


def journeys(live):
    actions = [json.loads(line) for line in (live / 'actions.jsonl').read_text(encoding='utf-8').splitlines()]
    shown, out = landed(live, actions), []
    for action in actions:
        if not action.get('input_executed') or 'after_capture' not in action:
            continue
        route = shown.get(action['route'], action['route']).split('/')[2:]
        guild = route[0] if route[0] != '@me' else None
        out.append({'id': f'action-{action["index"]:06}', 'guild': guild, 'channel': route[1] if len(route) > 1 else None,
                    'steps': [step(s) for s in action['setup'] + [action['input']]]})
    return out


def replay(file, out, size, zoom, exe):
    data = out / 'data'
    data.mkdir(parents=True)
    saved = Path(os.environ['LOCALAPPDATA']) / 'Rustcord'
    shutil.copy(saved / 'settings.json', data / 'settings.json')
    env = {key: value for key, value in os.environ.items() if not key.startswith(('RUSTCORD_', 'MOCK_'))}
    env.update(RUSTCORD_TOUR=str(out), RUSTCORD_READONLY='1', RUSTCORD_REPLAY=str(file), RUSTCORD_TOUR_SIZE=size, RUSTCORD_TOUR_ZOOM=zoom,
               RUSTCORD_DATA=str(data), RUSTCORD_TOKEN=(saved / 'token').read_text(encoding='utf-8').strip(), RUSTCORD_SILENT='1', RUSTCORD_FAKE_MIC='1', RUSTCORD_FAKE_CAM='1')
    return subprocess.run([str(exe)], env=env).returncode


EMOJI = re.compile('[\U0001f000-\U0001faff\u2190-\u2bff\u3030\u303d\u3297\u3299\ufe0f\u200d\u20e3\U000e0020-\U000e007f]')


def norm(text):
    """Rustcord draws an emoji as a picture with a space of text under it, so neither side's emoji is text to match."""
    return re.sub(r'\s+', '', EMOJI.sub('', text))


def colour(value):
    if value.startswith('#'):
        return tuple(int(value[i:i + 2], 16) for i in (1, 3, 5, 7))
    parts = [float(n) for n in re.findall(r'[\d.]+', value)]
    return (*(round(n) for n in parts[:3]), round(parts[3] * 255) if len(parts) > 3 else 255)


def region(box, screen):
    x, y = box[0] + box[2] / 2, box[1] + box[3] / 2
    if x < 72:
        return 'rail'
    if x < 375:
        return 'sidebar bottom' if y > screen[1] - 70 else 'sidebar'
    if y < 50:
        return 'header'
    return 'chat' if x < screen[0] - 265 else 'right panel'


def centre(box):
    return box[0] + box[2] / 2, box[1] + box[3] / 2


def inside(point, box):
    return box[0] <= point[0] <= box[0] + box[2] and box[1] <= point[1] <= box[1] + box[3]


def drawn(rustcord):
    """Rustcord's texts that can be seen: inside their clip and under nothing painted in a later layer."""
    covers = [box for box in rustcord['boxes'] if int(box['fill'][7:9], 16) >= 100 and not box.get('img')]
    return [text for text in rustcord['texts'] if inside(centre(text['b']), text['clip'])
            and not any(box['z'] > text['z'] and inside(centre(text['b']), box['b']) and inside(centre(text['b']), box['clip']) for box in covers)]


def pair(discord, rustcord):
    """Match every text one client drew to the same text in the other, nearest first."""
    theirs, ours = defaultdict(list), defaultdict(list)
    shown = [text for text in discord['texts'] if not text['covered']]
    for text in shown:
        if norm(text['t']):
            theirs[norm(text['t'])].append(text)
    for block in {text['block'] for text in shown}:
        whole = discord['blocks'][block]
        if norm(whole['t']) and norm(whole['t']) not in theirs:
            theirs[norm(whole['t'])].append({**whole, 'block_only': True})
    for text in drawn(rustcord):
        if norm(text['t']):
            ours[norm(text['t'])].append(text)
    pairs, missing, extra = [], [], []
    for key in theirs.keys() | ours.keys():
        a, b = list(theirs.get(key, [])), list(ours.get(key, []))
        while a and b:
            far, d, r = min(((abs(d['b'][0] - r['b'][0]) + abs(d['b'][1] - r['b'][1]), d, r) for d in a for r in b), key=lambda found: found[0])
            if far > 250:
                break
            a.remove(d)
            b.remove(r)
            pairs.append((key, d, r))
        missing += [(key, d) for d in a if not d.get('block_only')]
        extra += [(key, r) for r in b]
    # a paragraph Rustcord drew whole is not missing its words
    whole = [(norm(text['t']), text['b']) for text in drawn(rustcord)]
    within = lambda d, box: box[0] - 250 <= d['b'][0] <= box[0] + box[2] + 250 and box[1] - 250 <= d['b'][1] <= box[1] + box[3] + 250
    missing = [(key, d) for key, d in missing if not any(key in longer and key != longer and within(d, box) for longer, box in whole)]
    return pairs, missing, extra


def label(picture):
    """What a picture is, in a form that repeats across states: an asset's name, a kind of CDN image, or an icon's size and shape."""
    source = picture.get('src') or ''
    if '/assets/' in source:
        return 'asset ' + source.rsplit('/', 1)[1]
    for kind in ('avatars', 'icons', 'emojis', 'banners', 'attachments', 'avatar-decoration-presets', 'clan-badges', 'role-icons', 'stickers'):
        if f'/{kind}/' in source:
            return kind
    if picture['kind'] == 'svg':
        return f'icon {round(picture["b"][2])}x{round(picture["b"][3])} {picture.get("shape", "")}'
    return picture['kind']


def overlap(a, b):
    width = min(a[0] + a[2], b[0] + b[2]) - max(a[0], b[0])
    height = min(a[1] + a[3], b[1] + b[3]) - max(a[1], b[1])
    return 0 if width <= 0 or height <= 0 else width * height / (a[2] * a[3] + b[2] * b[3] - width * height)


def pictures(discord, rustcord, screen):
    """Icons and images by place and size, and painted boxes by colour and corner, where each client drew one."""
    found = []
    covers = [box for box in rustcord['boxes'] if int(box['fill'][7:9], 16) >= 100 and not box.get('img')]
    seen = lambda box: inside(centre(box['b']), box['clip']) and not any(
        over['z'] > box['z'] and inside(centre(box['b']), over['b']) and inside(centre(box['b']), over['clip']) for over in covers)
    ours = [box for box in rustcord['boxes'] if box.get('img') and seen(box)]
    for picture in sorted((p for p in discord.get('images', []) if min(p['b'][2:]) >= 6), key=lambda p: p['b'][2] * p['b'][3]):
        row = {'text': label(picture), 'region': region(picture['b'], screen), 'discord': picture['b']}
        near = [box for box in ours if abs(centre(box['b'])[0] - centre(picture['b'])[0]) <= 12 and abs(centre(box['b'])[1] - centre(picture['b'])[1]) <= 12
                and 0.6 <= box['b'][2] / picture['b'][2] <= 1.6]
        if not near:
            found.append({**row, 'kind': 'picture missing in Rustcord'})
            continue
        box = min(near, key=lambda box: abs(centre(box['b'])[0] - centre(picture['b'])[0]) + abs(centre(box['b'])[1] - centre(picture['b'])[1]))
        ours.remove(box)
        row['rustcord'] = [round(n, 1) for n in box['b']]
        if abs(box['b'][2] - picture['b'][2]) > 1.5 or abs(box['b'][3] - picture['b'][3]) > 1.5:
            found.append({**row, 'kind': 'picture size', 'by': [picture['b'][2:], [round(n, 1) for n in box['b'][2:]]]})
        elif abs(centre(box['b'])[0] - centre(picture['b'])[0]) > 1.5 or abs(centre(box['b'])[1] - centre(picture['b'])[1]) > 1.5:
            found.append({**row, 'kind': 'picture moved', 'by': [round(centre(box['b'])[0] - centre(picture['b'])[0], 1), round(centre(box['b'])[1] - centre(picture['b'])[1], 1)]})
    plain = [box for box in rustcord['boxes'] if not box.get('img') and not box.get('round') and seen(box)]
    for fill in discord.get('fills', []):
        if min(fill['b'][2:]) < 8 or fill['b'][2] > screen[0] * 0.9:
            continue
        best = max(plain, key=lambda box: overlap(fill['b'], box['b']), default=None)
        if best is None or overlap(fill['b'], best['b']) < 0.8:
            continue
        row = {'text': f'box {round(fill["b"][2])}x{round(fill["b"][3])}', 'region': region(fill['b'], screen), 'discord': fill['b'], 'rustcord': [round(n, 1) for n in best['b']]}
        want, have = colour(fill['fill']), colour(best['fill'])
        # a see-through fill's colour on screen depends on what is under it, which only Discord's side knows
        if want[3] >= 250 and max(abs(a - b) for a, b in zip(want, have)) > 8:
            found.append({**row, 'kind': 'fill colour', 'by': [fill['fill'], best['fill']]})
        if abs(min(fill['radius'], min(fill['b'][2:]) / 2) - min(best.get('radius') or 0, min(best['b'][2:]) / 2)) > 1.5:
            found.append({**row, 'kind': 'corner radius', 'by': [fill['radius'], best.get('radius') or 0]})
    return found


def compare(discord, rustcord):
    screen = discord['viewport']
    pairs, missing, extra = pair(discord, rustcord)
    found, columns = [], defaultdict(list)
    for key, d, r in pairs:
        where = region(d['b'], screen)
        row = {'text': d['t'][:120], 'region': where, 'discord': d['b'], 'rustcord': [round(n, 1) for n in r['b']]}
        columns[where].append((d['b'][1], d['b'][0], row, (r['b'][0] - d['b'][0], centre(r['b'])[1] - centre(d['b'])[1])))
        if d.get('block_only'):
            continue
        if r.get('size') and abs(r['size'] - d['size']) > 0.26:
            found.append({**row, 'kind': 'font size', 'by': [d['size'], round(r['size'], 2)]})
        styled = not d['font'].startswith(PLAIN)
        if styled != r['font'].startswith('name') or not styled and d['font'] == 'gg sans' and WEIGHTS.get(d['weight'], r['font']) != r['font']:
            found.append({**row, 'kind': 'font', 'by': [f'{d["font"]} {d["weight"]}', r['font']]})
        elif r.get('rows', 1) == 1 and abs(r['b'][2] - d['b'][2]) > max(2.0, d['b'][2] * 0.04):
            found.append({**row, 'kind': 'text width', 'by': [d['b'][2], round(r['b'][2], 1)]})
        want, have = colour(d['col']), colour(r['col'])
        if not r.get('mixed') and max(abs(a - b) for a, b in zip(want, have)) > 10:
            found.append({**row, 'kind': 'colour', 'by': ['#%02x%02x%02x%02x' % want, '#%02x%02x%02x%02x' % have]})
    # one missing divider moves everything under it: say where the spacing changes, not every text after
    for where, column in columns.items():
        last, above = (0.0, 0.0), 'the edge'
        for _, _, row, off in sorted(column, key=lambda entry: entry[:2], reverse=where in ('chat', 'sidebar bottom')):
            if abs(off[0] - last[0]) > 1.5 or abs(off[1] - last[1]) > 1.5:
                found.append({**row, 'kind': 'spacing', 'by': [round(off[0] - last[0], 1), round(off[1] - last[1], 1)], 'after': above, 'total': [round(off[0], 1), round(off[1], 1)]})
            last, above = off, row['text'][:60]
    found += pictures(discord, rustcord, screen)
    found += [{'text': d['t'][:120], 'region': region(d['b'], screen), 'discord': d['b'], 'kind': 'missing in Rustcord'} for key, d in missing]
    found += [{'text': r['t'][:120], 'region': region(r['b'], screen), 'rustcord': [round(n, 1) for n in r['b']], 'kind': 'only in Rustcord'} for key, r in extra]
    return found, len(pairs), len(pairs) / max(1, len(pairs) + len(missing))


def diff(live, replayed, out):
    out.mkdir(parents=True, exist_ok=True)
    how = {row['id']: row for row in json.loads((replayed / 'replay.json').read_text(encoding='utf-8'))} if (replayed / 'replay.json').exists() else {}
    states, tally, unlike = [], Counter(), []
    for line in (live / 'actions.jsonl').read_text(encoding='utf-8').splitlines():
        action = json.loads(line)
        name = f'action-{action["index"]:06}'
        steps = len(action['setup']) + 1
        sides = {}
        for side, number in [('before', steps - 1), ('after', steps)]:
            theirs, ours = live / f'{name}-{side}.json', replayed / f'{name}.{number}.scene.json'
            if theirs.exists() and ours.exists():
                sides[side] = (json.loads(theirs.read_text(encoding='utf-8')), json.loads(ours.read_text(encoding='utf-8')), number)
        if len(sides) < 2 or 'texts' not in sides['before'][0]:
            continue
        # a journey Rustcord began from another screen says nothing about the input
        if compare(*sides['before'][:2])[2] < 0.5:
            unlike.append(name)
            continue
        for side, (discord, rustcord, number) in sides.items():
            found, matched, _ = compare(discord, rustcord)
            was, now = ({norm(text['t']) for text in scene['texts']} for scene in (sides['before'][0], discord))
            ours_was, ours_now = ({norm(text['t']) for text in drawn(scene)} for scene in (sides['before'][1], rustcord))
            if side == 'after' and len(now - was) > 2 and not ours_now - ours_was:
                found.insert(0, {'kind': 'nothing changed in Rustcord', 'text': f'{action["input"]["method"]} {(action["input"].get("target") or {}).get("name", "")[:80]}', 'region': 'input'})
            tally.update(row['kind'] for row in found)
            states.append({'id': f'{name} {side}', 'family': action['family'], 'route': action['route'], 'input': action['input'], 'aimed': (how.get(name) or {}).get('steps'),
                           'discord_image': os.path.relpath(live / f'{name}-{side}.png', out), 'rustcord_image': os.path.relpath(replayed / f'{name}.{number}.png', out),
                           'matched_texts': matched, 'differences': found})
    # the same text off by the same amount in forty states is one thing to fix
    grouped = defaultdict(lambda: {'count': 0, 'states': []})
    for state in states:
        for row in state['differences']:
            entry = grouped[row['kind'], row['region'], row['text'], json.dumps(row.get('by'))]
            entry.update(kind=row['kind'], region=row['region'], text=row['text'], by=row.get('by'), after=row.get('after'), discord=row.get('discord'), rustcord=row.get('rustcord'))
            entry['count'] += 1
            if len(entry['states']) < 5:
                entry['states'].append(state['id'])
    ranked = sorted(grouped.values(), key=lambda entry: -entry['count'])
    (out / 'differences.json').write_text(json.dumps({'states': states, 'grouped': ranked, 'tally': tally, 'began_elsewhere': unlike}, indent=1), encoding='utf-8')
    rows = ''.join(f'<tr><td>{entry["count"]}</td><td>{html.escape(entry["kind"])}</td><td>{html.escape(entry["region"])}</td><td>{html.escape(entry["text"])}</td>'
                   f'<td>{html.escape(json.dumps(entry["by"]))}{" after " + html.escape(entry["after"]) if entry.get("after") else ""}</td><td>{html.escape(json.dumps(entry["discord"]))}</td><td>{html.escape(json.dumps(entry["rustcord"]))}</td>'
                   f'<td>{html.escape(", ".join(entry["states"]))}</td></tr>' for entry in ranked[:3000])
    shots = ''.join(f'<h3 id="{html.escape(state["id"])}">{html.escape(state["id"])} &middot; {html.escape(state["family"])} &middot; {html.escape(state["input"]["method"])} '
                    f'{html.escape((state["input"].get("target") or {}).get("name", "")[:80])} &middot; {len(state["differences"])} differences</h3>'
                    f'<div class="pair"><figure><img loading="lazy" src="{html.escape(state["discord_image"])}"><figcaption>Discord</figcaption></figure>'
                    f'<figure><img loading="lazy" src="{html.escape(state["rustcord_image"])}"><figcaption>Rustcord</figcaption></figure></div>' for state in states)
    (out / 'index.html').write_text(
        '<!doctype html><meta charset="utf-8"><title>Discord and Rustcord, paired</title><style>body{font:14px system-ui;margin:16px;background:#111;color:#ddd}table{border-collapse:collapse}'
        'td,th{border:1px solid #333;padding:3px 6px;vertical-align:top;max-width:420px;overflow-wrap:anywhere}.pair{display:flex;gap:8px}.pair figure{margin:0;flex:1}.pair img{width:100%}</style>'
        f'<h1>{len(states)} paired states, {sum(tally.values())} differences</h1><p>{html.escape(json.dumps(tally))}</p><p>{len(unlike)} journeys left out, Rustcord began them from another screen: {html.escape(", ".join(unlike))}</p>'
        '<table><tr><th>states</th><th>kind</th><th>region</th><th>text</th><th>by (Discord, Rustcord)</th><th>Discord box</th><th>Rustcord box</th><th>seen in</th></tr>'
        f'{rows}</table>{shots}', encoding='utf-8')
    return states, unlike, tally


def show(live, replayed, state, side, box, crop):
    action = next(json.loads(line) for line in (live / 'actions.jsonl').read_text(encoding='utf-8').splitlines() if f'"index":{int(state.split("-")[1])},' in line.replace(' ', ''))
    number = len(action['setup']) + (side == 'after')
    discord = json.loads((live / f'{state}-{side}.json').read_text(encoding='utf-8'))
    rustcord = json.loads((replayed / f'{state}.{number}.scene.json').read_text(encoding='utf-8'))
    box = box or [0, 0, *discord['viewport']]
    within = lambda b: inside(centre(b), box)
    brief = lambda b: [round(n, 1) for n in b]
    pairs, missing, extra = pair(discord, rustcord)
    print(f'{state} {side}: {action["family"]} {action["route"]} input {action["input"]["method"]} {(action["input"].get("target") or {}).get("name", "")[:60]!r}')
    print('text | discord box size weight colour font | rustcord box size font colour')
    for key, d, r in sorted(pairs, key=lambda found: (found[1]['b'][1], found[1]['b'][0])):
        if within(d['b']):
            print(f'  {d["t"][:40]!r} | {brief(d["b"])} {d.get("size")} {d.get("weight")} {d.get("col")} {d.get("font")} | {brief(r["b"])} {r["size"]} {r["font"]} {r["col"]}')
    for key, d in missing:
        if within(d['b']):
            print(f'  only in Discord: {d["t"][:60]!r} {brief(d["b"])} {d.get("size")} {d.get("weight")} {d.get("col")} {d.get("font")}')
    for key, r in extra:
        if within(r['b']):
            print(f'  only in Rustcord: {r["t"][:60]!r} {brief(r["b"])} {r["size"]} {r["font"]} {r["col"]}')
    print('discord controls (role, name, box, checked, submenu):')
    for control in discord['controls']:
        if within(control['box']):
            print(f'  {control["role"]} {control["name"][:50]!r} {control["box"]} {control["checked"] or ""} {"submenu" if control["submenu"] else ""}')
    print('discord pictures (what, box) and fills (box, colour, radius, border):')
    for picture in discord.get('images', []):
        if within(picture['b']):
            print(f'  {label(picture)} {picture["b"]}')
    for fill in discord.get('fills', []):
        if within(fill['b']):
            print(f'  fill {fill["b"]} {fill["fill"]} {fill["radius"]} {fill["border"] or ""}')
    print('rustcord pictures and boxes (box, fill, radius):')
    for box in rustcord['boxes']:
        if within(box['b']) and inside(centre(box['b']), box['clip']):
            print(f'  {"picture" if box.get("img") else "box"} {brief(box["b"])} {box["fill"]} {box.get("radius") or ""}')
    print('rustcord named rects:')
    for name, rect in sorted(rustcord['marks'].items(), key=lambda mark: (mark[1][1], mark[1][0])):
        if within(rect):
            print(f'  {name} {brief(rect)}')
    if crop:
        from PIL import Image
        halves = []
        for file in (live / f'{state}-{side}.png', replayed / f'{state}.{number}.png'):
            image = Image.open(file).convert('RGB')
            scale = image.width / discord['viewport'][0]
            halves.append(image.crop(tuple(round(n * scale) for n in (box[0], box[1], box[0] + box[2], box[1] + box[3]))).resize((round(box[2] * 1.5), round(box[3] * 1.5))))
        sheet = Image.new('RGB', (halves[0].width * 2 + 6, halves[0].height), 'red')
        sheet.paste(halves[0], (0, 0))
        sheet.paste(halves[1], (halves[0].width + 6, 0))
        sheet.save(crop)
        print('crop:', crop.resolve())


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest='command', required=True)
    make = commands.add_parser('journeys')
    make.add_argument('--live', type=Path, required=True)
    make.add_argument('--out', type=Path, required=True)
    make.add_argument('--limit', type=int)
    run = commands.add_parser('replay')
    run.add_argument('--journeys', type=Path, required=True)
    run.add_argument('--out', type=Path, required=True)
    run.add_argument('--size', default='1707x913')
    run.add_argument('--zoom', default='150')
    run.add_argument('--exe', type=Path, default=ROOT / 'target' / 'release' / 'rustcord.exe')
    report = commands.add_parser('diff')
    report.add_argument('--live', type=Path, required=True)
    report.add_argument('--replay', type=Path, required=True)
    report.add_argument('--out', type=Path, required=True)
    look = commands.add_parser('show')
    look.add_argument('--live', type=Path, required=True)
    look.add_argument('--replay', type=Path, required=True)
    look.add_argument('--state', required=True)
    look.add_argument('--side', choices=['before', 'after'], default='after')
    look.add_argument('--box', type=lambda text: [float(n) for n in text.split(',')])
    look.add_argument('--crop', type=Path)
    args = parser.parse_args()
    if args.command == 'show':
        return show(args.live, args.replay, args.state, args.side, args.box, args.crop)
    if args.command == 'journeys':
        made = journeys(args.live)[:args.limit]
        args.out.write_text(json.dumps(made), encoding='utf-8')
        print(len(made), 'journeys')
    elif args.command == 'replay':
        sys.exit(replay(args.journeys.resolve(), args.out.resolve(), args.size, args.zoom, args.exe))
    else:
        states, unlike, tally = diff(args.live, args.replay, args.out)
        print(len(states), 'paired states;', len(unlike), 'journeys began elsewhere;', dict(tally))
        print('report:', (args.out / 'index.html').resolve())


if __name__ == '__main__':
    main()
