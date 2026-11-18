"""Checks for survey_steps in src/ui/e2e/survey.rs: discord's survey notice."""


def checks(runs, reqs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    seen = any(r.get("method") == "POST" and "/users/@me/survey/2024-11_blocking/seen" in r.get("path", "") for r in reqs)
    return {
        "survey: discord's notice right of the rail with its prompt and button, and closing it hides that survey": both("survey_notice") and both("survey_closed") and seen,
    }
