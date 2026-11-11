import unittest

import discord_coverage


class DiscordCoverageTests(unittest.TestCase):
    def data(self):
        return dict(reference_views=[dict(id='discord:settings/notifications.png', title='Notifications')],
                    reference_controls=[dict(id='node:1', reference_view='discord:settings/notifications.png', role='label', title='Enable Desktop Notifications')],
                    controls=[dict(id='control:toggle:Enable Desktop Notifications', observations=1, methods={'hover': 1}, evidence=[])], views=[])

    def binding(self):
        return dict(id='notifications.desktop', title='Toggle desktop notifications',
                    reference=dict(view='discord:settings/notifications.png', role='label', label='Enable Desktop Notifications'),
                    rustcord=dict(control='control:toggle:Enable Desktop Notifications', method='click'))

    def test_similar_labels_do_not_create_a_binding(self):
        result = discord_coverage.matrix(self.data(), [])
        self.assertEqual(result['features'], [])
        self.assertEqual(result['reference_views'][0]['bindings'], [])

    def test_hover_does_not_cover_a_click_interaction(self):
        row = discord_coverage.matrix(self.data(), [self.binding()])['features'][0]
        self.assertEqual(row['status'], 'seen_not_exercised')
        self.assertFalse(row['parity_verified'])

    def test_correct_method_records_an_attempt_without_proving_parity(self):
        data = self.data()
        data['controls'][0]['methods']['click'] = 1
        row = discord_coverage.matrix(data, [self.binding()])['features'][0]
        self.assertEqual(row['status'], 'input_attempted_in_mock')
        self.assertFalse(row['parity_verified'])

    def test_duplicate_controls_in_one_reference_state_need_disambiguation(self):
        data = self.data()
        data['reference_controls'].append(dict(data['reference_controls'][0], id='node:2'))
        data['controls'][0]['methods']['click'] = 1
        row = discord_coverage.matrix(data, [self.binding()])['features'][0]
        self.assertEqual(row['status'], 'ambiguous_reference')

    def test_missing_reference_cannot_be_covered_by_a_mock_click(self):
        data = self.data()
        data['reference_controls'] = []
        data['controls'][0]['methods']['click'] = 1
        row = discord_coverage.matrix(data, [self.binding()])['features'][0]
        self.assertEqual(row['status'], 'reference_missing')

    def test_view_capture_does_not_cover_every_control_on_the_page(self):
        data = self.data()
        data['views'] = [dict(id='settings:Notifications', status='captured', evidence=['shot.png'])]
        bindings = [dict(id='notifications.view', title='Notifications screen', reference=dict(view='discord:settings/notifications.png'), rustcord=dict(view='settings:Notifications'))]
        result = discord_coverage.matrix(data, bindings)
        self.assertEqual(result['features'][0]['status'], 'rustcord_view_captured')
        self.assertEqual(result['reference_views'][0]['control_bindings'], 0)


if __name__ == '__main__':
    unittest.main()
