"""Checks for the composer's spellcheck and Discover > Servers, driven by spell_discover.rs in
src/ui/e2e: what the client drew, and what it asked the mock for."""


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    got = lambda method, part: [r for r in reqs if r.get("method") == method and part in r.get("path", "")]
    return {
        "spellcheck: misspelled words got squiggles and a menu with suggestions, Add to Dictionary and Spellcheck": both("spell_squiggles") and both("spell_menu"),
        "spellcheck: a suggestion replaced the word, Add to Dictionary stopped the squiggle": both("spell_replace") and both("spell_learn"),
        "spellcheck: the Spellcheck switch turned the squiggles off and back on": both("spell_off") and both("spell_on"),
        "discover: Servers drew the featured cards from /discoverable-guilds, with the categories": both("discover_home")
            and len(got("GET", "/discoverable-guilds?offset=0")) >= 2 and len(got("GET", "/discovery/categories")) >= 2,
        "discover: a category tab and a search asked the search endpoint": both("discover_category") and both("discover_search")
            and len(got("GET", "category_id=1&")) >= 2 and len(got("GET", "query=rust")) >= 2,
        "discover: clicking a server joined its preview as a lurker": both("discover_preview")
            and len(got("PUT", "/guilds/9000/members/@me?lurker=true")) >= 2,
    }
