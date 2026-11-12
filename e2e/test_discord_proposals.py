import unittest

import propose_discord_features


class ProposalTests(unittest.TestCase):
    def test_proposals_keep_reference_context_and_require_review(self):
        family = dict(id='message', view='discord:message.png', required_visible=['message-menu'])
        data = dict(reference_controls=[dict(id='node:1', reference_view='discord:message.png', role='menuitem', title='Copy Text')],
                    controls=[dict(id='control:menu_row:Copy Text')])
        rows = propose_discord_features.propose(data, [family])
        self.assertEqual(rows[0]['mapping_status'], 'candidate')
        self.assertEqual(rows[0]['reference']['view'], 'discord:message.png')
        self.assertEqual(rows[0]['rustcord']['required_visible'], ['message-menu'])

    def test_duplicate_reference_labels_do_not_generate_a_binding(self):
        family = dict(id='message', view='discord:message.png', required_visible=['message-menu'])
        node = dict(id='node:1', reference_view='discord:message.png', role='menuitem', title='Copy Text')
        data = dict(reference_controls=[node,dict(node,id='node:2')], controls=[dict(id='control:menu_row:Copy Text')])
        self.assertEqual(propose_discord_features.propose(data, [family]), [])

    def test_background_labels_and_disabled_menu_entries_are_not_proposed(self):
        family = dict(id='message', view='discord:message.png', required_visible=['message-menu'])
        data = dict(reference_controls=[dict(id='background',reference_view='discord:message.png',role='span',title='Add Friend'),
                                        dict(id='disabled',reference_view='discord:message.png',role='menuitem',title='Copy Text',disabled=True)],
                    controls=[dict(id='control:menu_row:Add Friend'),dict(id='control:menu_row:Copy Text')])
        self.assertEqual(propose_discord_features.propose(data,[family]), [])

    def test_submenus_propose_hover_instead_of_a_click(self):
        family = dict(id='message',view='discord:message.png',required_visible=['message-menu'])
        data = dict(reference_controls=[dict(id='apps',reference_view='discord:message.png',role='menuitem',title='Apps',has_submenu=True)],
                    controls=[dict(id='control:menu_row:Apps')])
        row = propose_discord_features.propose(data,[family])[0]
        self.assertEqual(row['rustcord']['method'], 'hover')


if __name__ == '__main__':
    unittest.main()
