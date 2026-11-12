import json
import tempfile
import unittest
from pathlib import Path

import discord_pair


def theirs(t, box, **extra):
    return {'t': t, 'b': box, 'size': 16, 'weight': '500', 'col': '#dcdcdfff', 'font': 'gg sans', 'covered': False, 'block': 0, **extra}


def ours(t, box, **extra):
    return {'t': t, 'b': box, 'clip': [0, 0, 1707, 913], 'z': 0, 'rows': 1, 'mixed': False, 'size': 16.0, 'font': 'medium', 'col': '#dcdcdfff', **extra}


def scene(texts, boxes=()):
    return {'texts': texts, 'boxes': list(boxes), 'marks': {}, 'layered': False, 'modal': False}


def page(texts):
    left, top = min(text['b'][0] for text in texts), min(text['b'][1] for text in texts)
    box = [left, top, max(text['b'][0] + text['b'][2] for text in texts) - left, max(text['b'][1] + text['b'][3] for text in texts) - top]
    return {'viewport': [1707, 913], 'layers': [], 'texts': texts, 'blocks': [{'t': ' '.join(text['t'] for text in texts), 'b': box}]}


class Pairing(unittest.TestCase):
    def kinds(self, discord, rustcord):
        return [(row['kind'], row['text'], row.get('by')) for row in discord_pair.compare(discord, rustcord)[0]]

    def test_identical_screens_have_no_differences(self):
        texts = [('Friends', [121, 99, 51, 21]), ('Shop', [121, 179, 35, 21])]
        self.assertEqual(self.kinds(page([theirs(*t) for t in texts]), scene([ours(*t) for t in texts])), [])

    def test_one_missing_divider_is_one_spacing_finding_not_one_per_row_below(self):
        discord = page([theirs('Quests', [121, 219, 48, 21]), theirs('Direct Messages', [89, 276, 99, 18], size=14), theirs('someone', [133, 310, 60, 21]), theirs('another', [133, 354, 60, 21])])
        rustcord = scene([ours('Quests', [121, 219, 48, 21]), ours('Direct Messages', [89, 266, 99, 18], size=14.0), ours('someone', [133, 300, 60, 21]), ours('another', [133, 344, 60, 21])])
        found = self.kinds(discord, rustcord)
        self.assertEqual(found, [('spacing', 'Direct Messages', [0.0, -10.0])])

    def test_size_weight_colour_and_styled_names_are_told_apart(self):
        discord = page([theirs('10', [100, 400, 16, 21]), theirs('general', [116, 500, 60, 21]), theirs('Cyril', [133, 600, 37, 21], font='8Bit'), theirs('muted', [116, 700, 40, 21], col='#81828aff')])
        rustcord = scene([ours('10', [100, 401, 14, 19], size=14.0), ours('general', [116, 500, 60, 21], font='semi'), ours('Cyril', [133, 600, 32, 21]), ours('muted', [116, 700, 40, 21])])
        found = self.kinds(discord, rustcord)
        self.assertIn(('font size', '10', [16, 14.0]), found)
        self.assertIn(('font', 'general', ['gg sans 500', 'semi']), found)
        self.assertIn(('font', 'Cyril', ['8Bit 500', 'medium']), found)
        self.assertIn(('colour', 'muted', ['#81828aff', '#dcdcdfff']), found)
        self.assertNotIn('text width', [kind for kind, text, _ in found if text == 'Cyril'])

    def test_emoji_drawn_as_pictures_still_match_and_real_absences_are_listed(self):
        discord = page([theirs('\U0001f4cb┃info', [116, 186, 64, 21]), theirs('3.5-year npc', [151, 366, 65, 16], size=12)])
        rustcord = scene([ours(' ┃info', [116, 186, 64, 21]), ours('lyn', [132, 831, 20, 21])])
        self.assertEqual(self.kinds(discord, rustcord), [('missing in Rustcord', '3.5-year npc', None), ('only in Rustcord', 'lyn', None)])

    def test_hidden_text_on_either_side_is_not_a_difference(self):
        discord = page([theirs('Friends', [121, 99, 51, 21]), theirs('Unread messages, a server', [35, 105, 512, 21], covered=True)])
        under = ours('behind the dialog', [400, 300, 100, 21])
        clipped = ours('scrolled out', [116, 20, 80, 21], clip=[72, 81, 302, 780])
        rustcord = scene([ours('Friends', [121, 99, 51, 21]), under, clipped], [{'b': [380, 280, 300, 80], 'clip': [0, 0, 1707, 913], 'z': 1, 'fill': '#000000b3'}])
        self.assertEqual(self.kinds(discord, rustcord), [])

    def test_words_of_a_paragraph_drawn_whole_are_not_missing(self):
        discord = page([theirs('hello', [450, 300, 36, 21]), theirs('bold', [490, 300, 30, 21], weight='700'), theirs('world', [524, 300, 40, 21])])
        rustcord = scene([ours('hello bold world', [450, 300, 114, 21], mixed=True)])
        self.assertEqual(self.kinds(discord, rustcord), [])

    def test_the_same_word_far_away_is_not_the_same_text(self):
        discord = page([theirs('Hoplite', [813, 147, 64, 26], size=20)])
        rustcord = scene([ours('Hoplite', [843, 700, 45, 19], size=14.0)])
        self.assertEqual([kind for kind, _, _ in self.kinds(discord, rustcord)], ['missing in Rustcord', 'only in Rustcord'])


class Journeys(unittest.TestCase):
    def test_inputs_keep_discord_ids_and_the_route_discord_settled_on(self):
        channel = {'role': 'channel', 'name': 'general (text channel), unread', 'text': 'general', 'item': 'channels___1339586800519221299', 'id': '', 'box': [80, 200, 286, 32]}
        message = {'role': 'message', 'name': 'someone hello', 'text': 'someone', 'item': '', 'id': 'chat-messages-1339586800519221302-1487651059143807056', 'box': [376, 120, 1331, 48]}
        gear = {'role': 'button', 'name': 'User Settings', 'text': '', 'item': '', 'id': '', 'box': [320, 868, 32, 32]}
        with tempfile.TemporaryDirectory() as folder:
            live = Path(folder)
            rows = [
                {'index': 1, 'route': '/channels/1/2', 'setup': [], 'input': {'method': 'right-click', 'target': channel}, 'input_executed': True, 'before_capture': 'action-000001-before', 'after_capture': 'a'},
                {'index': 2, 'route': '/channels/1/2', 'setup': [{'method': 'click', 'target': gear}], 'input': {'method': 'key', 'key': 'Escape'}, 'input_executed': True, 'before_capture': 'x', 'after_capture': 'a'},
                {'index': 3, 'route': '/channels/@me', 'setup': [], 'input': {'method': 'right-click', 'target': message}, 'input_executed': True, 'before_capture': 'action-000003-before', 'after_capture': 'a'},
                {'index': 4, 'route': '/channels/@me', 'setup': [], 'input': {'method': 'hover', 'target': gear}, 'status': 'reference_context_mismatch'},
            ]
            (live / 'actions.jsonl').write_text('\n'.join(json.dumps(row) for row in rows), encoding='utf-8')
            (live / 'action-000001-before.json').write_text(json.dumps({'route': '/channels/1/1339586800519221302'}), encoding='utf-8')
            (live / 'action-000003-before.json').write_text(json.dumps({'route': '/channels/@me'}), encoding='utf-8')
            made = discord_pair.journeys(live)
        self.assertEqual([journey['id'] for journey in made], ['action-000001', 'action-000002', 'action-000003'])
        self.assertEqual((made[0]['guild'], made[0]['channel']), ('1', '1339586800519221302'))
        self.assertEqual(made[0]['steps'], [{'do': 'rclick', 'at': [223.0, 216.0], 'rect': 'chan:1339586800519221299', 'text': 'general'}])
        self.assertEqual(made[1]['steps'], [{'do': 'click', 'at': [336.0, 884.0], 'rect': 'gear'}, {'do': 'key', 'key': 'Escape'}])
        self.assertEqual((made[2]['guild'], made[2]['channel'], made[2]['steps'][0]['rect']), (None, None, 'msg:1487651059143807056'))


if __name__ == '__main__':
    unittest.main()
