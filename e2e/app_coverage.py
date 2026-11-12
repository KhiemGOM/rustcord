"""Inventory app surfaces and attach recorded runtime evidence.

    python e2e/app_coverage.py --campaign e2e/artifacts/usage-campaign --out e2e/artifacts/usage-campaign/app-coverage --watch
"""

import argparse
import hashlib
import html
import json
import os
import re
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

import parity
import discord_coverage

METHODS = ('click', 'right', 'hover', 'middle')
ROLES = {'button', 'tab', 'textbox', 'menuitem', 'switch', 'checkbox', 'radio', 'combobox', 'slider', 'option', 'link'}
_REF_SIGNATURE = None
_REF_DATA = None


def source_catalog(repo):
    controls = {}
    dynamic = []
    checkpoints = {}
    for path in sorted((repo / 'src/ui').rglob('*.rs')):
        raw = path.read_text(encoding='utf-8')
        text = re.sub(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*.*?\*/', lambda m: m[0] if m[0].startswith('"') else ' ' * len(m[0]), raw, flags=re.S)
        text = text.split('#[cfg(test)]')[0]
        calls = ((r'\b(?:mark|mark_in)\(\s*[^,\n]+,\s*"([^"\n]+)"', 'control:'),
                 (r'\brects\.insert\(\s*"([^"\n]+)"', ''))
        for pattern, prefix in calls:
            for match in re.finditer(pattern, text):
                key = prefix + match[1]
                source = f'{path.resolve()}:{text[:match.start()].count(chr(10)) + 1}'
                controls.setdefault(key, dict(id=key, kind='control', sources=[]))['sources'].append(source)
        for match in re.finditer(r'\b(?:mark|mark_in|rects\.insert)\([^\n]*format!\(\s*"([^"\n]+)"', text):
            dynamic.append(dict(pattern=match[1], source=f'{path.resolve()}:{text[:match.start()].count(chr(10)) + 1}'))
        for match in re.finditer(r'\b(?:shot|shot_probe)\(\s*ctx,\s*"([^"\n]+\.png)"', text):
            name = match[1]
            row = checkpoints.setdefault(name, dict(id='capture:' + name, kind='checkpoint', title=name,
                                                    screenshot_pattern=re.escape(name), sources=[]))
            row['sources'].append(f'{path.resolve()}:{text[:match.start()].count(chr(10)) + 1}')
    views = []
    tour = repo / 'src/ui/e2e/tour.rs'
    tour_text = tour.read_text(encoding='utf-8') if tour.exists() else ''
    server_slugs = dict(re.findall(r'\(\s*ServerPage::(\w+),\s*"([^"]+)"', tour_text))
    for area in ('settings', 'server', 'channel'):
        path = repo / f'src/ui/{area}/mod.rs'
        if not path.exists():
            continue
        text = path.read_text(encoding='utf-8')
        enum = re.search(r'pub enum Page\s*\{([^}]+)\}', text)
        if not enum:
            continue
        labels = dict(re.findall(r'Item::Page\(\s*Page::(\w+),\s*"([^"]+)"', text))
        for variant in re.findall(r'\b(\w+)\s*(?:,|$)', enum[1]):
            label = labels.get(variant, variant)
            slug = ''.join(c.lower() if c.isalnum() else '_' for c in label)
            if area == 'server':
                slug = server_slugs.get(variant, variant.lower())
            elif area == 'channel':
                slug = variant.lower()
            views.append(dict(id=f'{area}:{variant}', kind='view', title=f'{area} / {label}',
                              source=f'{path.resolve()}:{text[:enum.start()].count(chr(10)) + 1}',
                              screenshot_pattern=rf'{"chan" if area == "channel" else area}_\d+_{re.escape(slug)}_\d+\.png'))
    return list(controls.values()), views + list(checkpoints.values()), dynamic


def parse_log(path, bindings=()):
    controls = {}
    events = []
    errors = []
    pending = False
    scoped = {}
    scoped_bindings = [b for b in bindings if b['rustcord'].get('required_visible')]
    lines = path.read_text(encoding='utf-8').splitlines(keepends=True)
    for number, line in enumerate(lines, 1):
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            if number == len(lines) and not line.endswith('\n'):
                pending = True
            else:
                errors.append(f'{path}:{number}: {error}')
            continue
        events.append(event)
        for key in event.get('visible', []):
            row = controls.setdefault(key, dict(observations=0, methods={}, outcomes={}, evidence=[]))
            row['observations'] += 1
            if not row['evidence']:
                row['evidence'].append(dict(path=str(path.resolve()), line=number, case=event.get('case'), state=event.get('before'), event='observed'))
        action = event.get('action', {})
        key = action.get('target')
        if key:
            row = controls.setdefault(key, dict(observations=0, methods={}, outcomes={}, evidence=[]))
            method = action.get('method', 'right' if action.get('right') else 'click')
            row['methods'][method] = row['methods'].get(method, 0) + 1
            if not any(e['event'] == method for e in row['evidence']):
                row['evidence'].append(dict(path=str(path.resolve()), line=number, case=event.get('case'), state=event.get('before'), event=method))
            visible = set(event.get('visible', []))
            for binding in scoped_bindings:
                target = binding['rustcord']
                if key == target['control'] and method == target['method'] and set(target['required_visible']) <= visible:
                    proof = scoped.setdefault(binding['id'], dict(attempts=0, evidence=[]))
                    proof['attempts'] += 1
                    if len(proof['evidence']) < 2:
                        proof['evidence'].append(dict(path=str(path.resolve()), line=number, case=event.get('case'), state=event.get('before'), event=method))
    for previous, current in zip(events, events[1:]):
        key = previous.get('action', {}).get('target')
        if key and previous.get('before') and current.get('before'):
            outcome = 'marker_state_changed' if previous['before'] != current['before'] else 'no_marker_state_change'
            row = controls[key]['outcomes']
            row[outcome] = row.get(outcome, 0) + 1
    return dict(controls=controls, actions=sum('action' in e for e in events), errors=errors, pending=pending,
                states=list({e['before'] for e in events if e.get('before')}), scoped_inputs=scoped)


def references(ref, cache=None):
    global _REF_SIGNATURE, _REF_DATA
    index = ref / 'INDEX.md'
    if not index.exists():
        return dict(views=[], controls=[], errors=['reference atlas is unavailable'])
    names = list(dict.fromkeys(re.findall(r'^### \[([^\]]+)\]\(([^)]+\.png)\)', index.read_text(encoding='utf-8'), re.M)))
    stats = [(str(index), index.stat().st_mtime_ns)]
    for _, name in names:
        meta = (ref / name).with_suffix('.json')
        stats.append((name, meta.stat().st_mtime_ns, meta.stat().st_size) if meta.exists() else (name, None))
    signature = hashlib.sha256(json.dumps(stats).encode()).hexdigest()
    if _REF_SIGNATURE == signature:
        return _REF_DATA
    if cache and cache.exists():
        saved = json.loads(cache.read_text(encoding='utf-8'))
        if saved.get('signature') == signature:
            saved['data']['signature'] = signature
            _REF_SIGNATURE, _REF_DATA = signature, saved['data']
            return _REF_DATA
    views, controls, errors = [], [], []
    for title, name in names:
        image = ref / name
        meta = image.with_suffix('.json')
        key = 'discord:' + name
        area = Path(name).parts[0] if len(Path(name).parts) > 1 else 'reference'
        views.append(dict(id=key, kind='reference_view', title=title, area=area, image=str(image.resolve()),
                          metadata=str(meta.resolve()), status='unmapped', verified=False))
        if not meta.exists():
            continue
        try:
            data = json.loads(meta.read_text(encoding='utf-8'))
            for i, element in enumerate(data.get('els', [])):
                role = element.get('role') or element.get('t')
                if role not in ROLES and element.get('t') not in ('button', 'input', 'textarea', 'select', 'a') and element.get('cur') != 'pointer':
                    continue
                x, y, w, h = (element.get(k, 0) for k in ('x', 'y', 'w', 'h'))
                if w <= 0 or h <= 0 or x + w <= 0 or y + h <= 0 or x >= data.get('vw', float('inf')) or y >= data.get('vh', float('inf')):
                    continue
                label = element.get('l') or element.get('text') or element.get('c') or role or 'unlabelled'
                controls.append(dict(id=f'{key}#node:{i}', kind='reference_control', reference_view=key, area=area,
                                     title=label[:240], role=role, rect=[x, y, w, h], status='unmapped', verified=False))
        except (ValueError, OSError, TypeError) as error:
            errors.append(f'{meta}: {error}')
    data = dict(views=views, controls=controls, errors=errors, signature=signature)
    if cache:
        parity.write_json(cache, dict(signature=signature, data=data))
    _REF_SIGNATURE, _REF_DATA = signature, data
    return data


def collect(repo, campaign, ref, cache=None):
    static, views, dynamic = source_catalog(repo)
    path = repo / 'e2e/discord_features.json'
    bindings = discord_coverage.load(path) if path.exists() else []
    binding_signature = hashlib.sha256(json.dumps(bindings, sort_keys=True).encode()).hexdigest()
    rows = {c['id']: dict(c, observations=0, methods={}, outcomes={}, evidence=[], verified=False) for c in static}
    logs = {}
    if cache and (cache / 'logs-v4.json').exists():
        logs = json.loads((cache / 'logs-v4.json').read_text(encoding='utf-8'))
    active_logs = {}
    errors, pending, states = [], [], set()
    actions = 0
    scoped = {}
    for path in sorted(campaign.glob('*/native/*/explore/actions.jsonl')):
        stamp = [path.stat().st_mtime_ns, path.stat().st_size, binding_signature]
        saved = logs.get(str(path))
        summary = saved['summary'] if saved and saved['stamp'] == stamp else parse_log(path, bindings)
        active_logs[str(path)] = dict(stamp=stamp, summary=summary)
        actions += summary['actions']
        states.update(summary['states'])
        errors.extend(summary['errors'])
        if summary['pending']:
            pending.append(str(path))
        for key, value in summary['scoped_inputs'].items():
            proof = scoped.setdefault(key, dict(attempts=0, evidence=[]))
            proof['attempts'] += value['attempts']
            proof['evidence'] = (proof['evidence'] + value['evidence'])[:3]
        for key, observed in summary['controls'].items():
            row = rows.setdefault(key, dict(id=key, kind='control', sources=[], observations=0, methods={}, outcomes={}, evidence=[], verified=False))
            row['observations'] += observed['observations']
            for field in ('methods', 'outcomes'):
                for name, count in observed[field].items():
                    row[field][name] = row[field].get(name, 0) + count
            evidence = {e['event']: e for e in row['evidence'] + observed['evidence']}
            row['evidence'] = sorted(evidence.values(), key=lambda e: e['event'] == 'observed')
    if cache:
        parity.write_json(cache / 'logs-v4.json', active_logs)
    for row in rows.values():
        row['status'] = 'interacted' if row['methods'] else 'observed' if row['observations'] else 'untested'
        row['origin'] = 'source_literal' if row['sources'] else 'runtime_discovered'
    shots = {}
    for path in campaign.glob('*/native/**/*.png'):
        with path.open('rb') as stream:
            header = stream.read(24)
        if len(header) < 24 or header[:8] != b'\x89PNG\r\n\x1a\n':
            continue
        shots.setdefault(path.name, []).append(path)
    for view in views:
        captures = [path for name, paths in shots.items() if re.fullmatch(view['screenshot_pattern'], name) for path in paths]
        variants = set()
        for path in captures:
            for part in path.parts:
                match = re.fullmatch(r'tour_(\d+x\d+)(?:_z(\d+))?', part)
                if match:
                    variants.add(match[1] + '@' + (match[2] or '100'))
        view.update(status='captured' if captures else 'untested', captured_variants=sorted(variants),
                    evidence=[str(path.resolve()) for path in captures[:3]], capture_files=len(captures), verified=False)
    reference = references(ref, cache / 'reference.json' if cache else None)
    result = dict(version=1, generated=datetime.now(timezone.utc).isoformat(), campaign=str(campaign.resolve()),
                complete_app_denominator=False, scope='Best-effort Rust source page/marker inventory plus runtime discoveries and the saved Discord atlas. Not code/branch coverage or every possible app state.',
                actions=actions, observed_states=len(states), controls=sorted(rows.values(), key=lambda r: r['id']), views=views,
                dynamic_sites=dynamic, reference_views=reference['views'], reference_controls=reference['controls'],
                reference_signature=reference.get('signature', 'unavailable'),
                pending_logs=pending, errors=errors + reference['errors'], scoped_inputs=scoped)
    result['discord_features'] = discord_coverage.matrix(result, bindings)
    return result


def relative(path, out):
    try:
        return quote(os.path.relpath(path, out).replace('\\', '/'), safe='/.:')
    except ValueError:
        return Path(path).resolve().as_uri()


def reference_pages(out, data):
    folder = out / 'reference'
    folder.mkdir(exist_ok=True)
    views = {row['id']: dict(row, page=folder / f'view-{i:05}.html') for i, row in enumerate(data['reference_views'])}
    by_view = {key: [] for key in views}
    groups = {}
    for row in data['reference_controls']:
        by_view[row['reference_view']].append(row)
        key = (row.get('role'), row['title'])
        group = groups.setdefault(key, dict(views=set(), instances=0))
        group['views'].add(row['reference_view'])
        group['instances'] += 1
    signature = out / 'reference-pages.signature'
    page_signature = data['reference_signature'] + ':feature-bindings-v1'
    changed = not signature.exists() or signature.read_text() != page_signature
    style = '<meta charset="utf-8"><style>body{font:15px system-ui;background:#17171b;color:#eee;margin:30px}a{color:#a9beff}td,th{padding:8px;text-align:left;border-bottom:1px solid #555}td{max-width:650px;overflow-wrap:anywhere}</style>'
    if changed:
        parity.write_json(out / 'reference-inventory.json', dict(views=data['reference_views'], controls=data['reference_controls']))
        for key, view in views.items():
            body = f'<h1>{html.escape(view["title"])}</h1><p>Captured Discord control candidates. <a href="../discord.html">Feature and interaction coverage</a> lists explicit Rustcord exercise bindings. Correctness remains unverified.</p>'
            for field in ('image', 'metadata'):
                if Path(view[field]).is_file():
                    body += f'<a href="{relative(view[field], folder)}">{field}</a> '
            body += '<table><tr><th>Role</th><th>Label or CSS fallback</th><th>Rectangle</th></tr>'
            body += ''.join(f'<tr><td>{html.escape(str(row["role"]))}</td><td>{html.escape(row["title"])}</td><td>{row["rect"]}</td></tr>' for row in by_view[key])
            view['page'].write_text('<!doctype html>' + style + body + '</table>', encoding='utf-8')
    result = []
    for (role, title), group in groups.items():
        key = hashlib.sha256(json.dumps([role, title]).encode()).hexdigest()[:20]
        page = folder / f'group-{key}.html'
        if changed:
            body = f'<h1>{html.escape(title)}</h1><p>{group["instances"]} candidate instances in {len(group["views"])} captures. Grouped by role and label for navigation; these are not necessarily the same feature.</p><ul>'
            body += ''.join(f'<li><a href="{views[v]["page"].name}">{html.escape(views[v]["title"])}</a></li>' for v in sorted(group['views']))
            page.write_text('<!doctype html>' + style + body + '</ul>', encoding='utf-8')
        result.append(dict(id='discord-label:' + key, kind='reference_control_group', title=title, status='unmapped', verified=False,
                           detail=f'{role}: {group["instances"]} instances in {len(group["views"])} captures; grouped by label, not feature identity', links=[str(page)]))
    if changed:
        signature.write_text(page_signature)
    return result, views


def render(out, data):
    groups, reference_views = reference_pages(out, data)
    discord_coverage.render(out, data['discord_features'], data, relative)
    export = dict(data, reference_controls=dict(path='reference-inventory.json', instances=len(data['reference_controls'])))
    parity.write_json(out / 'coverage.json', export)
    counts = {kind: dict(Counter(row['status'] for row in data['views'] + data['controls'] + data['reference_views'] if row['kind'] == kind)) for kind in ('view', 'checkpoint', 'control', 'reference_view')}
    counts['reference_control_instances'] = dict(unmapped=len(data['reference_controls']))
    rows = [dict(row) for row in data['views'] + data['controls'] + data['reference_views']] + groups
    for row in rows:
        if 'evidence' in row:
            row['links'] = [e if isinstance(e, str) else e['path'] for e in row['evidence']]
        elif row.get('image'):
            row['links'] = [str(reference_views[row['id']]['page']), row['image'], row['metadata']]
        if row.get('sources'):
            row['source_note'] = '\n'.join(row['sources'])
    for row in rows:
        row['links'] = [relative(path, out) for path in row.get('links', []) if Path(path).is_file()]
    payload = json.dumps(rows, ensure_ascii=True, separators=(',', ':')).replace('<', '\\u003c')
    summary = json.dumps(counts, indent=2)
    page = '''<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="60"><title>App surface coverage</title>
<style>body{font:15px system-ui;background:#17171b;color:#eee;margin:30px}a{color:#a9beff}td,th{padding:9px;text-align:left;border-bottom:1px solid #555;vertical-align:top}input,select,button{padding:9px;margin:5px}td{max-width:650px;overflow-wrap:anywhere}pre{white-space:pre-wrap}</style>
<h1>App surface coverage</h1><p>Observed = marker appeared. Interacted = input was attempted. Captured = a matching page screenshot exists. Unmapped = Discord reference has no verified Rustcord pairing. None of those establishes functional correctness.</p>
<p><a href="discord.html"><b>Discord feature and interaction coverage</b></a> is the reference-first feature matrix. The tables below are supporting inventories.</p>
<p>Control rows are instrumentation identifiers and can include aliases or layout markers; their counts are not counts of distinct UI features.</p>
<p>Denominators are incomplete: static marker extraction is best effort, dynamic controls are discovered at runtime, and uninstrumented views can be missing. Views include declared settings pages; checkpoints include named captures throughout the app. This is not code/branch coverage. No whole-app percentage is claimed.</p>
<p>Reference control instances are grouped by role/label for navigation. Open a group to see every matching capture. Shared labels do not establish feature identity or parity. <a href="reference-inventory.json">Complete reference inventory</a> · <a href="coverage.json">Coverage data</a></p>
<p id="meta"></p><pre id="summary"></pre><details><summary>Dynamic marker sites and collection notes</summary><pre id="notes"></pre></details>
<select id="kind"><option value="">All kinds</option><option>view</option><option>checkpoint</option><option>control</option><option>reference_view</option><option>reference_control_group</option></select>
<select id="status"><option value="">All statuses</option><option>untested</option><option>observed</option><option>interacted</option><option>captured</option><option>unmapped</option></select><input id="search" placeholder="Find a screen, control or label"><button id="previous">Previous</button><button id="next">Next</button><span id="count"></span>
<table><thead><tr><th>Kind / status</th><th>Screen or control</th><th>Attempted input / capture variants</th><th>Evidence</th></tr></thead><tbody id="rows"></tbody></table>
<script>const rows=PAYLOAD;let page=0;const size=100;const el=id=>document.getElementById(id);
el('meta').textContent=META;el('summary').textContent=SUMMARY;el('notes').textContent=NOTES;
function draw(){const query=el('search').value.toLowerCase(),kind=el('kind').value,status=el('status').value;const list=rows.filter(r=>(!kind||r.kind===kind)&&(!status||r.status===status)&&JSON.stringify(r).toLowerCase().includes(query));page=Math.max(0,Math.min(page,Math.max(0,Math.ceil(list.length/size)-1)));el('rows').replaceChildren();for(const r of list.slice(page*size,(page+1)*size)){const tr=document.createElement('tr');for(const value of [r.kind+' / '+r.status,r.title||r.id,r.detail||JSON.stringify(r.methods||r.captured_variants||{}),null]){const td=document.createElement('td');if(value!==null)td.textContent=value;else{for(const [i,path]of(r.links||[]).entries()){const a=document.createElement('a');a.href=path;a.textContent='evidence '+(i+1)+' ';td.append(a)}if(r.source_note||r.source){const text=document.createElement('pre');text.textContent=r.source_note||r.source;td.append(text)}}tr.append(td)}el('rows').append(tr)}el('count').textContent=list.length+' entries; page '+(page+1)}
for(const id of ['kind','status','search'])el(id).oninput=()=>{page=0;draw()};el('previous').onclick=()=>{page--;draw()};el('next').onclick=()=>{page++;draw()};draw();</script>'''
    encode = lambda value: json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')
    values = dict(PAYLOAD=payload, META=encode(f'Generated {data["generated"]}; {data["actions"]} actions; {data["observed_states"]} instrumented state fingerprints.'),
                  SUMMARY=encode(summary), NOTES=encode(json.dumps(dict(dynamic_sites=data['dynamic_sites'], pending_logs=data['pending_logs'], errors=data['errors']), indent=2)))
    page = re.sub(r'\b(PAYLOAD|META|SUMMARY|NOTES)\b', lambda match: values[match[1]], page)
    campaign_report = Path(data['campaign']) / 'index.html'
    if campaign_report.is_file():
        page = page.replace('<h1>App surface coverage</h1>', f'<h1>App surface coverage</h1><p><a href="{relative(campaign_report, out)}">Campaign assertions and measurements</a></p>')
    pending = out / 'index.html.tmp'
    pending.write_text(page, encoding='utf-8')
    pending.replace(out / 'index.html')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--watch', action='store_true')
    args = parser.parse_args()
    out, campaign = args.out.resolve(), args.campaign.resolve()
    out.mkdir(parents=True, exist_ok=True)
    cache = out / 'cache'
    cache.mkdir(exist_ok=True)
    print(f'App coverage: {out / "index.html"}', flush=True)
    while True:
        data = collect(parity.ROOT, campaign, parity.ROOT / 'ref', cache)
        render(out, data)
        watching = args.watch and not (out / 'stop').exists()
        if watching:
            state = json.loads((campaign / 'campaign.json').read_text(encoding='utf-8'))
            watching = state['status'] not in ('budget_finished', 'blocked', 'stopped')
        parity.write_json(out / 'watcher.json', dict(pid=os.getpid(), generated=data['generated'], status='watching' if watching else 'complete'))
        print(json.dumps({k: len(data[k]) for k in ('views', 'controls', 'reference_views', 'reference_controls')}), flush=True)
        if not watching:
            return
        time.sleep(60)


if __name__ == '__main__':
    main()
