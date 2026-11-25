"""The shared commit hooks' checks, run by CI over every commit a push or pull request brings in, so
commits made without the hooks are held to the same rules: no AI attribution, no test reports or local
paths in messages, short messages, no source file growing past 1000 lines. A commit that edits existing
tests alongside the code they check only warns.

    python tools/gates.py origin/master..HEAD
"""

import fnmatch
import re
import subprocess
import sys

MAX = 1000

ATTRIBUTION = re.compile(
    r'^co-authored-by:.*(claude|anthropic|codex|openai|chatgpt|gpt-|copilot|cursor|gemini|grok)'
    r'|generated (with|by) .*(claude|codex|cursor|copilot|chatgpt|gemini|grok)', re.I | re.M)
REPORT = re.compile(
    r'^\s*[-*]?\s*(checks?|tests?|verified|verification|validation|e2e|ran)\b[^:./]{0,40}:'
    r'|\b(tests?|checks?|fixtures?|suites?|specs?|build|lint|typecheck|ci|e2e)\b[^.]*\b(green|passed|passes|passing)\b'
    r'|\b[0-9]+ (passed|failed|checks)\b'
    r'|[a-z]:\\|/c/users/|appdata|(^|\s)temp\\|/tmp/', re.I)

SKIP = ['vendor/*', '*/vendor/*', '*node_modules/*', 'dist/*', '*/dist/*', 'build/*', '*/build/*',
        'target/*', '*/target/*', '*.min.js']
SRC = ['*.py', '*.js', '*.mjs', '*.cjs', '*.ts', '*.tsx', '*.jsx', '*.rs', '*.java', '*.kt', '*.c', '*.cc',
       '*.cpp', '*.h', '*.hpp', '*.cs', '*.go', '*.lua', '*.ps1', '*.sh', '*.mcfunction']
TEST = ['test/*', '*/test/*', 'tests/*', '*/tests/*', 'spec/*', '*/spec/*', '*/__tests__/*', '*_test.*',
        '*.test.*', '*.spec.*', 'test_*.py', '*/test_*.py', '*Test.java', '*Test.kt', '*Test.cs',
        '*Tests.cs', '*.snap']


def matches(path, globs):
    return any(fnmatch.fnmatchcase(path, g) for g in globs)


def src(path):
    return not matches(path, SKIP) and matches(path, SRC)


def message(msg):
    msg = re.sub(r'(?ms)^# -* >8 -*$.*', '', msg)
    lines = [l for l in msg.splitlines() if not l.startswith('#')]
    filled = [l for l in lines if l.strip()]
    errors = []
    if ATTRIBUTION.search('\n'.join(lines)):
        errors.append('drop the AI attribution (co-author trailer / generated-with line)')
    report = next((l for l in filled[1:] if REPORT.search(l)), None)
    if report:
        errors.append(f'test results and local paths go in the PR, not the commit: {report.strip()}')
    subject = filled[0] if filled else ''
    if not re.match(r'Merge |Revert |fixup!|squash!|amend!', subject):
        if len(subject) > 72:
            errors.append(f'subject is {len(subject)} chars, keep it to 72')
        if len(filled) - 1 > 10:
            errors.append(f'body is {len(filled) - 1} lines; keep it to the why, or split the commit')
    return errors


def git(cwd, *args):
    return subprocess.run(['git', *args], cwd=cwd, check=True, capture_output=True,
                          encoding='utf-8', errors='replace').stdout


def lines(cwd, rev, path):
    r = subprocess.run(['git', 'show', f'{rev}:{path}'], cwd=cwd, capture_output=True)
    return r.stdout.count(b'\n') if r.returncode == 0 else 0


def check(revs, cwd='.'):
    errors, warnings = [], []
    log = git(cwd, 'log', '--no-merges', '--reverse', '--format=%H%x00%B%x00', revs).split('\0')
    for sha, msg in zip(log[0::2], log[1::2]):
        sha = sha.strip()
        name = f'{sha[:8]} {msg.strip().splitlines()[0] if msg.strip() else ""}'
        problems = message(msg)

        big = []
        for f in git(cwd, 'show', '--format=', '--name-only', '--diff-filter=AM', sha).splitlines():
            if not src(f):
                continue
            new = lines(cwd, sha, f)
            old = lines(cwd, f'{sha}^', f)
            if new > MAX >= old:
                big.append(f'{f}: {old} -> {new} lines')
        if big:
            problems.append(f'these went past {MAX} lines, split them instead: ' + ', '.join(big))
        if problems:
            errors.append(f'{name}\n' + '\n'.join(f'  {p}' for p in problems))

        edited = []
        for row in git(cwd, 'show', '--format=', '--numstat', '--diff-filter=MD', sha).splitlines():
            _, deleted, f = row.split('\t', 2)
            if matches(f, TEST) and deleted != '0':
                edited.append(f)
        code = [f for f in git(cwd, 'show', '--format=', '--name-only', '--diff-filter=AMD', sha).splitlines()
                if src(f) and not matches(f, TEST)]
        if edited and code:
            warnings.append(f'{name}\n  changes existing tests in the same commit as the code they check: '
                            + ', '.join(edited))
    return errors, warnings


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    errors, warnings = check(sys.argv[1])
    for w in warnings:
        print(f'warning: {w}')
    for e in errors:
        print(e)
    sys.exit(1 if errors else 0)
