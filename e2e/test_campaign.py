import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import campaign
import stress
import sys
import json
from types import SimpleNamespace
import parity


class CampaignTests(unittest.TestCase):
    def test_schedule_varies_sizes_zooms_and_seeds(self):
        tasks = [campaign.task(i) for i in range(150)]
        tours = {tuple(t['sizes']) for t in tasks if t['suite'] == 'tour'}
        seeds = [t['suite'] for t in tasks if t['suite'].startswith('stress-')]
        self.assertEqual(len(tours), 15)
        self.assertEqual(len(seeds), len(set(seeds)))
        self.assertTrue({'full', 'restart', 'bench'} <= {t['suite'] for t in tasks})
        self.assertEqual(tasks, [campaign.task(i) for i in range(150)])

    def test_discovery_gets_most_slots_and_regressions_are_periodic(self):
        tasks = [campaign.task(i)['suite'] for i in range(160)]
        self.assertGreaterEqual(sum(t.startswith('stress-') for t in tasks), 112)
        self.assertEqual(tasks.count('full'), 4)
        self.assertEqual(tasks.count('bench'), 4)

    def test_old_sessions_seed_the_campaign_visit_history(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(16)
            state['attempts'] = [{'result': {'runs': {'explore': {'rects': {'stress_visits': {'chat|click|gear': 3}}}}}},
                                 {'result': {'runs': {'explore': {'rects': {'stress_visits': {'chat|click|gear': 2, 'chat|right|gear': 1}}}}}}]
            campaign.init_corpus(root, state)
            corpus = json.loads((root / 'corpus.json').read_text())
            self.assertEqual(corpus['visits']['chat|click|gear'], 5)
            self.assertEqual(corpus['visits']['chat|right|gear'], 1)
            campaign.init_corpus(root, state)
            self.assertEqual(json.loads((root / 'corpus.json').read_text()), corpus)

    def test_recovered_cumulative_history_is_not_counted_twice(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(16)
            state['attempts'] = [{'result': {'runs': {'explore': {'rects': {'stress_visits': {'chat|click|gear': 3}}}}}},
                                 {'result': {'runs': {'explore': {'rects': {'stress_corpus_loaded': True, 'stress_visits': {'chat|click|gear': 8}}}}}}]
            campaign.init_corpus(root, state)
            self.assertEqual(json.loads((root / 'corpus.json').read_text())['visits']['chat|click|gear'], 8)

    def test_repeated_states_and_targets_do_not_inflate_discovery(self):
        with tempfile.TemporaryDirectory() as tmp:
            state = campaign.new_state(16)
            state['attempts'] = [dict(index=i, directory=f'run-{i}', status='completed', task={'suite': f'stress-{i + 1}'},
                                     result=dict(status='recorded', state_ids=['same-state'], targets=['control:button:Save'])) for i in range(2)]
            campaign.save(Path(tmp), state)
            self.assertEqual(state['attempts'][0]['new_states'], 1)
            self.assertEqual(state['attempts'][0]['new_targets'], 1)
            self.assertEqual(state['attempts'][1]['new_states'], 0)
            self.assertEqual(state['attempts'][1]['new_targets'], 0)

    def test_resuming_old_campaign_starts_discovery_with_a_new_seed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(.001)
            state['attempts'] = [dict(index=0, directory='old', status='completed', task={'suite': 'stress-2'}, result={'seed': 2})]
            with patch.object(campaign, 'perform', return_value=dict(name='stress-3', status='recorded')) as perform, patch.object(campaign.time, 'monotonic', side_effect=[0, 4]):
                campaign.execute(root, state)
            self.assertEqual(perform.call_args.args[1]['suite'], 'stress-3')

    def test_native_run_snapshots_its_input_history(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / 'corpus.json'
            corpus.write_text('{"version":1,"visits":{"x":2}}')
            def launch(cmd, **kwargs):
                snapshot = Path(kwargs['env']['RUSTCORD_EXPLORE_CORPUS'])
                self.assertNotEqual(snapshot, corpus)
                self.assertEqual(json.loads(snapshot.read_text())['visits']['x'], 2)
                corpus.write_text('{"version":1,"visits":{"x":99}}')
                (snapshot.parent / 'result.json').write_text('{"checks":{"completed":true}}')
                return SimpleNamespace(returncode=0)
            with patch.object(parity.subprocess, 'run', side_effect=launch), patch.object(parity, 'binaries', return_value={}):
                parity.native_suite('stress-1', root, [], corpus=corpus)
            self.assertEqual(json.loads((root / 'native/stress-1/corpus-input.json').read_text())['visits']['x'], 2)

    def test_campaign_preserves_failed_runs_and_continues(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(.001)
            with patch.object(campaign, 'perform', return_value=dict(name='stress-1', status='failed', checks={'alive': False})), patch.object(campaign.time, 'monotonic', side_effect=[0, 2, 2, 4]):
                campaign.execute(root, state)
            self.assertEqual(state['status'], 'budget_finished')
            self.assertEqual(state['attempts'][0]['result']['status'], 'failed')
            self.assertEqual(len(state['attempts']), 2)
            self.assertTrue((root / 'campaign.json').is_file())
            self.assertTrue((root / 'index.html').is_file())

    def test_stop_file_prevents_launching_another_test(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'stop').touch()
            state = campaign.new_state(16)
            with patch.object(campaign, 'perform') as perform:
                campaign.execute(root, state)
            perform.assert_not_called()
            self.assertEqual(state['status'], 'stopped')

    def test_resume_retries_interrupted_task_without_replacing_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(.001)
            state['attempts'] = [dict(index=0, directory='00000-old', status='running')]
            with patch.object(campaign, 'perform', return_value=dict(name='stress-1', status='recorded')), patch.object(campaign.time, 'monotonic', side_effect=[0, 4]):
                campaign.execute(root, state)
            self.assertEqual(state['attempts'][0]['status'], 'interrupted')
            self.assertEqual(state['attempts'][1]['index'], 0)
            self.assertNotEqual(state['attempts'][1]['directory'], '00000-old')

    def test_repeated_infrastructure_errors_stop_the_campaign(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(16)
            with patch.object(campaign, 'perform', side_effect=OSError('missing executable')):
                campaign.execute(root, state)
            self.assertEqual(state['status'], 'blocked')
            self.assertEqual(len(state['attempts']), 3)

    def test_report_exposes_observed_controls_without_pointer_actions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            state = campaign.new_state(16)
            state['attempts'] = [dict(index=0, directory='run', status='completed', task={'suite': 'stress-1'}, result={
                'status': 'recorded', 'observed_controls': ['control:button:Save', 'control:toggle:Mute'], 'targets': ['control:button:Save']})]
            campaign.save(root, state)
            page = (root / 'index.html').read_text(encoding='utf-8')
            self.assertIn('observed but not pointer-targeted: 1', page)
            self.assertIn('control:toggle:Mute', page)

    def test_exploration_can_rerun_the_campaign_seed_and_action_count(self):
        result = dict(ok=True, exit_code=0, marks_ms={'stress_completed': 1})
        with tempfile.TemporaryDirectory() as tmp, patch.object(sys, 'argv', ['stress.py', '--actions', '800', '17']), patch.object(stress.tempfile, 'mkdtemp', return_value=tmp), patch.object(stress.subprocess, 'Popen'), patch.object(stress.time, 'sleep'), patch.object(stress.run, 'run_client', return_value=result) as client:
            with self.assertRaises(SystemExit) as end:
                stress.main()
        self.assertEqual(end.exception.code, 0)
        extra = client.call_args.args[2]
        self.assertEqual(extra['RUSTCORD_STRESS_SEED'], '17')
        self.assertEqual(extra['RUSTCORD_STRESS_ACTIONS'], '800')

    def test_sleep_request_is_released_when_a_campaign_stops(self):
        with patch.object(campaign.ctypes.windll.kernel32, 'SetThreadExecutionState', return_value=1) as power:
            with self.assertRaises(RuntimeError):
                with campaign.keep_awake():
                    raise RuntimeError('interrupted')
        self.assertEqual([call.args[0] for call in power.call_args_list], [0x80000003, 0x80000000])


if __name__ == '__main__':
    unittest.main()
