"""Record native checks and compare paired Discord evidence.

    python e2e/parity.py init --out e2e/artifacts/parity-plan.json
    python e2e/parity.py run --plan e2e/artifacts/parity-plan.json
    python e2e/parity.py compare --plan plan.json --candidate capture-folder
"""

import argparse
import bisect
import hashlib
import html
import json
import math
import os
import platform
import re
import subprocess
import sys
import time
import uuid
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ENV_KEYS = ('os', 'dpi', 'zoom', 'theme', 'viewport', 'scenario', 'input')


def write_json(path, value):
    pending = path.with_suffix(path.suffix + '.tmp')
    pending.write_text(json.dumps(value, indent=2, allow_nan=False), encoding='utf-8')
    pending.replace(path)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def inventory(ref):
    cases = {}
    index = ref / 'INDEX.md'
    if index.exists():
        for title, name in re.findall(r'^### \[([^\]]+)\]\(([^)]+\.png)\)', index.read_text(encoding='utf-8'), re.M):
            key = 'atlas:' + name
            cases[key] = dict(id=key, kind='image', title=title, reference=str((ref / name).resolve()), candidate=None)
    qol = ref / 'QOL.md'
    if qol.exists():
        section = ''
        for number, line in enumerate(qol.read_text(encoding='utf-8').splitlines(), 1):
            if line.startswith('## '):
                section = line[3:]
            if section and (re.match(r'^\d+\.', line) or line.startswith('| ') and not line.startswith('| -')):
                key = f'qol:{number}'
                cases[key] = dict(id=key, kind='manual', title=f'{section}: {line}', source=f'{qol.resolve()}:{number}')
    for key, title in {
        'clipboard-history': 'Select text and image using native Win+V; attach exactly once',
        'touchpad': 'Native touchpad scroll distance, inertia, reversal and stopping',
        'wheel': 'Native wheel displacement over time, including direction reversal',
        'focus': 'Focus, keyboard navigation, Escape and modal dismissal',
        'tray': 'Minimize, tray, notifications and restoration',
        'reconnect': 'Connection loss, reconnect and state recovery',
        'frame-pacing': 'Input latency, frame stalls and animation progression',
    }.items():
        cases['native:' + key] = dict(id='native:' + key, kind='manual', title=title)
    return dict(version=1, scope='Local reference atlas, QoL inventory and native interaction gaps; not all possible Discord states', cases=list(cases.values()))


def load_plan(path):
    plan = json.loads(path.read_text(encoding='utf-8'))
    if plan.get('version') != 1 or not isinstance(plan.get('cases'), list):
        raise ValueError('plan needs version 1 and a cases array')
    ids = [case['id'] for case in plan['cases']]
    if len(ids) != len(set(ids)):
        raise ValueError('duplicate case ids')
    if any(case.get('kind') not in ('image', 'metric', 'trace', 'manual') for case in plan['cases']):
        raise ValueError('case kind must be image, metric, trace or manual')
    return plan


def number(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError('expected a finite number')
    return value


def limit(case, key, default=0):
    value = number(case.get(key, default))
    if value < 0:
        raise ValueError(f'{key} must be nonnegative')
    return value


def value_at(path, key):
    value = json.loads(path.read_text(encoding='utf-8'))
    for part in key.split('.') if key else []:
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def trace(value):
    if not isinstance(value, list) or len(value) < 2:
        raise ValueError('trace needs at least two [time_ms, value] samples')
    for sample in value:
        if not isinstance(sample, list) or len(sample) != 2:
            raise ValueError('trace samples must be [time_ms, value]')
        for item in sample:
            number(item)
    if any(a[0] >= b[0] for a, b in zip(value, value[1:])):
        raise ValueError('trace times must increase')
    return value


def interpolate(samples, at):
    i = max(1, min(bisect.bisect_left(samples, at, key=lambda sample: sample[0]), len(samples) - 1))
    a, b = samples[i - 1:i + 1]
    return a[1] + (b[1] - a[1]) * (at - a[0]) / (b[0] - a[0])


def images(case, reference, candidate, out):
    from PIL import Image, ImageChops, ImageDraw

    with Image.open(reference) as source:
        a = source.convert('RGBA')
    with Image.open(candidate) as source:
        b = source.convert('RGBA')
    a.save(out / 'reference.png')
    b.save(out / 'candidate.png')
    result = dict(reference='reference.png', candidate='candidate.png', reference_size=list(a.size), candidate_size=list(b.size))
    if a.size != b.size:
        return dict(result, status='different', reason='image dimensions differ; no resizing applied')
    mask = Image.new('L', a.size, 255)
    draw = ImageDraw.Draw(mask)
    for x, y, w, h in case.get('masks', []):
        if any(not isinstance(n, int) for n in (x, y, w, h)) or min(x, y) < 0 or min(w, h) <= 0 or x + w > a.width or y + h > a.height:
            raise ValueError('mask is outside the image or has invalid dimensions')
        draw.rectangle((x, y, x + w - 1, y + h - 1), fill=0)
    compared = mask.histogram()[255]
    if compared == 0:
        raise ValueError('masks exclude every pixel')
    delta = ImageChops.difference(a, b)
    channels = delta.split()
    largest = channels[0]
    for channel in channels[1:]:
        largest = ImageChops.lighter(largest, channel)
    tolerance = limit(case, 'pixel_tolerance')
    budget = limit(case, 'max_changed_fraction')
    if tolerance > 255 or budget > 1:
        raise ValueError('pixel tolerance must be <=255 and changed fraction <=1')
    changed = ImageChops.multiply(largest.point(lambda n: 255 if n > tolerance else 0), mask)
    count = changed.histogram()[255]
    overlay = b.convert('RGB')
    overlay.paste((255, 0, 80), mask=changed)
    overlay.save(out / 'diff.png')
    fraction = count / compared
    return dict(result, status='verified' if fraction <= budget else 'different', diff='diff.png',
                changed_pixels=count, compared_pixels=compared, changed_fraction=fraction, masked_pixels=a.width * a.height - compared)


def compare(case, base, candidate_root, out, index):
    result = dict(id=case['id'], title=case.get('title', case['id']), kind=case['kind'], status='untested')
    if case['kind'] == 'manual':
        return dict(result, reason='requires a reproducible native-input test and paired evidence')
    if not case.get('reference') or not case.get('candidate'):
        return dict(result, reason='no reference/candidate pairing')
    a, b = base / case['reference'], candidate_root / case['candidate']
    if not a.is_file() or not b.is_file():
        return dict(result, reason='missing evidence', missing=[str(p) for p in (a, b) if not p.is_file()])
    if a.samefile(b):
        return dict(result, status='error', reason='reference and candidate are the same file')
    ae, be = case.get('reference_env', {}), case.get('candidate_env', {})
    if any(ae.get(k) is None or be.get(k) is None for k in ENV_KEYS):
        return dict(result, reason='missing capture conditions', required=list(ENV_KEYS))
    if any(ae[k] != be[k] for k in ENV_KEYS):
        return dict(result, reason='capture conditions differ', reference_env=ae, candidate_env=be)
    folder = out / f'case-{index:05}'
    folder.mkdir(exist_ok=True)
    try:
        details = dict(reference_env=ae, candidate_env=be, reference_sha256=digest(a), candidate_sha256=digest(b))
        if case['kind'] == 'image':
            details.update(images(case, a, b, folder))
        elif case['kind'] in ('metric', 'trace'):
            reference = value_at(a, case.get('key', ''))
            candidate = value_at(b, case.get('key', ''))
            if case['kind'] == 'metric':
                error = abs(number(reference) - number(candidate))
            else:
                aa, bb = trace(reference), trace(candidate)
                if aa[0][0] != bb[0][0] or aa[-1][0] != bb[-1][0]:
                    raise ValueError('trace time ranges differ; align to the same input window')
                times = sorted({s[0] for s in aa + bb})
                error = max(abs(interpolate(aa, t) - interpolate(bb, t)) for t in times)
            write_json(folder / 'reference.json', reference)
            write_json(folder / 'candidate.json', candidate)
            details.update(reference='reference.json', candidate='candidate.json', max_error=error,
                           status='verified' if error <= limit(case, 'tolerance') else 'different')
        else:
            raise ValueError(f'unknown comparison kind: {case["kind"]}')
        for key in ('reference', 'candidate', 'diff'):
            if key in details:
                details[key] = f'{folder.name}/{details[key]}'
        return dict(result, **details)
    except (OSError, ValueError, KeyError, TypeError, IndexError) as error:
        return dict(result, status='error', reason=str(error))


def report(out, data):
    data['coverage'] = {k: 0 for k in ('verified', 'different', 'untested', 'error')}
    data['coverage'].update(Counter(c['status'] for c in data['cases']))
    assertions = []
    areas = {}
    for suite in data['native']:
        for name, ok in suite.get('checks', {}).items():
            label = name
            if name.startswith(('cold: ', 'warm: ')):
                label = name.split(': ', 1)[1]
            area = label.split(':', 1)[0] if ':' in label else suite['name']
            status = 'passed' if ok is True else 'failed_or_unreached'
            assertions.append(dict(suite=suite['name'], area=area, name=name, status=status))
            counts = areas.setdefault(area, dict(passed=0, failed_or_unreached=0))
            counts[status] += 1
    data['native_coverage'] = dict(assertions=len(assertions), passed=sum(a['status'] == 'passed' for a in assertions),
                                   failed_or_unreached=sum(a['status'] != 'passed' for a in assertions), areas=areas)
    write_json(out / 'assertions.json', assertions)
    write_json(out / 'report.json', data)
    rows = []
    for case in data['cases']:
        links = ' '.join(f'<a href="{html.escape(case[k], quote=True)}">{k}</a>' for k in ('reference', 'candidate', 'diff') if case.get(k))
        detail = case.get('reason', '')
        if 'changed_fraction' in case:
            detail = f'{case["changed_pixels"]}/{case["compared_pixels"]} pixels differ ({case["changed_fraction"]:.2%}); {case["masked_pixels"]} masked'
        elif 'max_error' in case:
            detail = f'maximum absolute error: {case["max_error"]:g}'
        title = html.escape(case.get('title', case['id']))
        rows.append(f'<tr><td>{html.escape(case["status"])}</td><td>{html.escape(case["id"])}<br>{title}</td><td>{html.escape(detail)}</td><td>{links}</td></tr>')
    native = []
    for suite in data['native']:
        log = 'build.log' if suite['name'] == 'build' else f'native/{suite["name"]}/log.txt'
        failures = ''.join(f'<p>{html.escape(tag)} stopped: {html.escape(str(run["error"]))}</p>' for tag, run in suite.get('runs', {}).items() if run.get('error'))
        native.append(f'<h3>{html.escape(suite["name"])}: {html.escape(suite["status"])}</h3><p><a href="{log}">log</a></p>{failures}<details><summary>Raw measurements and run metadata</summary><pre>{html.escape(json.dumps(suite, indent=2))}</pre></details>')
    page = '<!doctype html><meta charset="utf-8"><title>Rustcord parity audit</title><style>body{font:15px system-ui;margin:32px;background:#17171b;color:#eee}a{color:#a9beff}td,th{padding:8px;border-bottom:1px solid #555;text-align:left}pre{white-space:pre-wrap}input{padding:10px;width:60%}</style>'
    page += '<h1>Rustcord parity audit</h1><p>Coverage applies only to the recorded plan. Mock checks are not Discord parity. Capture conditions are declared in the plan; this tool cannot independently attest them.</p>'
    page += f'<p>{html.escape(data["scope"])}</p><pre>{html.escape(json.dumps(data["coverage"], indent=2))}</pre>'
    page += '<input id="filter" placeholder="Filter areas, assertions, states or statuses">' + ''.join(native)
    page += '<h2>Native assertions by area</h2><p>False assertions can be caused by an earlier stopped step. Run errors above identify the first reported stop; these counts are not counts of independent bugs.</p><table><thead><tr><th>Area</th><th>Passed</th><th>Failed or unreached</th></tr></thead><tbody>'
    page += ''.join(f'<tr><td>{html.escape(area)}</td><td>{counts["passed"]}</td><td>{counts["failed_or_unreached"]}</td></tr>' for area, counts in sorted(areas.items())) + '</tbody></table>'
    page += f'<h2>Individual native assertions ({len(assertions)})</h2><p><a href="assertions.json">Export assertions</a></p><table><thead><tr><th>Status</th><th>Suite</th><th>Area</th><th>Assertion</th></tr></thead><tbody>'
    page += ''.join('<tr>' + ''.join(f'<td>{html.escape(a[k])}</td>' for k in ('status', 'suite', 'area', 'name')) + '</tr>' for a in assertions) + '</tbody></table>'
    page += '<h2>Paired Discord comparisons</h2><table><thead><tr><th>Status</th><th>State</th><th>Reason</th><th>Evidence</th></tr></thead><tbody>' + ''.join(rows) + '</tbody></table>'
    shots = sorted((out / 'native').rglob('*.png'))
    page += f'<h2>Native screenshots ({len(shots)})</h2>'
    for shot in shots:
        name = html.escape(shot.relative_to(out).as_posix(), quote=True)
        page += f'<details><summary>{name}</summary><a href="{name}"><img loading="lazy" style="max-width:100%" src="{name}"></a></details>'
    page += '<script>document.querySelector("#filter").oninput=e=>{for(const row of document.querySelectorAll("tbody tr"))row.hidden=!row.textContent.toLowerCase().includes(e.target.value.toLowerCase())}</script>'
    (out / 'index.html').write_text(page, encoding='utf-8')
    md = ['# Rustcord parity audit', '', data['scope'], '', 'Mock checks are not Discord parity.', '', f'Coverage: {data["coverage"]}', '']
    md += [f'- {s["name"]}: {s["status"]} ({len(s.get("checks", {}))} checks)' for s in data['native']]
    md += ['', f'Native assertions: {data["native_coverage"]["passed"]} passed, {data["native_coverage"]["failed_or_unreached"]} failed or unreached.', '', '| Area | Passed | Failed or unreached |', '| --- | ---: | ---: |']
    md += [f'| {area} | {counts["passed"]} | {counts["failed_or_unreached"]} |' for area, counts in sorted(areas.items())]
    md += ['', '[Interactive report](index.html)', '[Machine-readable evidence](report.json)']
    (out / 'report.md').write_text('\n'.join(md) + '\n', encoding='utf-8')


def worker(name, folder, sizes):
    import run

    run.ART = folder
    if name == 'bench':
        sys.argv = ['bench.py', '--artifacts', str(folder / 'capture')]
        import bench

        bench.main()
        return dict(measurements=json.loads((folder / 'capture/bench.json').read_text(encoding='utf-8')), evidence='capture/bench.json')
    if name == 'full':
        run.ART = folder / 'capture'
        sys.argv = ['run.py', '--no-build']
        try:
            run.main()
        except SystemExit as end:
            code = end.code
        else:
            code = 0
        result = json.loads((run.ART / 'report.json').read_text(encoding='utf-8'))
        return dict(checks=result['checks'], exit_code=code, evidence='capture/report.json', runs={tag: result[tag] for tag in ('cold', 'warm', 'login') if tag in result})
    if name == 'tour':
        import tour

        sys.argv = ['tour.py']
        checks = {}
        for spec in sizes:
            good, _ = tour.sweep(folder, spec)
            checks[spec] = good
        return dict(checks=checks)
    log = folder / 'mock_requests.jsonl'
    mock_env = dict(os.environ, MOCK_SETTINGS_LIVE='1') if name == 'restart' else dict(os.environ)
    mock = subprocess.Popen([str(run.ROOT / 'target/release/examples/mockcord.exe'), str(run.HTTP), str(run.WS), str(log), str(run.VOICE), str(run.REMOTE)], env=mock_env)
    try:
        time.sleep(.5)
        fake = dict(RUSTCORD_FAKE_MIC='1', RUSTCORD_FAKE_CAM='1', RUSTCORD_FAKE_VIDEO='1', RUSTCORD_SILENT='1')
        if name.startswith('stress-'):
            seed = int(name.removeprefix('stress-'))
            res = run.run_client('explore', folder / 'data', dict(fake, RUSTCORD_E2E_FROM='9000', RUSTCORD_STRESS_SEED=str(seed), RUSTCORD_STRESS_ACTIONS='800'))
            completed = res.get('ok') is True and res.get('exit_code') == 0 and 'stress_completed' in res.get('marks_ms', {}) and res.get('rects', {}).get('stress_actions') == 800
            checks = {'800 exploratory actions completed without a reported client failure': completed,
                      'exploration recorded UI states and state/action visits': bool(res.get('rects', {}).get('stress_states')) and bool(res.get('rects', {}).get('stress_visits'))}
            return dict(checks=checks, runs={'explore': res}, exploration=True, seed=seed)
        if name == 'restart':
            first = run.run_client('change', folder / 'data', dict(fake, RUSTCORD_E2E_FROM='5000', RUSTCORD_P4B='keep'))
            second = run.run_client('verify', folder / 'data', dict(fake, RUSTCORD_E2E_FROM='5040', RUSTCORD_P4B='verify'))
            checks = {'settings changed through controls': 'p4b_changed' in first.get('marks_ms', {}),
                      'settings survived client restart': 'p4b_stuck' in second.get('marks_ms', {})}
            checks.update({f'{tag} finished': r.get('ok') is True and r.get('exit_code') == 0 for tag, r in (('change', first), ('verify', second))})
            return dict(checks=checks, runs={'change': first, 'verify': second})
        import profed

        runs = {}
        for tag in ('cold', 'warm'):
            runs[tag] = run.run_client(tag, folder / 'data', dict(RUSTCORD_E2E_FROM='3900', RUSTCORD_E2E_TO='1', RUSTCORD_FAKE_MIC='1', RUSTCORD_FAKE_CAM='1', RUSTCORD_FAKE_VIDEO='1', RUSTCORD_SILENT='1'))
        reqs = [json.loads(line) for line in log.read_text(encoding='utf-8').splitlines() if line.strip()]
        checks = {f'{tag}: {key}': ok for tag, res in runs.items() for key, ok in profed.checks({'cold': res}, reqs, folder / 'data').items()}
        checks.update({f'{tag} finished': r.get('ok') is True and r.get('exit_code') == 0 for tag, r in runs.items()})
        return dict(checks=checks, runs=runs)
    finally:
        mock.kill()
        mock.wait()


def binaries():
    return {str(path.relative_to(ROOT)): dict(sha256=digest(path), bytes=path.stat().st_size) if path.is_file() else None
            for path in (ROOT / 'target/release/rustcord.exe', ROOT / 'target/release/examples/mockcord.exe')}


def native_suite(name, out, sizes):
    folder = out / 'native' / name
    folder.mkdir(parents=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('RUSTCORD_', 'MOCK_'))}
    before = binaries()
    began = time.monotonic()
    with (folder / 'log.txt').open('w', encoding='utf-8') as log:
        proc = subprocess.run([sys.executable, str(Path(__file__).resolve()), '_worker', name, str(folder), *sizes], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
    path = folder / 'result.json'
    result = json.loads(path.read_text(encoding='utf-8')) if path.exists() else dict(error='worker exited without results')
    checks = result.get('checks', {})
    changed = before != binaries()
    suite_code = result.get('exit_code', 0)
    result.update(name=name, exit_code=proc.returncode, elapsed_seconds=round(time.monotonic() - began, 2), binaries=before,
                  status='passed' if proc.returncode == 0 and suite_code == 0 and checks and all(v is True for v in checks.values()) and not changed else 'failed')
    if name == 'bench' and proc.returncode == 0 and result.get('measurements') and not changed:
        result['status'] = 'recorded'
    if result.get('exploration') and result['status'] == 'passed':
        result['status'] = 'recorded'
    if changed:
        result['error'] = 'binaries changed during the run; results cannot identify one build'
    write_json(path, result)
    return result


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '_worker':
        folder = Path(sys.argv[3])
        try:
            result = worker(sys.argv[2], folder, sys.argv[4:])
            write_json(folder / 'result.json', result)
        except Exception as error:
            write_json(folder / 'result.json', dict(error=str(error)))
            raise
        return
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('action', choices=('init', 'run', 'compare'))
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--ref', type=Path, default=ROOT / 'ref')
    parser.add_argument('--candidate', type=Path, help='root for candidate paths in the plan; defaults to this audit folder')
    parser.add_argument('--out', type=Path)
    parser.add_argument('--suite', action='append', choices=('full', 'tour', 'profile', 'bench'), help='repeat to select suites; default: full, tour and bench')
    parser.add_argument('--size', action='append', help='tour WxH@zoom; repeat for multiple sizes')
    parser.add_argument('--build', action='store_true', help='build locked release client and mock before testing')
    args = parser.parse_args()
    plan = load_plan(args.plan.resolve()) if args.plan else inventory(args.ref.resolve())
    if args.action == 'init':
        if not args.out:
            parser.error('init requires --out')
        args.out.parent.mkdir(parents=True, exist_ok=True)
        with args.out.open('x', encoding='utf-8') as stream:
            json.dump(plan, stream, indent=2)
        print(f'{len(plan["cases"])} states: {args.out}')
        return
    sizes = args.size or ['1280x720', '940x500', '1707x913']
    if any(not re.fullmatch(r'[1-9]\d*x[1-9]\d*(?:@[1-9]\d*)?', s) for s in sizes):
        parser.error('size must be WxH or WxH@zoom')
    out = (args.out or ROOT / 'e2e/artifacts/parity' / (datetime.now().strftime('%Y%m%d-%H%M%S-') + uuid.uuid4().hex[:8])).resolve()
    out.mkdir(parents=True, exist_ok=False)
    write_json(out / 'plan.json', plan)
    git = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True)
    dirty = subprocess.run(['git', 'status', '--porcelain'], cwd=ROOT, capture_output=True, text=True)
    data = dict(version=1, started=datetime.now(timezone.utc).isoformat(), scope=plan.get('scope', 'Only states declared in the supplied plan'),
                host=platform.platform(), git_head=git.stdout.strip(), worktree=dirty.stdout.splitlines(), binaries=binaries(),
                native=[], cases=[dict(id=c['id'], kind=c['kind'], status='untested', reason='not evaluated yet') for c in plan['cases']])
    report(out, data)
    print(f'Artifacts: {out}', flush=True)
    if args.action == 'run':
        if args.build:
            with (out / 'build.log').open('w', encoding='utf-8') as log:
                build = subprocess.run(['cargo', 'build', '--locked', '--release', '--bin', 'rustcord', '--example', 'mockcord'], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            if build.returncode:
                data['native'].append(dict(name='build', status='failed', exit_code=build.returncode))
                report(out, data)
                return 1
            data['binaries'] = binaries()
        for name in dict.fromkeys(args.suite or ['full', 'tour', 'bench']):
            print(f'Running {name}', flush=True)
            data['native'].append(dict(name=name, status='running'))
            report(out, data)
            data['native'][-1] = native_suite(name, out, sizes)
            report(out, data)
    base = args.plan.resolve().parent if args.plan else ROOT
    candidate = args.candidate.resolve() if args.candidate else out
    for i, case in enumerate(plan['cases']):
        data['cases'][i] = compare(case, base, candidate, out, i)
    data['finished'] = datetime.now(timezone.utc).isoformat()
    report(out, data)
    print(json.dumps(dict(coverage=data['coverage'], native={s['name']: s['status'] for s in data['native']})))
    if any(s['status'] not in ('passed', 'recorded') for s in data['native']) or any(data['coverage'][k] for k in ('different', 'error')):
        return 1
    return 2 if data['coverage']['untested'] or not data['cases'] else 0


if __name__ == '__main__':
    sys.exit(main())
