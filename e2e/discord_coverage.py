"""Match explicit Discord feature bindings to recorded Rustcord evidence."""

import fnmatch
import html
import json
from collections import Counter
from pathlib import Path

import parity


def load(path):
    data = json.loads(path.read_text(encoding='utf-8'))
    if data.get('version') != 1 or not isinstance(data.get('features'), list):
        raise ValueError('Discord bindings need version 1 and a features array')
    ids = [row['id'] for row in data['features']]
    if len(ids) != len(set(ids)):
        raise ValueError('duplicate Discord feature ids')
    return data['features']


def matrix(data, bindings):
    controls = {row['id']: row for row in data['controls']}
    views = {row['id']: row for row in data['views']}
    ref_controls = data['reference_controls']
    counts = Counter(row['reference_view'] for row in ref_controls)
    reference = {row['id']: dict(id=row['id'], title=row['title'], bindings=[], control_bindings=0,
                                captured_control_instances=counts[row['id']]) for row in data['reference_views']}
    bound = set()
    features = []
    normalize = lambda text: ' '.join(text.casefold().split())
    for binding in bindings:
        selector, target = binding['reference'], binding['rustcord']
        selected = [key for key in reference if fnmatch.fnmatchcase(key, selector['view'])]
        nodes = []
        row = dict(binding, status='not_exercised', parity_verified=False, reference_views=selected,
                   reference_controls=[], rustcord_evidence=[], attempts=0)
        if binding.get('mapping_status') == 'candidate':
            row['status'] = 'mapping_candidate'
            features.append(row)
            continue
        if 'control' in target:
            nodes = [node for node in ref_controls if node['reference_view'] in selected
                     and normalize(node['title']) == normalize(selector['label'])
                     and (not selector.get('role') or node['role'] == selector['role'])
                     and (not selector.get('node') or node['id'] == selector['node'])]
            row['reference_controls'] = [node['id'] for node in nodes]
            ambiguous = any(n > 1 for n in Counter(node['reference_view'] for node in nodes).values())
            if not nodes:
                row['status'] = 'reference_missing'
            elif ambiguous:
                row['status'] = 'ambiguous_reference'
            else:
                source = controls.get(target['control'], {})
                method = target['method']
                row['attempts'] = source.get('methods', {}).get(method, 0)
                row['rustcord_evidence'] = [e for e in source.get('evidence', []) if isinstance(e, str) or e.get('event') == method]
                row['status'] = 'input_attempted_in_mock' if row['attempts'] else 'seen_not_exercised' if source.get('observations') else 'not_exercised'
                if target.get('required_visible'):
                    proof = data.get('scoped_inputs', {}).get(binding['id'], {})
                    row['attempts'] = proof.get('attempts', 0)
                    row['rustcord_evidence'] = proof.get('evidence', [])
                    row['status'] = 'input_attempted_in_mock' if row['attempts'] else 'context_not_exercised'
                if any(node.get('disabled') for node in nodes):
                    row['status'] = 'reference_state_not_matched'
                for node in nodes:
                    if node['id'] not in bound:
                        reference[node['reference_view']]['control_bindings'] += 1
                        bound.add(node['id'])
                selected = list(dict.fromkeys(node['reference_view'] for node in nodes))
                row['reference_views'] = selected
        elif not selected:
            row['status'] = 'reference_missing'
        else:
            source = views.get(target['view'], {})
            row['rustcord_evidence'] = source.get('evidence', [])
            if source.get('status') == 'captured':
                row['status'] = 'rustcord_view_captured'
        for key in selected:
            reference[key]['bindings'].append(row['id'])
        features.append(row)
    return dict(version=1, scope='Explicit bindings against the saved real Discord atlas; Rustcord exercise evidence uses the mock server. No behavior or visual parity is inferred.',
                features=features, reference_views=list(reference.values()),
                unmapped_reference_views=sum(not row['bindings'] for row in reference.values()),
                unbound_reference_control_instances=len(ref_controls) - len(bound))


def render(out, result, data, relative):
    parity.write_json(out / 'discord.json', result)
    refs = {row['id']: row for row in data['reference_views']}
    ref_index = {row['id']: i for i, row in enumerate(data['reference_views'])}
    rows = []
    for row in result['features']:
        reference_links = ' '.join(f'<a href="reference/view-{ref_index[key]:05}.html">{html.escape(refs[key]["title"])}</a>' for key in row['reference_views'])
        proof = []
        for item in row['rustcord_evidence'][:3]:
            path = item if isinstance(item, str) else item['path']
            if Path(path).is_file():
                label = 'capture' if isinstance(item, str) else f'action line {item["line"]}'
                proof.append(f'<a href="{relative(path, out)}">{label}</a>')
        target = row['rustcord'].get('control', row['rustcord'].get('view', ''))
        method = row['rustcord'].get('method', 'view capture')
        required = row['rustcord'].get('required_visible', [])
        context = '<details><summary>Required action context</summary>' + html.escape(', '.join(required)) + '</details>' if required else ''
        rows.append(f'<tr><td>{html.escape(row["title"])}</td><td>{html.escape(row["status"])}</td><td>{reference_links}</td><td>{html.escape(target)}<br>{html.escape(method)}; {row["attempts"]} pointer attempts<br>{" ".join(proof)}{context}</td></tr>')
    page = '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="60"><title>Discord feature and interaction coverage</title><style>body{font:15px system-ui;background:#17171b;color:#eee;margin:30px}a{color:#a9beff}td,th{padding:10px;vertical-align:top;text-align:left;border-bottom:1px solid #555}td{max-width:600px;overflow-wrap:anywhere}input{padding:10px;width:65%}</style>'
    page += '<h1>Discord feature and interaction coverage</h1><p>Actual Discord captures define the reference side. Explicit bindings identify the corresponding Rustcord surface or input. This report does not drive live Discord.</p>'
    page += '<p><b>input_attempted_in_mock</b> means the named input was attempted in Rustcord against the mock; it does not prove the outcome matched Discord. <b>rustcord_view_captured</b> covers a view capture, not every control or substate on that page. <b>ambiguous_reference</b> and <b>reference_missing</b> require more reference context.</p>'
    page += '<p>Unexercised statuses mean this map has no matching recorded input evidence, not that the feature is absent. Engine-level or scripted checks are separate evidence and are not automatically credited as UI clicks. View bindings identify a surface family; matching scroll positions, content and behavior remain unverified.</p>'
    page += '<p>Context-scoped inputs count only when every required marker was visible in that exact action record. Mapping candidates never receive coverage credit. Menu-family bindings do not establish every ownership, permission or channel-type variant; checked states and behavioral outcomes remain unverified.</p>'
    page += f'<p>{result["unmapped_reference_views"]} captured reference views have no explicit binding; {result["unbound_reference_control_instances"]} reference control instances remain unbound. These are inventory gaps, not counts of missing features. No whole-Discord coverage percentage is claimed.</p>'
    page += '<p><a href="discord.json">Bindings and evidence data</a> · <a href="index.html">Surface inventory and action details</a></p><input id="filter" placeholder="Find a feature, status, reference or Rustcord identifier">'
    page += '<h2>Explicit feature mappings</h2><table><thead><tr><th>Discord feature / interaction</th><th>Exercise status</th><th>Actual Discord reference</th><th>Rustcord evidence</th></tr></thead><tbody>' + ''.join(rows) + '</tbody></table>'
    page += '<h2>All captured Discord views</h2><p>A binding on a view does not cover its remaining controls.</p><table><thead><tr><th>Reference state</th><th>Feature bindings</th><th>Control candidates bound / captured</th></tr></thead><tbody>'
    for row in result['reference_views']:
        page += f'<tr><td><a href="reference/view-{ref_index[row["id"]]:05}.html">{html.escape(row["title"])}</a></td><td>{html.escape(", ".join(row["bindings"]) or "unmapped")}</td><td>{row["control_bindings"]} / {row["captured_control_instances"]}</td></tr>'
    page += '</tbody></table><script>document.getElementById("filter").oninput=e=>{const q=e.target.value.toLowerCase();for(const row of document.querySelectorAll("tbody tr"))row.hidden=!row.textContent.toLowerCase().includes(q)}</script>'
    pending = out / 'discord.html.tmp'
    pending.write_text(page, encoding='utf-8')
    pending.replace(out / 'discord.html')
