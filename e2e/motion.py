"""Checks for the animations, driven by src/ui/e2e/motion.rs: the curves against their
control points, a modal seen mid-scale and then settled, and Reduced Motion landing it at once."""


def checks(runs, reqs, data):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    return {
        "motion: ease-out at .5 is .6846 and the 500/18 spring first reaches 1 at .097s": both("motion_curves"),
        "motion: the settings window was caught mid-scale and then settled at full size": both("motion_modal_mid") and both("motion_modal_settled"),
        "motion: with Reduced Motion on, the settings window opened at full size on its first frame": both("motion_reduced_instant"),
    }
