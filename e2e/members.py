"""Checks for members_steps in src/ui/e2e/members.rs: the server settings Members table used for
real against the mock's member state, and what the client put on the wire for each action."""


def checks(runs, reqs, data):
    cold = runs["cold"].get("marks_ms", {})
    has = lambda *names: all(n in cold for n in names)
    sent = lambda method, tail, body="": any(r.get("method") == method and r.get("path", "").split("?")[0].endswith(tail) and body in (r.get("body") or "") for r in reqs)
    return {
        "members: the table lists all seven members, sorts oldest first, searches by username and clears the search": has("mm_rows", "mm_sort", "mm_search", "mm_search_cleared"),
        "members: the signals filter shows only the timed out member, then only the unusual DM one": has("mm_signal_timed", "mm_signal_unusual"),
        "members: a role added from the plus and removed with the chip's dot reached the mock and the table": has("mm_role_added", "mm_role_removed")
            and sent("PUT", "/members/106/roles/1101") and sent("DELETE", "/members/106/roles/1101"),
        "members: prune asked how many first (days 7), then removed the two members with no roles": has("mm_prune") and sent("GET", "/prune") and sent("POST", "/prune", '"days":7'),
        "members: a timeout from the row menu sent communication_disabled_until and the table shows it": has("mm_timeout") and sent("PATCH", "/members/102", "communication_disabled_until"),
        "members: a kick and a ban from the row menu left the table without that member": has("mm_kick", "mm_ban")
            and sent("DELETE", "/members/105") and sent("PUT", "/bans/104", "delete_message_seconds"),
        "members: bulk ban of the owner was refused and the row stayed": has("mm_bulk_ban_owner_refused") and sent("POST", "/bulk-ban/v2", "101"),
        "members: bulk kick of two selected rows removed both": has("mm_bulk_kick") and sent("DELETE", "/members/102") and sent("DELETE", "/members/103"),
    }
