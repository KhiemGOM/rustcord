"""Audit actual Discord before/after journeys, menus and hover recordings.

    python e2e/discord_journeys.py --ref ref --live <recorder-directory> --out <report-directory>
"""

import argparse
import html
import json
import os
import re
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

import parity


def delta(before, after):
    a = Counter(line.strip() for line in before.splitlines() if line.strip())
    b = Counter(line.strip() for line in after.splitlines() if line.strip())
    return dict(added=list((b - a).elements()), removed=list((a - b).elements()))


def evidence(folder, name):
    return {kind: str(folder / (name + suffix)) for kind, suffix in [('image', '.png'), ('aria', '.aria.yml'), ('network', '.net.json')]
            if (folder / (name + suffix)).is_file()}


def archive(ref):
    rows = []
    for index in sorted(ref.glob('*/_index.json')):
        folder = index.parent
        items = json.loads(index.read_text(encoding='utf-8'))
        names = {row['name'] for row in items}
        for item in items:
            name = item['name']
            if not name.endswith('_0before'):
                continue
            after = name.removesuffix('_0before') + '_1after'
            before_evidence, after_evidence = evidence(folder, name), evidence(folder, after)
            row = dict(id=f'{folder.name}/{name.removesuffix("_0before")}', family=folder.name, method='archived-input',
                       title=item.get('note', name), source='saved-discord', status='reference_incomplete',
                       before=before_evidence, after=after_evidence, parity_verified=False, recipe_status='missing')
            if after in names and all('image' in e and 'aria' in e for e in [before_evidence, after_evidence]):
                change = delta(Path(before_evidence['aria']).read_text(encoding='utf-8'), Path(after_evidence['aria']).read_text(encoding='utf-8'))
                row['delta'] = change
                row['status'] = 'reference_transition_captured' if any(change.values()) else 'reference_no_observable_change'
                row['image_changed'] = parity.digest(Path(before_evidence['image'])) != parity.digest(Path(after_evidence['image']))
                if not any(change.values()) and row['image_changed']:
                    row['status'] = 'reference_visual_change_only'
            rows.append(row)
    for file in sorted(ref.glob('*/*.menu.json')):
        tree = json.loads(file.read_text(encoding='utf-8'))

        def visit(menu, trail):
            for number, item in enumerate((menu or {}).get('items', [])):
                if not item.get('role', '').startswith('menuitem'):
                    continue
                menu_path = trail + [item.get('text', '')]
                rows.append(dict(id=f'{file.parent.name}/{file.name}#{"/".join(menu_path)}:{number}', family=file.parent.name,
                                 method='menu-state', title=' → '.join(menu_path), source='saved-discord',
                                 status='reference_disabled_control' if item.get('disabled') else 'reference_menu_observed',
                                 menu_item_id=item.get('id'), menu_path=menu_path, checked=item.get('checked'), submenu=item.get('sub', False),
                                 before=dict(metadata=str(file)), after={}, parity_verified=False, recipe_status='missing'))
                visit(item.get('children'), menu_path)

        visit(tree, [])
    for file in sorted(ref.glob('*/*.sweep.json')):
        sweep = json.loads(file.read_text(encoding='utf-8'))
        for item in sweep.get('items', []):
            changed = bool(item.get('hoverDiff') or item.get('reveals') or item.get('layers'))
            row = dict(id=f'{file.parent.name}/{file.name}#{item["i"]}', family=file.parent.name, method='hover',
                       title=item.get('label') or item.get('text') or item.get('cls') or f'hover {item["i"]}', source='saved-discord',
                       status='reference_input_failed' if item.get('err') else 'reference_hover_observed' if changed else 'reference_no_observable_change',
                       before=dict(metadata=str(file)), after={}, parity_verified=False, recipe_status='missing',
                       target={key: item[key] for key in ('role', 'label', 'box') if key in item},
                       delta={key: item[key] for key in ('hoverDiff', 'reveals', 'layers') if key in item})
            if item.get('err'):
                row['error'] = item['err']
            image = file.parent / (file.name.removesuffix('.sweep.json') + '_sweep') / f'{item["i"]:03}_hover.png'
            if image.is_file():
                row['after']['image'] = str(image)
            rows.append(row)
    qol = ref / 'QOL.md'
    if qol.is_file():
        section = ''
        for number, line in enumerate(qol.read_text(encoding='utf-8').splitlines(), 1):
            if line.startswith('## '):
                section = line[3:]
            if re.search(r'\bsrc\b', line) and not re.search(r'\bq\d+', line) and line.lstrip().startswith(('-', '|')):
                rows.append(dict(id=f'qol-source:{number}', family=section, method='source-only', title=line.strip(), source='discord-source-not-exercised',
                                 status='source_only_unexercised', before=dict(source=str(qol)), after={}, parity_verified=False, recipe_status='missing', source_line=number))
    return rows


def recipes(folders):
    result = {}
    for folder in folders:
        for file in sorted(folder.glob('*.json')):
            plan = json.loads(file.read_text(encoding='utf-8'))
            if not isinstance(plan, list) or not plan or not all(isinstance(step, list) and step and isinstance(step[0], str) for step in plan):
                continue
            for i, step in enumerate(plan):
                if step[0] != 'cap' or not step[1].endswith('_0before'):
                    continue
                after = step[1].removesuffix('_0before') + '_1after'
                end = next((j for j in range(i + 1, len(plan)) if plan[j][:2] == ['cap', after]), None)
                if end is None:
                    continue
                inputs = [s for s in plan[i + 1:end] if s[0] not in ('wait', 'cap', 'evalsave')]
                result.setdefault(step[1], []).append(dict(source=str(file), setup=[s for s in plan[:i] if s[0] not in ('cap', 'evalsave')], inputs=inputs))
    return result


def live(folder):
    manifest = json.loads((folder / 'run.json').read_text(encoding='utf-8'))
    if manifest.get('app') != 'discord-web':
        raise ValueError('live reference must be an actual Discord web run')
    rows = []
    file = folder / 'actions.jsonl'
    lines = file.read_text(encoding='utf-8').splitlines(keepends=True) if file.exists() else []
    for number, line in enumerate(lines, 1):
        if number == len(lines) and not line.endswith('\n'):
            break
        action = json.loads(line)
        input_ = action['input']
        row = dict(id=f'{folder.name}/action-{action["index"]}', family=action['family'], method=input_['method'],
                   title=input_.get('target', {}).get('name') or input_.get('key') or input_['method'],
                   source='live-discord', status=action['status'], parity_verified=False, recipe_status='recorded',
                   setup=action['setup'] if 'setup' in action else [], input=input_, before={}, after={},
                   action_log=dict(path=str(file), line=number), discord_build=manifest.get('discord_build'),
                   blocked_requests=action.get('blocked_requests', []), check=action.get('check', dict(status='unreviewed', name='older recording without outcome check')))
        for side in ('before', 'after'):
            if action.get(side):
                capture = action.get(side + '_capture', 'state-' + action[side])
                row[side] = evidence(folder, capture)
                metadata = folder / (capture + '.json')
                if metadata.is_file():
                    row[side]['metadata'] = str(metadata)
        if not action.get('replay_context_matched'):
            row['status'] = 'reference_context_mismatch'
        elif action['status'] == 'observed_transition':
            if not action.get('input_executed') or not all('aria' in row[side] and 'image' in row[side] and 'metadata' in row[side] for side in ('before', 'after')):
                row['status'] = 'reference_incomplete'
            else:
                row['delta'] = delta(Path(row['before']['aria']).read_text(encoding='utf-8'), Path(row['after']['aria']).read_text(encoding='utf-8'))
        if row['status'] in ('reference_context_mismatch', 'reference_incomplete', 'input_failed') or not action.get('input_executed'):
            row['check'] = dict(status='not_run', name='valid executed input and complete reference evidence required')
        rows.append(row)
    return rows


def report(ref, runs, out, plans=()):
    out.mkdir(parents=True, exist_ok=True)
    rows = archive(ref) if ref.is_dir() else []
    planned = recipes(plans)
    for row in rows:
        if row['method'] == 'archived-input':
            matches = planned.get(row['id'].split('/')[-1] + '_0before', [])
            if matches:
                row['recipe_status'] = 'archived_plan' if len(matches) == 1 else 'ambiguous_archived_plans'
                row['recipes'] = matches
    frontiers, manifests = [], []
    for run in runs:
        rows.extend(live(run))
        manifests.append(json.loads((run / 'run.json').read_text(encoding='utf-8')))
        file = run / 'frontier.json'
        if file.is_file():
            for entry in json.loads(file.read_text(encoding='utf-8')):
                methods = entry['methods']
                status = 'click_unreviewed_or_excluded' if entry['click_policy'] == 'unreviewed-or-excluded' else 'disabled_in_reference' if entry['click_policy'] == 'disabled' else 'click_not_exercised' if not methods.get('click') else 'input_recorded_outcome_unpaired'
                frontiers.append(dict(entry, run=str(run), status=status))
    result = dict(version=1, generated=datetime.now(timezone.utc).isoformat(), scope='Actual Discord reference evidence and unresolved behavior checks. Captured states, menu observations, hover changes and executed inputs are different evidence types. No Rustcord parity is credited.',
                  counts=dict(Counter(row['status'] for row in rows)), source_counts=dict(Counter(row['source'] for row in rows)),
                  reference_checks=dict(Counter(row.get('check', {}).get('status', 'not_declared') for row in rows)),
                  frontier_counts=dict(Counter(row['status'] for row in frontiers)), journeys=rows, frontier=frontiers, runs=manifests)
    parity.write_json(out / 'journeys.json', result)
    relative = lambda value: Path(os.path.relpath(value, out)).as_posix()
    display = json.loads(json.dumps(rows))
    for row in display:
        for side in ('before', 'after'):
            row[side] = {key: relative(value) for key, value in row[side].items()}
        if row.get('action_log'):
            row['action_log']['path'] = relative(row['action_log']['path'])
    payload = json.dumps(dict(rows=display, frontier=frontiers), ensure_ascii=True).replace('<', '\\u003c')
    page = '''<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="30"><title>Discord behavior journeys</title>
<style>body{font:15px system-ui;background:#17171b;color:#eee;margin:28px}a{color:#acc2ff}td,th{padding:9px;text-align:left;vertical-align:top;border-bottom:1px solid #555}td{max-width:550px;overflow-wrap:anywhere}input,select,button{padding:9px;margin:5px}pre{white-space:pre-wrap;max-height:350px;overflow:auto}img{max-width:100%;max-height:400px}summary{cursor:pointer}</style>
<h1>Discord behavior journeys</h1>
<p>Actual Discord defines the reference side. An observed transition records what changed; an input attempt or a menu observation does not establish that an expected behavior worked. Rustcord behavior remains unpaired until the same journey has matching outcome checks.</p>
<p>Saved captures do not have complete replay recipes. Live recordings retain exact setup/input, before/after states, screenshots, accessibility trees and blocked requests. Writes, presence and voice changes are blocked, so server-success states remain untested. The web client does not establish desktop-only parity.</p>
<p><a href="journeys.json">Complete evidence and backlog</a></p><pre id="summary"></pre>
<select id="kind"><option value="journeys">Reference journeys</option><option value="frontier">Discovered control backlog</option></select>
<input id="query" placeholder="Find an action, surface, state, outcome or gap"><button id="prev">Previous</button><button id="next">Next</button><span id="count"></span>
<table><thead><tr><th>Surface / input</th><th>Evidence status</th><th>Reference evidence / unresolved checks</th></tr></thead><tbody id="rows"></tbody></table>
<script>const data=PAYLOAD;const el=id=>document.getElementById(id);el('summary').textContent=SUMMARY;let page=0;const size=40;
function link(parent,path,label){const a=document.createElement('a');a.href=path;a.textContent=label+' ';parent.append(a)}
function draw(){const frontier=el('kind').value==='frontier',q=el('query').value.toLowerCase();const rows=(frontier?data.frontier:data.rows).filter(r=>JSON.stringify(r).toLowerCase().includes(q));page=Math.max(0,Math.min(page,Math.max(0,Math.ceil(rows.length/size)-1)));el('rows').replaceChildren();for(const r of rows.slice(page*size,(page+1)*size)){const tr=document.createElement('tr');for(const value of [r.family+' / '+(r.method||JSON.stringify(r.methods))+' / '+(r.title||r.target.name),r.status]){const td=document.createElement('td');td.textContent=value;tr.append(td)}const td=document.createElement('td');if(!frontier){for(const side of ['before','after']){for(const [kind,path]of Object.entries(r[side]||{}))link(td,path,side+' '+kind)}if(r.action_log)link(td,r.action_log.path,'action line '+r.action_log.line)}const details=document.createElement('details'),summary=document.createElement('summary'),pre=document.createElement('pre');summary.textContent='Input, context, state changes and gaps';pre.textContent=JSON.stringify(r,null,2);details.append(summary,pre);td.append(details);tr.append(td);el('rows').append(tr)}el('count').textContent=rows.length+' entries; page '+(page+1)}
el('query').oninput=el('kind').onchange=()=>{page=0;draw()};el('prev').onclick=()=>{page--;draw()};el('next').onclick=()=>{page++;draw()};draw();</script>'''
    summary = json.dumps(dict(reference_evidence=result['counts'], declared_reference_checks=result['reference_checks'], discovered_control_backlog=result['frontier_counts'],
                              runs=[{key: run.get(key) for key in ('app', 'status', 'actions', 'states', 'queued', 'elapsed_seconds', 'discord_build')} for run in manifests]), indent=2)
    page = page.replace('PAYLOAD', payload).replace('SUMMARY', json.dumps(summary).replace('<', '\\u003c'))
    pending = out / 'index.html.tmp'
    pending.write_text(page, encoding='utf-8')
    pending.replace(out / 'index.html')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ref', type=Path, default=parity.ROOT / 'ref')
    parser.add_argument('--live', type=Path, action='append', default=[])
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--plans', type=Path, action='append', default=[])
    parser.add_argument('--watch', action='store_true')
    args = parser.parse_args()
    while True:
        result = report(args.ref.resolve(), [run.resolve() for run in args.live], args.out.resolve(), [folder.resolve() for folder in args.plans])
        print(json.dumps(dict(counts=result['counts'], frontier=result['frontier_counts'], report=str(args.out / 'index.html'))), flush=True)
        watching = args.watch and not (args.out / 'stop').exists() and any(run.get('status') in ('starting', 'running') for run in result['runs'])
        parity.write_json(args.out / 'watcher.json', dict(pid=os.getpid(), status='watching' if watching else 'complete', generated=result['generated']))
        if not watching:
            break
        time.sleep(30)


if __name__ == '__main__':
    main()
