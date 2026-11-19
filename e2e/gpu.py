"""Checks for gpu_steps in src/ui/e2e/gpu.rs: losing the graphics device."""


def checks(runs):
    both = lambda m: all(m in r.get("marks_ms", {}) for r in runs.values())
    return {
        "gpu reset: a lost graphics device is replaced in place and the window draws as it did before": both("gpu_reset_redraws"),
    }
