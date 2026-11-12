import json
import tempfile
import unittest
from pathlib import Path

import discord_journeys


class JourneyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding='utf-8')
        return path

    def capture(self, name, text):
        path = self.root / name
        path.with_suffix('.png').write_bytes(b'capture')
        Path(str(path) + '.aria.yml').write_text(text, encoding='utf-8')

    def test_before_after_is_reference_evidence_not_parity_or_a_replay_recipe(self):
        self.write('q1/_index.json', [dict(name='q101_test_0before', note='Escape cancels reply - before'), dict(name='q101_test_1after', note='after')])
        self.capture('q1/q101_test_0before', '- button "Cancel Reply"\n- textbox "Message"')
        self.capture('q1/q101_test_1after', '- textbox "Message"')
        row = discord_journeys.archive(self.root)[0]
        self.assertEqual(row['status'], 'reference_transition_captured')
        self.assertEqual(row['delta']['removed'], ['- button "Cancel Reply"'])
        self.assertFalse(row['parity_verified'])
        self.assertEqual(row['recipe_status'], 'missing')

    def test_missing_or_unchanged_evidence_cannot_be_a_transition(self):
        self.write('q1/_index.json', [dict(name='q101_test_0before', note='test')])
        self.capture('q1/q101_test_0before', '- textbox "Message"')
        self.assertEqual(discord_journeys.archive(self.root)[0]['status'], 'reference_incomplete')
        self.write('q1/_index.json', [dict(name='q101_test_0before', note='test'), dict(name='q101_test_1after', note='after')])
        self.capture('q1/q101_test_1after', '- textbox "Message"')
        self.assertEqual(discord_journeys.archive(self.root)[0]['status'], 'reference_no_observable_change')

    def test_nested_menu_states_and_failed_hover_remain_separate_observations(self):
        self.write('s/m.menu.json', dict(items=[dict(id='apps', role='menuitem', text='Apps', sub=True, children=dict(items=[dict(id='use', role='menuitem', text='Use App', disabled=True)]))]))
        self.write('s/h.sweep.json', dict(items=[dict(i=0, label='Unknown', err='timeout')]))
        rows = discord_journeys.archive(self.root)
        self.assertEqual(len(rows), 3)
        child = next(row for row in rows if row.get('menu_item_id') == 'use')
        self.assertEqual(child['menu_path'], ['Apps', 'Use App'])
        self.assertEqual(child['status'], 'reference_disabled_control')
        self.assertEqual(next(row for row in rows if row['method'] == 'hover')['status'], 'reference_input_failed')

    def test_live_context_mismatch_and_missing_capture_cannot_pass(self):
        self.write('run.json', dict(app='discord-web'))
        run = self.root
        (run / 'actions.jsonl').write_text(json.dumps(dict(index=1, before='a', after='b', status='observed_transition', replay_context_matched=False, input=dict(method='click'), family='home')) + '\n')
        self.assertEqual(discord_journeys.live(run)[0]['status'], 'reference_context_mismatch')
        (run / 'actions.jsonl').write_text(json.dumps(dict(index=1, before='a', after='b', status='observed_transition', replay_context_matched=True, input=dict(method='click'), family='home')) + '\n')
        self.assertEqual(discord_journeys.live(run)[0]['status'], 'reference_incomplete')

    def test_unexecuted_action_cannot_keep_a_passing_outcome_and_partial_tail_is_pending(self):
        self.write('run.json', dict(app='discord-web'))
        action = dict(index=1, before='a', after='b', status='observed_transition', replay_context_matched=True,
                      input=dict(method='click'), family='home', check=dict(status='passed'))
        log = self.root / 'actions.jsonl'
        log.write_text(json.dumps(action) + '\n' + json.dumps(dict(action, index=2)), encoding='utf-8')
        rows = discord_journeys.live(self.root)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['check']['status'], 'not_run')

    def test_non_discord_run_is_rejected(self):
        self.write('run.json', dict(app='rustcord-mock'))
        with self.assertRaisesRegex(ValueError, 'Discord'):
            discord_journeys.live(self.root)

    def test_archived_recipes_require_exact_capture_names_and_do_not_claim_execution(self):
        folder = self.root / 'plans'
        self.write('plans/q.json', [['goto', '/channels/@me'], ['cap', 'q101_0before'], ['key', 'Escape'], ['wait', 500], ['cap', 'q101_1after']])
        result = discord_journeys.recipes([folder])
        self.assertEqual(result['q101_0before'][0]['inputs'], [['key', 'Escape']])
        self.assertEqual(result['q101_0before'][0]['setup'], [['goto', '/channels/@me']])
        self.assertNotIn('q102_0before', result)

    def test_source_only_behaviors_remain_reference_gaps(self):
        (self.root / 'QOL.md').write_text('## Escape\n- Incoming call declines. src\n- Composer cancels. src + q101\n', encoding='utf-8')
        rows = discord_journeys.archive(self.root)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['status'], 'source_only_unexercised')


if __name__ == '__main__':
    unittest.main()
