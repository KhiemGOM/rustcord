import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import gates


class Message(unittest.TestCase):
    def bad(self, *msg):
        self.assertTrue(gates.message('\n\n'.join(msg)), msg)

    def good(self, *msg):
        self.assertEqual(gates.message('\n\n'.join(msg)), [], msg)

    def test_attribution(self):
        self.bad('add a', 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>')
        self.bad('add a', '🤖 Generated with [Claude Code](https://claude.com/claude-code)')
        self.good('add a', 'Co-Authored-By: Dustiyn <d@example.com>')

    def test_length(self):
        self.bad('add a file that has a really quite long subject line which goes on and on')
        self.bad('add a', '\n'.join(f'line {i}' for i in range(11)))
        self.good('a' * 72)
        self.good('Merge pull request #51 from someone/a-branch-whose-name-is-long-enough-to-pass-72')

    def test_reports(self):
        self.bad('add guides', 'Checks: focused browser tests, lint, live screenshots.')
        self.bad('keep logs off stdout', 'CoreServicesContractFixture is green; build + fixturesPortable green.')
        self.bad('fix speaking marks', 'Red on the parent: 28 checks fail.')
        self.bad('fix vanilla toggle', r'E2E on 1.16.1: Temp\zr-20261026\e2e_fight.py --negative')
        self.bad('fix toggle', r'repro in C:\Users\User\AppData\Local\Temp\x')
        self.bad('fix layout', '180 passed, 1 failed (10.5m)')

    def test_prose(self):
        self.good('verify Minecraft accounts against the tier bot\'s pending codes')
        self.good('pass crystal breaks to the dragon',
                  'the fight only passes crystal breaks to that UUID, so the summoned dragon never heard them')
        self.good('add MCSR split tests', 'tests/: characterization test for MCSR splits')
        self.good('speed up findNearest', 'dropped the sourceBlocks == 12 check')
        self.good('release bundle: verifyReleaseZip tests first (red: no releaseZip task)')


class Repo(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(tmp.name)
        self.env = dict(os.environ, GIT_AUTHOR_NAME='t', GIT_AUTHOR_EMAIL='t@t',
                        GIT_COMMITTER_NAME='t', GIT_COMMITTER_EMAIL='t@t')
        self.git('init', '-q', '--template=')
        self.commit('init', {'a.txt': 'a\n'})
        self.base = self.git('rev-parse', 'HEAD').strip()

    def git(self, *args):
        return subprocess.run(['git', '-c', 'commit.gpgsign=false', *args], cwd=self.dir, env=self.env,
                              check=True, capture_output=True, text=True).stdout

    def commit(self, msg, files):
        for name, text in files.items():
            (self.dir / name).parent.mkdir(parents=True, exist_ok=True)
            (self.dir / name).write_text(text)
        self.git('add', '-A')
        self.git('commit', '-q', '-m', msg)

    def check(self):
        return gates.check(f'{self.base}..HEAD', self.dir)

    def test_file_crossing_the_limit(self):
        self.commit('add grow', {'grow.py': 'x\n' * 900})
        self.commit('grow', {'grow.py': 'x\n' * 1001})
        errors, _ = self.check()
        self.assertEqual(len(errors), 1)
        self.assertIn('grow.py: 900 -> 1001', errors[0])

    def test_exempt_files(self):
        self.commit('add data', {'data.json': 'x\n' * 1001, 'vendor/lib.py': 'x\n' * 1001})
        self.commit('add huge', {'huge.py': 'x\n' * 1200})
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.commit('touch huge', {'huge.py': 'x\n' * 1210})
        self.assertEqual(self.check(), ([], []))

    def test_test_edited_with_its_code_warns(self):
        self.commit('add f', {'f.py': 'def f(): return 1\n', 'tests/test_f.py': 'assert f() == 1\n'})
        self.commit('change f', {'f.py': 'def f(): return 2\n', 'tests/test_f.py': 'assert f() == 2\n'})
        errors, warnings = self.check()
        self.assertEqual(errors, [])
        self.assertEqual(len(warnings), 1)
        self.assertIn('tests/test_f.py', warnings[0])

    def test_new_tests_with_new_code(self):
        self.commit('add f', {'f.py': 'def f(): return 1\n', 'tests/test_f.py': 'assert f() == 1\n'})
        self.commit('add g', {'f.py': 'def f(): return 1\ndef g(): return 0\n',
                              'tests/test_f.py': 'assert f() == 1\nassert g() == 0\n'})
        self.commit('loosen g test', {'tests/test_f.py': 'assert f() == 1\nassert g() <= 0\n'})
        self.assertEqual(self.check(), ([], []))

    def test_merges_are_skipped(self):
        self.git('checkout', '-q', '-b', 'side')
        self.commit('add b', {'b.txt': 'b\n'})
        self.git('checkout', '-q', '-')
        self.commit('add c', {'c.txt': 'c\n'})
        self.git('merge', '-q', '--no-ff', 'side', '-m', 'm' * 100)
        self.assertEqual(self.check(), ([], []))

    def test_each_commit_is_named(self):
        self.commit('add b\n\nCo-Authored-By: Claude <noreply@anthropic.com>', {'b.txt': 'b\n'})
        errors, _ = self.check()
        self.assertEqual(len(errors), 1)
        self.assertIn(self.git('rev-parse', '--short', 'HEAD').strip(), errors[0])
        self.assertIn('add b', errors[0])


if __name__ == '__main__':
    unittest.main()
