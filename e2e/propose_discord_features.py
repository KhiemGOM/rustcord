"""Propose context-scoped menu bindings for review, without crediting coverage.

    python e2e/propose_discord_features.py --coverage path/to/coverage.json --out proposals.json
"""

import argparse
import json
import re
from collections import Counter, defaultdict
from pathlib import Path

FAMILIES = [
    dict(id='message', view='discord:c2_chat/c63_message_ctx_other.png', required_visible=['control:menu_row:Copy Message Link', 'control:menu_row:Mark Unread']),
    dict(id='friend', view='discord:a_shell/a13_friend_ctx.png', required_visible=['control:menu_row:Remove Friend', 'control:menu_row:Add Friend Nickname']),
    dict(id='dm', view='discord:a_shell/a14_dm_ctx.png', required_visible=['control:menu_row:Close DM', 'control:menu_row:Pin']),
    dict(id='group-dm', view='discord:a_shell/a15_groupdm_ctx.png', required_visible=['control:menu_row:Leave Group', 'control:menu_row:Edit Group']),
    dict(id='channel', view='discord:b_guild2/b11_ctx_text_channel.png', required_visible=['control:menu_row:Edit Channel', 'control:menu_row:Duplicate Channel']),
    dict(id='server', view='discord:b_guild2/b37_guild_menu_owner.png', required_visible=['control:menu_row:Hide Muted Channels', 'control:menu_row:Notification Settings']),
    dict(id='member', view='discord:b_guild2/b22_ctx_member.png', required_visible=['control:menu_row:Mention', 'control:menu_row:Message']),
]


def propose(data, families=FAMILIES):
    normalize = lambda text: ' '.join(text.casefold().split())
    native = defaultdict(list)
    for row in data['controls']:
        if row['id'].startswith('control:menu_row:'):
            native[normalize(row['id'].removeprefix('control:menu_row:'))].append(row['id'])
    result = []
    for family in families:
        nodes = [row for row in data['reference_controls'] if row['reference_view'] == family['view'] and row['role'] in ('menuitem', 'menuitemcheckbox', 'menuitemradio') and not row.get('disabled')]
        counts = Counter(normalize(row['title']) for row in nodes)
        for node in nodes:
            label = normalize(node['title'])
            if counts[label] != 1 or len(native[label]) != 1:
                continue
            slug = re.sub('[^a-z0-9]+', '-', label).strip('-')
            method = 'hover' if node.get('has_submenu') else 'click'
            result.append(dict(id=f'menu.{family["id"]}.{slug}.{method}', title=f'{family["id"]} menu: {node["title"]}',
                               mapping_status='candidate', reference=dict(view=family['view'], role=node['role'], label=node['title'], node=node['id']),
                               rustcord=dict(control=native[label][0], method=method, required_visible=family['required_visible'])))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--coverage', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    data = json.loads(args.coverage.read_text(encoding='utf-8'))
    if isinstance(data['reference_controls'], dict):
        reference = args.coverage.parent / data['reference_controls']['path']
        data['reference_controls'] = json.loads(reference.read_text(encoding='utf-8'))['controls']
    rows = propose(data)
    with args.out.open('x', encoding='utf-8') as stream:
        json.dump(dict(version=1, features=rows), stream, indent=2)
    print(f'{len(rows)} proposals; review context before removing mapping_status=candidate: {args.out}')


if __name__ == '__main__':
    main()
