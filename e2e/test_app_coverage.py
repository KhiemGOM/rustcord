import json
import tempfile
import unittest
import re
from pathlib import Path
from PIL import Image

import app_coverage


class AppCoverageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.source = self.root / 'src/ui'
        self.source.mkdir(parents=True)
        self.campaign = self.root / 'campaign'
        self.campaign.mkdir()
        self.ref = self.root / 'ref'
        self.ref.mkdir()
        (self.source / 'settings').mkdir()
        (self.source / 'settings/mod.rs').write_text('pub enum Page { Account, Profiles }\nItem::Page(Page::Account, "Account", Icon::Account, &["Password"]);\nmark(ui, "save", rect);\nmark(ui, "cancel", rect);\nmark(ui, &format!("toggle:{label}"), rect);\n')

    def log(self, events, tail=''):
        folder = self.campaign / 'task/native/stress-1/explore'
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / 'actions.jsonl'
        path.write_text(''.join(json.dumps(e) + '\n' for e in events) + tail)
        return path

    def build(self):
        return app_coverage.collect(self.root, self.campaign, self.ref)

    def test_controls_keep_unseen_and_seen_separate_from_attempted(self):
        self.log([dict(before='state1', visible=['control:save'], action={'key': 'Escape'})])
        data = self.build()
        rows = {r['id']: r for r in data['controls']}
        self.assertEqual(rows['control:save']['status'], 'observed')
        self.assertEqual(rows['control:save']['evidence'][0]['event'], 'observed')
        self.assertEqual(rows['control:cancel']['status'], 'untested')
        self.assertFalse(rows['control:save']['verified'])
        self.log([dict(before='state1', visible=['control:save'], action={'target': 'control:save', 'method': 'click'})])
        rows = {r['id']: r for r in self.build()['controls']}
        self.assertEqual(rows['control:save']['methods']['click'], 1)
        self.assertEqual(rows['control:save']['status'], 'interacted')
        self.assertFalse(rows['control:save']['verified'])

    def test_dynamic_controls_are_not_a_closed_denominator(self):
        self.log([dict(before='s', visible=['control:toggle:Mute'], action={'target': 'control:toggle:Mute', 'method': 'hover'})])
        data = self.build()
        rows = {r['id']: r for r in data['controls']}
        self.assertIn('control:toggle:Mute', rows)
        self.assertGreater(len(data['dynamic_sites']), 0)
        self.assertFalse(data['complete_app_denominator'])

    def test_navigating_to_a_page_does_not_prove_it_was_captured(self):
        self.log([dict(before='s', visible=['settings:Account'], action={'target': 'settings:Account', 'method': 'click'})])
        data = self.build()
        page = next(v for v in data['views'] if v['id'] == 'settings:Account')
        self.assertEqual(page['status'], 'untested')
        folder = self.campaign / 'task/native/tour/tour_1280x720_z100'
        folder.mkdir(parents=True)
        Image.new('RGB', (2, 2)).save(folder / 'settings_00_account_0.png')
        Image.new('RGB', (2, 2)).save(folder / 'settings_00_account_1.png')
        page = next(v for v in self.build()['views'] if v['id'] == 'settings:Account')
        self.assertEqual(page['status'], 'captured')
        self.assertEqual(page['captured_variants'], ['1280x720@100'])
        self.assertFalse(page['verified'])

    def test_discord_reference_capture_does_not_verify_rustcord(self):
        (self.ref / 'INDEX.md').write_text('### [chat](chat.png)\n')
        (self.ref / 'chat.json').write_text(json.dumps(dict(vw=100, vh=100, els=[dict(t='button', role='button', l='save', x=1, y=1, w=10, h=10)])))
        self.log([dict(before='s', visible=['control:save'], action={'target': 'control:save', 'method': 'click'})])
        data = self.build()
        self.assertEqual(len(data['reference_views']), 1)
        self.assertEqual(len(data['reference_controls']), 1)
        self.assertEqual(data['reference_controls'][0]['status'], 'unmapped')

    def test_partial_log_tail_is_not_reported_as_a_completed_action(self):
        path = self.log([dict(before='s', visible=['control:save'], action={'key': 'Tab'})], '{"before":')
        data = self.build()
        self.assertEqual(data['actions'], 1)
        self.assertEqual(data['pending_logs'], [str(path)])

    def test_observed_transition_is_not_a_verified_outcome(self):
        self.log([dict(before='a', visible=['control:save'], action={'target': 'control:save', 'method': 'click'}),
                  dict(before='b', visible=[], terminal=True)])
        data = self.build()
        row = next(r for r in data['controls'] if r['id'] == 'control:save')
        self.assertEqual(row['outcomes']['marker_state_changed'], 1)
        self.assertFalse(row['verified'])

    def test_each_pointer_method_keeps_its_own_evidence(self):
        methods = ['click', 'right', 'hover', 'middle']
        self.log([dict(before='s', visible=['control:save'], action={'target': 'control:save', 'method': method}) for method in methods])
        row = next(r for r in self.build()['controls'] if r['id'] == 'control:save')
        self.assertTrue(set(methods) <= {item['event'] for item in row['evidence']})

    def test_capture_checkpoints_include_non_settings_screens(self):
        (self.source / 'e2e.rs').write_text('self.shot(ctx, "message_edit.png");')
        data = self.build()
        row = next(v for v in data['views'] if v['id'] == 'capture:message_edit.png')
        self.assertEqual(row['status'], 'untested')

    def test_embedded_labels_do_not_replace_template_markers(self):
        self.log([dict(before='s', visible=['control:</script> META PAYLOAD'], action={'key': 'Tab'})])
        out = self.root / 'out'
        out.mkdir()
        app_coverage.render(out, self.build())
        page = (out / 'index.html').read_text(encoding='utf-8')
        payload = re.search(r'const rows=(.*?);let page=0', page, re.S)[1]
        rows = json.loads(payload)
        self.assertIn('control:</script> META PAYLOAD', [r['id'] for r in rows])


if __name__ == '__main__':
    unittest.main()
