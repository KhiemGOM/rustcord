# Parity audit

Run from the repository root with Python and Pillow installed. The native suites need Windows, a desktop and the release client/mock. The full suite uses the clipboard. Native suites use the mock and fake media devices; the audit runner clears inherited `RUSTCORD_*` and `MOCK_*` settings before launching them.

```powershell
python e2e/parity.py init --out e2e/artifacts/parity-plan.json
python e2e/parity.py run --build --plan e2e/artifacts/parity-plan.json
```

The default runs the existing cold/warm/login E2E checks, read-only layout tours at three window sizes, then the performance benchmark. `--build` builds locked release binaries first; omitting it tests the existing binaries. Every suite records the client and mock hashes and rejects results if either binary changed during execution. Other agents should avoid rebuilding those binaries during an audit.

For a shorter run:

```powershell
python e2e/parity.py run --suite profile
python e2e/parity.py run --suite tour --size 1280x720@150
python e2e/parity.py run --suite bench
```

Repeat `--suite` and `--size` to select several. The profile suite checks cold and warm paths separately. Benchmark numbers are recorded without a pass claim until paired thresholds are supplied.

Every run creates a new timestamped directory below `e2e/artifacts/parity/`. `--out <new-directory>` chooses a location; an existing directory is refused. Artifacts are local and ignored by Git:

- `index.html`: searchable coverage, checks, failures and screenshot gallery.
- `report.json` and `report.md`: coverage, build identity and native suite results.
- `plan.json`: the inventory and exact comparison settings used.
- `native/<suite>/`: complete console log, results, screenshots, native measurements and mock request log. Full-suite captures live under `capture/`.
- `native/bench/capture/`: performance summary and retained raw benchmark captures.
- `case-*/`: paired evidence, hashes and highlighted image differences.

Reports are saved before testing and after each suite. A terminated audit remains marked running/untested rather than passing. The native harness retains its existing stall and runtime limits; later checks may fail because an earlier step stopped the run. Inspect the error and progress files before interpreting those as independent bugs.

Exit codes: **0** means all declared comparisons verified and selected native suites passed or recorded; **1** means a difference, comparison error, build failure or native failure; **2** means missing/untested coverage. A passing mock suite does not verify Discord parity.

## Pair evidence

`init` inventories atlas screenshots from `ref/INDEX.md`, numbered/table behaviors from `ref/QOL.md`, and explicit native-input gaps. The atlas is optional and ignored by Git; without it the inventory is much smaller. Inventory entries start untested. It does not guess which mock screenshot matches which Discord screen. The inventory covers these sources, not every possible Discord state.

Edit the generated plan to pair matching captures. Reference paths are relative to the plan directory; candidate paths are relative to the new audit directory or `--candidate`. Absolute paths work too. Each pair needs matching capture conditions. They are declarations supplied by the capture author, not independently established by the tool. Account permissions, content and open UI state belong in `scenario`; the input device and action belong in `input`. Store Discord version, Rustcord build and capture date as additional metadata alongside those fields.

```json
{
  "version": 1,
  "scope": "Onyx chat, matched fixture and native mouse wheel",
  "cases": [{
    "id": "chat-top",
    "kind": "image",
    "reference": "discord/chat.png",
    "candidate": "rustcord/chat.png",
    "reference_env": {
      "os": "Windows 11", "dpi": 144, "zoom": 100,
      "theme": "Onyx", "viewport": [1707, 913],
      "scenario": "same channel, messages, permissions and scroll position",
      "input": "native mouse, idle after opening channel"
    },
    "candidate_env": {
      "os": "Windows 11", "dpi": 144, "zoom": 100,
      "theme": "Onyx", "viewport": [1707, 913],
      "scenario": "same channel, messages, permissions and scroll position",
      "input": "native mouse, idle after opening channel"
    },
    "pixel_tolerance": 2,
    "max_changed_fraction": 0,
    "masks": []
  }]
}
```

```powershell
python e2e/parity.py compare --plan path/to/plan.json --candidate path/to/captures
```

Image comparisons never resize. Different dimensions fail. `pixel_tolerance` is the maximum channel difference ignored per pixel (default zero); `max_changed_fraction` is the allowed fraction of unmasked pixels that differ (default zero). Masks use `[x, y, width, height]` physical pixels; they are recorded in the plan and excluded from the denominator. An entirely masked image is rejected. Use narrow masks for truly variable content; masking layout differences hides bugs.

For numbers use `kind: "metric"`, JSON reference/candidate files, a dot-separated `key` such as `scroll.ui_ms_p95`, and an absolute `tolerance` in that metric's units. Comparison is symmetric: a number on either side outside the tolerance differs. Boolean and nonfinite values are rejected.

For scrolling/animation measurements use `kind: "trace"` with JSON arrays of `[time_ms, value]` (or a `key` pointing to them). Times must increase and both traces must span the same input window. The tool linearly interpolates both traces at every sample timestamp and compares maximum absolute error to `tolerance`; matching endpoints alone cannot pass different motion. This compares the supplied samples, so capture frequently enough to expose stalls. It does not manufacture native wheel/touchpad measurements from synthetic input.

`manual` entries remain untested. Native Win+V selection, touchpads, reconnects, tray behavior and unscripted states need their own reproducible capture/test adapters. This tool does not operate real Discord or automatically replay the complete atlas yet. Existing reference captures and any recordings remain inputs; it does not continuously record desktop video. Those limits stay visible as coverage gaps.
