"""Run and retain a bounded mock-user campaign.

    python e2e/campaign.py --hours 16 --out e2e/artifacts/campaign
    python e2e/campaign.py --resume --out e2e/artifacts/campaign
"""

import argparse
import html
import json
import math
import os
import shutil
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path

import parity


def task(index):
    cycle, slot = divmod(index, 5)
    size = ('940x500', '1280x720', '1707x913')[cycle % 3]
    zoom = (75, 100, 125, 150, 200)[cycle // 3 % 5]
    suite = (f'stress-{cycle + 1}', 'restart', 'tour', 'full', 'bench')[slot]
    return dict(suite=suite, sizes=[f'{size}@{zoom}'] if suite == 'tour' else [])


def new_state(hours):
    return dict(version=1, hours=hours, active_seconds=0, status='running', pid=os.getpid(),
                created=datetime.now(timezone.utc).isoformat(), attempts=[])


def save(root, state):
    state['updated'] = datetime.now(timezone.utc).isoformat()
    parity.write_json(root / 'campaign.json', state)
    rows = []
    passed = failed = actions = 0
    states = set()
    transitions = set()
    observed = set()
    targeted = set()
    for attempt in state['attempts']:
        result = attempt.get('result', {})
        checks = result.get('checks', {})
        passed += sum(ok is True for ok in checks.values())
        failed += sum(ok is not True for ok in checks.values())
        actions += result.get('action_count', 0)
        states.update(result.get('state_ids', []))
        transitions.update(result.get('transition_ids', []))
        observed.update(result.get('observed_controls', []))
        targeted.update(name for name in result.get('targets', []) if name.startswith('control:'))
        folder = attempt['directory']
        status = result.get('status', attempt['status'])
        label = attempt.get('task', {}).get('suite', 'interrupted task')
        link = f'<a href="{folder}/index.html">{html.escape(label)}</a>'
        rows.append(f'<tr><td>{attempt["index"]}</td><td>{link}</td><td>{html.escape(status)}</td><td>{len(checks)}</td><td>{result.get("action_count", 0)}</td></tr>')
    page = '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="30"><title>Rustcord usage campaign</title><style>body{font:16px system-ui;background:#17171b;color:#eee;margin:32px}a{color:#a9beff}td,th{padding:10px;text-align:left;border-bottom:1px solid #555}</style><h1>Rustcord usage campaign</h1>'
    page += f'<p>Status: {html.escape(state["status"])}. Recorded test time: {state["active_seconds"] / 3600:.2f} / {state["hours"]:g} hours.</p>'
    page += f'<p>Native assertions: {passed} passed; {failed} failed or unreached. Recorded exploratory actions: {actions}.</p>'
    page += f'<p>Observed UI fingerprints: {len(states)}; observed state/action transitions: {len(transitions)}. Fingerprints describe instrumented controls and routes, not every visual or semantic state.</p>'
    page += f'<p>Shared control markers observed: {len(observed)}; pointer-targeted: {len(targeted)}; observed but not pointer-targeted: {len(observed - targeted)}. A targeted control is not necessarily functionally verified.</p>'
    page += '<details><summary>Observed markers without a pointer action yet</summary><pre>' + html.escape('\n'.join(sorted(observed - targeted))) + '</pre></details>'
    page += '<p>These are mock-client checks and sampled action sequences. They do not prove years of semantic behavior or all Discord interactions. Repeated passes do not increase reference coverage.</p><p><a href="coverage/index.html">Reference coverage gaps</a> · <a href="campaign.json">Raw campaign state</a></p>'
    page += '<table><tr><th>Task</th><th>Evidence</th><th>Outcome</th><th>Assertions</th><th>Actions</th></tr>' + ''.join(rows) + '</table>'
    (root / 'index.html').write_text(page, encoding='utf-8')


def perform(root, spec):
    result = parity.native_suite(spec['suite'], root, spec['sizes'])
    actions = []
    for log in root.rglob('actions.jsonl'):
        actions += [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines() if line.strip()]
    result['action_count'] = sum('action' in event for event in actions)
    result['action_families'] = sorted({key for event in actions for key in event.get('action', {}) if key != 'at'})
    result['targets'] = sorted({event['action']['target'] for event in actions if event.get('action', {}).get('target')})
    result['observed_controls'] = sorted({name for event in actions for name in event.get('visible', []) if name.startswith('control:')})
    result['state_ids'] = sorted({event['before'] for event in actions if event.get('before')})
    edges = set()
    for previous, event in zip(actions, actions[1:]):
        if previous.get('before') and event.get('before') and previous.get('action'):
            action = {key: value for key, value in previous['action'].items() if key != 'at'}
            edges.add(json.dumps([previous['before'], action, event['before']], sort_keys=True))
    result['transition_ids'] = sorted(edges)
    parity.report(root, dict(scope='Mock-client task. See the campaign reference coverage for unresolved Discord comparisons.', native=[result], cases=[]))
    return result


def execute(root, state):
    for attempt in state['attempts']:
        if attempt['status'] == 'running':
            attempt['status'] = 'interrupted'
    state.update(status='running', pid=os.getpid())
    save(root, state)
    while state['active_seconds'] < state['hours'] * 3600:
        if (root / 'stop').exists():
            state['status'] = 'stopped'
            break
        if shutil.disk_usage(root).free < 2**30:
            state.update(status='blocked', error='less than 1 GiB free; retained evidence was not removed')
            break
        index = sum(a['status'] == 'completed' for a in state['attempts'])
        spec = task(index)
        folder = f'{index:05}-{uuid.uuid4().hex[:8]}'
        out = root / folder
        out.mkdir()
        attempt = dict(index=index, directory=folder, task=spec, status='running')
        state['attempts'].append(attempt)
        parity.report(out, dict(scope='Task in progress. Raw logs and captures are retained under native/.', native=[dict(name=spec['suite'], status='running')], cases=[]))
        save(root, state)
        began = time.monotonic()
        try:
            attempt['result'] = perform(out, spec)
        except Exception as error:
            attempt['result'] = dict(name=spec['suite'], status='failed', error=str(error))
            parity.report(out, dict(scope='Task failed before all checks were available.', native=[attempt['result']], cases=[]))
        attempt['elapsed_seconds'] = time.monotonic() - began
        state['active_seconds'] += attempt['elapsed_seconds']
        attempt['status'] = 'completed'
        save(root, state)
        recent = state['attempts'][-3:]
        if len(recent) == 3 and all(a.get('result', {}).get('error') and not a['result'].get('checks') for a in recent):
            state.update(status='blocked', error='three consecutive infrastructure failures; inspect task reports before resuming')
            break
    else:
        state['status'] = 'budget_finished'
    save(root, state)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--hours', type=float, default=16)
    parser.add_argument('--resume', action='store_true')
    args = parser.parse_args()
    if not math.isfinite(args.hours) or args.hours <= 0:
        parser.error('hours must be finite and positive')
    root = args.out.resolve()
    root.mkdir(parents=True, exist_ok=args.resume)
    import msvcrt

    with (root / 'controller.lock').open('a+b') as lock:
        if lock.tell() == 0:
            lock.write(b'0')
            lock.flush()
        lock.seek(0)
        msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        try:
            if args.resume:
                state = json.loads((root / 'campaign.json').read_text(encoding='utf-8'))
            else:
                state = new_state(args.hours)
                coverage = root / 'coverage'
                coverage.mkdir()
                plan = parity.inventory(parity.ROOT / 'ref')
                parity.write_json(coverage / 'plan.json', plan)
                parity.report(coverage, dict(scope=plan['scope'], native=[], cases=[dict(id=c['id'], title=c.get('title', c['id']), kind=c['kind'], status='untested', reason='No matching paired evidence yet') for c in plan['cases']]))
            execute(root, state)
        finally:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)


if __name__ == '__main__':
    main()
