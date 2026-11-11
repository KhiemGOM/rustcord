import json
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path

from PIL import Image

import parity


class ParityTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.out = self.root / 'out'
        self.out.mkdir()
        self.env = dict(os='Windows', dpi=144, zoom=100, theme='Onyx', viewport=[20, 20], scenario='same data', input='wheel')

    def case(self, kind='image'):
        return dict(id='chat', kind=kind, reference='discord.png', candidate='rustcord.png',
                    reference_env=self.env, candidate_env=dict(self.env))

    def image(self, name, color='black', size=(20, 20)):
        Image.new('RGB', size, color).save(self.root / name)

    def score(self, case):
        return parity.compare(case, self.root, self.root, self.out, 0)

    def test_missing_candidate_is_untested(self):
        self.image('discord.png')
        self.assertEqual(self.score(self.case())['status'], 'untested')

    def test_missing_environment_is_untested(self):
        self.image('discord.png')
        self.image('rustcord.png')
        case = self.case()
        del case['candidate_env']
        self.assertEqual(self.score(case)['status'], 'untested')

    def test_environment_mismatch_cannot_pass(self):
        self.image('discord.png')
        self.image('rustcord.png')
        case = self.case()
        case['candidate_env']['dpi'] = 96
        self.assertEqual(self.score(case)['status'], 'untested')

    def test_image_failure_records_diff_and_preserves_inputs(self):
        self.image('discord.png')
        self.image('rustcord.png', 'white')
        result = self.score(self.case())
        self.assertEqual(result['status'], 'different')
        self.assertEqual(result['changed_fraction'], 1)
        for name in ('reference', 'candidate', 'diff'):
            self.assertTrue((self.out / result[name]).is_file())

    def test_size_mismatch_fails_without_resizing(self):
        self.image('discord.png')
        self.image('rustcord.png', size=(10, 10))
        self.assertEqual(self.score(self.case())['status'], 'different')

    def test_mask_and_pixel_budget_are_explicit(self):
        self.image('discord.png')
        self.image('rustcord.png')
        with Image.open(self.root / 'rustcord.png') as image:
            image.putpixel((0, 0), (255, 255, 255))
            image.save(self.root / 'rustcord.png')
        case = self.case()
        self.assertEqual(self.score(case)['status'], 'different')
        case['masks'] = [[0, 0, 1, 1]]
        self.assertEqual(self.score(case)['status'], 'verified')
        case['masks'] = [[0, 0, 20, 20]]
        self.assertEqual(self.score(case)['status'], 'error')

    def test_metric_failure_and_nonfinite_values(self):
        (self.root / 'a.json').write_text('{"latency": 12}')
        (self.root / 'b.json').write_text('{"latency": 20}')
        case = dict(self.case('metric'), reference='a.json', candidate='b.json', key='latency', tolerance=2)
        self.assertEqual(self.score(case)['status'], 'different')
        (self.root / 'b.json').write_text('{"latency": NaN}')
        self.assertEqual(self.score(case)['status'], 'error')

    def test_trace_compares_motion_between_endpoints(self):
        (self.root / 'a.json').write_text('[[0,0],[50,50],[100,100]]')
        (self.root / 'b.json').write_text('[[0,0],[50,90],[100,100]]')
        case = dict(self.case('trace'), reference='a.json', candidate='b.json', tolerance=5)
        self.assertEqual(self.score(case)['status'], 'different')
        (self.root / 'b.json').write_text('[[0,0],[25,25],[100,100]]')
        self.assertEqual(self.score(case)['status'], 'verified')

    def test_inventory_does_not_invent_pairings(self):
        (self.root / 'INDEX.md').write_text('### [chat](chat.png) — channel\n### [chat](chat.png) — duplicate\n', encoding='utf-8')
        plan = parity.inventory(self.root)
        self.assertEqual(len([c for c in plan['cases'] if c['id'] == 'atlas:chat.png']), 1)
        self.assertTrue(all(not c.get('candidate') for c in plan['cases']))

    def test_cannot_compare_a_file_to_itself(self):
        self.image('discord.png')
        case = dict(self.case(), candidate='discord.png')
        self.assertEqual(self.score(case)['status'], 'error')

    def test_full_worker_keeps_its_log_outside_the_capture_folder(self):
        folder = self.root / 'full'
        folder.mkdir()
        (folder / 'log.txt').write_text('keep')
        module = SimpleNamespace(ART=None)

        def main():
            self.assertNotEqual(module.ART, folder)
            module.ART.mkdir()
            (module.ART / 'report.json').write_text('{"checks":{"rendered":true}}')
            raise SystemExit(0)

        module.main = main
        with patch.dict(sys.modules, {'run': module}), patch.object(sys, 'argv', []):
            result = parity.worker('full', folder, [])
        self.assertEqual(result['checks'], {'rendered': True})
        self.assertEqual((folder / 'log.txt').read_text(), 'keep')

    def test_bench_worker_records_measurements_without_passing_a_budget(self):
        def main():
            folder = Path(sys.argv[sys.argv.index('--artifacts') + 1])
            folder.mkdir()
            (folder / 'bench.json').write_text('{"idle":{"cpu_pct_of_one_core":2}}')

        with patch.dict(sys.modules, {'run': SimpleNamespace(), 'bench': SimpleNamespace(main=main)}), patch.object(sys, 'argv', []):
            result = parity.worker('bench', self.root, [])
        self.assertEqual(result['measurements']['idle']['cpu_pct_of_one_core'], 2)
        self.assertNotIn('checks', result)

    def test_recorded_benchmark_does_not_hide_untested_coverage(self):
        folder = self.root / 'benchmark'
        argv = ['parity.py', 'run', '--suite', 'bench', '--ref', str(self.root), '--out', str(folder)]
        with patch.object(sys, 'argv', argv), patch.object(parity, 'native_suite', return_value=dict(name='bench', status='recorded', measurements={'cpu': 2})):
            self.assertEqual(parity.main(), 2)
        result = json.loads((folder / 'report.json').read_text())
        self.assertGreater(result['coverage']['untested'], 0)

    def test_report_lists_each_native_assertion_without_claiming_parity(self):
        data = dict(scope='fixture', cases=[dict(id='atlas:chat', kind='image', status='untested')], native=[
            dict(name='full', status='failed', checks={'settings: save': True, 'settings: reset': False, 'composer: paste': True},
                 runs={'cold': {'error': 'stopped before reset', 'marks_ms': {'opened': 1}}})])
        parity.report(self.out, data)
        saved = json.loads((self.out / 'report.json').read_text())
        self.assertEqual(saved['native_coverage']['assertions'], 3)
        self.assertEqual(saved['native_coverage']['passed'], 2)
        self.assertEqual(saved['native_coverage']['failed_or_unreached'], 1)
        self.assertEqual(saved['native_coverage']['areas']['settings']['failed_or_unreached'], 1)
        self.assertEqual(saved['coverage']['verified'], 0)
        page = (self.out / 'index.html').read_text(encoding='utf-8')
        self.assertIn('settings: reset', page)
        self.assertIn('stopped before reset', page)
        self.assertTrue((self.out / 'assertions.json').is_file())

    def test_full_worker_keeps_native_errors_and_measurements(self):
        folder = self.root / 'full-results'
        folder.mkdir()
        module = SimpleNamespace(ART=None)

        def main():
            module.ART.mkdir()
            (module.ART / 'report.json').write_text(json.dumps({'checks': {'x': False}, 'cold': {'error': 'lost focus', 'frame_cpu_ms': {'p95': 4}}, 'warm': {}, 'login': {}}))
            raise SystemExit(1)

        module.main = main
        with patch.dict(sys.modules, {'run': module}), patch.object(sys, 'argv', []):
            result = parity.worker('full', folder, [])
        self.assertEqual(result['runs']['cold']['error'], 'lost focus')
        self.assertEqual(result['runs']['cold']['frame_cpu_ms']['p95'], 4)

    def test_cli_records_both_matching_and_different_evidence(self):
        self.image('discord.png')
        self.image('same.png')
        self.image('different.png', 'white')
        cases = [dict(self.case(), id='same', candidate='same.png'), dict(self.case(), id='different', candidate='different.png')]
        plan = self.root / 'pairs.json'
        plan.write_text(json.dumps({'version': 1, 'cases': cases}))
        folder = self.root / 'pairs'
        cmd = [sys.executable, str(Path(parity.__file__)), 'compare', '--plan', str(plan), '--candidate', str(self.root), '--out', str(folder)]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        self.assertEqual(proc.returncode, 1, proc.stderr)
        result = json.loads((folder / 'report.json').read_text())
        self.assertEqual(result['coverage'], dict(verified=1, different=1, untested=0, error=0))

    def test_cli_reports_missing_evidence_and_does_not_overwrite(self):
        plan = self.root / 'plan.json'
        plan.write_text(json.dumps({'version': 1, 'cases': [self.case()]}))
        folder = self.root / 'report'
        cmd = [sys.executable, str(Path(parity.__file__)), 'compare', '--plan', str(plan), '--candidate', str(self.root), '--out', str(folder)]
        first = subprocess.run(cmd, capture_output=True, text=True)
        self.assertEqual(first.returncode, 2, first.stderr)
        report = json.loads((folder / 'report.json').read_text())
        self.assertEqual(report['coverage']['untested'], 1)
        self.assertTrue((folder / 'index.html').exists())
        original = (folder / 'report.json').read_bytes()
        second = subprocess.run(cmd, capture_output=True, text=True)
        self.assertNotEqual(second.returncode, 0)
        self.assertEqual(original, (folder / 'report.json').read_bytes())


if __name__ == '__main__':
    unittest.main()
