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
- `assertions.json`: every evaluated native assertion, its suite, area and outcome. The HTML report shows a searchable area matrix and individual assertion table. Failed/unreached assertions stay separate from confirmed Discord differences; one early stopped step can prevent many later assertions from being reached.
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

## Long usage campaign

```powershell
python e2e/campaign.py --hours 16 --out e2e/artifacts/usage-campaign
```

The discovery-first campaign uses seven of every ten task slots for 800-action exploratory sessions with new seeds. Each cycle also has a layout tour and two validation slots; full cold/warm/login regressions and performance sampling run every fourth cycle, with restart/persistence checks in the intervening cycles. These are task proportions, not fixed wall-clock percentages. Tours cover fifteen size/zoom combinations across 940x500, 1280x720 and 1707x913 at 75%, 100%, 125%, 150% and 200% zoom. This changes app zoom, not the Windows display DPI or input device.

Exploration favors less-visited instrumented controls for each observed state and action type. It collects both the main UI targets and controls registered through the shared E2E marker (including inner buttons, dropdowns, toggles and inputs). It uses clicks, right clicks, hover, middle clicks, typing, shortcuts, scrolling and resizing, all inside the local mock. Delays vary from immediate actions to two-second pauses, with the chosen delay recorded for each action. Each action records the visible instrumented controls and state fingerprints; the next record supplies the state observed after that action. New states and periodic checkpoints get screenshots. The raw `actions.jsonl`, seed, executable hashes and mock requests remain in each task folder. Exploration checks completion/recording; it does not establish that every action behaved like Discord.

The exploratory selector excludes host startup settings, logout, checkout, external authorization and upload/download picker actions. Scripted suites cover some related flows using fixtures; excluded OS paths and real payments are not verified by the campaign. The visible-control logs retain excluded controls, so selection coverage can be inspected rather than assumed.

The campaign report counts observed fingerprints and state/action transitions separately from assertions and reference coverage. Fingerprints derive from the current route and instrumented widget names; they do not distinguish every possible visual, animation, server or semantic state. It does not infer a reference match from novelty. Logged input is injected into the app, so native OS clipboard history and device gesture paths remain separate coverage gaps.

`corpus.json` carries state/action visit counts between exploratory sessions, so each seed does not forget which interactions earlier seeds exercised. The controller migrates earlier run histories when resuming an older campaign, and refuses to accept a client that ignores its supplied history. New-state and new-target columns distinguish discovery from repetition. History currently deprioritizes previously visited pointer targets; keyboard/text action families are still seeded sampling, not globally coverage-guided. Controls without instrumentation and semantically different states with identical fingerprints remain gaps.

Rerun a campaign seed with `python e2e/stress.py --actions 800 --corpus <task>/native/stress-17/corpus-input.json 17` (replace `17` with the recorded seed). Each native run snapshots and hashes its initial history, so subsequent exploration does not overwrite the replay input. Keep the same executable and fixture versions. The seed reproduces the generator; asynchronous UI/network timing can still change which controls are available, so retain the original action log and screenshots when investigating a failure.

The budget counts completed test execution time, finishing the current task before stopping; it can exceed sixteen wall-clock hours. Failures are retained and later tasks continue. Three consecutive infrastructure errors or less than 1 GiB free disk space stop the campaign without removing evidence. The controller holds an OS lock to prevent two runs from sharing its output folder.

While the controller runs, it requests that Windows keep the system and display awake for rendering. This is a temporary thread request, released on exit; it does not change the power plan or prevent manual sleep/shutdown.

To stop after the current task, create `<campaign-directory>/stop`. To resume, remove that file yourself, then run:

```powershell
python e2e/campaign.py --resume --out e2e/artifacts/usage-campaign
```

Interrupted tasks retain their old folders and are retried into new ones. Resuming uses the original time budget. `index.html` refreshes every thirty seconds; completed task results are saved after every task. `campaign.json` records the controller PID, attempts and outcomes. A campaign is a sampling tool, not proof of every interaction or years of behavioral equivalence.

## App surface coverage

```powershell
python e2e/app_coverage.py --campaign e2e/artifacts/usage-campaign --out e2e/artifacts/usage-campaign/app-coverage --watch
```

This read-only observer can run beside an existing campaign without restarting it. It updates once per minute and stops after the campaign finishes, blocks or stops. Create `<coverage-directory>/stop` to stop just the observer. `watcher.json` records its PID, last generation time and state.

The report inventories public settings/server/channel page enums, literal E2E marker identifiers, named capture checkpoints throughout the app, runtime-discovered markers and the saved Discord atlas. Dynamic marker callsites are listed as an open-ended inventory. Source extraction is best effort; uninstrumented screens and interactions can be absent, so it does not report a whole-app coverage percentage. This is product surface coverage, not executable line/branch coverage.

Control rows represent instrumentation identifiers, including possible aliases and non-interactive layout markers. They are not a count of distinct app features.

`discord.html` is the Discord-first feature/interaction matrix; `discord.json` is its data export. Its initial explicit bindings live in `e2e/discord_features.json`, selecting captured Discord states and, for interactions, a role/label or exact node plus a Rustcord identifier and required input method. Equal labels across unrelated screens are not automatic bindings. Multiple matching nodes within a reference state require disambiguation. The initial binding set is partial; all captured reference states remain listed with their unbound control counts.

`input_attempted_in_mock` is recorded Rustcord input evidence, while `rustcord_view_captured` is surface-family capture evidence. Neither verifies parity. A hover does not cover a click, an engine-setting assertion does not establish a UI click, and a page capture does not cover every control or substate. Missing input evidence is not proof that a feature is unimplemented. Inspect the referenced captures and action-log lines to review a binding; actual Discord is not being driven live by this observer.

Bindings can require `rustcord.required_visible` marker identifiers. The collector matches these against the exact action record; a matching control used in another menu does not satisfy the binding. `context_not_exercised` means the required record is absent. Reference `.menu.json` files supply semantic menu-item IDs, disabled/checked states and submenu flags. Disabled entries and background text are excluded from menu proposals; submenu proposals require hover. Ownership, permissions and checked-state equivalence remain unverified.

Generate review candidates with `python e2e/propose_discord_features.py --coverage <coverage-directory>/coverage.json --out <new-proposals.json>`. The generator uses declared menu families, unique reference menu items and unique Rustcord identifiers. Its output is marked `mapping_status: candidate`, which cannot receive coverage credit or reduce unmapped-view counts. Review the reference item identity and recorded context before promoting a candidate into `discord_features.json`; some text matches, such as quick-reaction icons, need separate confirmation.

`observed` means a marker appeared; `interacted` means an input targeted it; `captured` means a matching page/checkpoint screenshot file exists. These statuses do not prove the control worked. The report lists pointer methods, observed marker-state changes, source locations, action logs and capture variants. Requested size/zoom variants are read from tour folder names; inspect the raw results for measured rendering conditions. Navigating toward a page does not count as a screenshot of that page.

Discord reference candidates are inventoried separately and stay `unmapped`. They are grouped by role/label for navigation, with per-capture pages retaining their rectangles and context. Shared labels do not identify the same feature or establish parity. `reference-inventory.json` retains all candidate instances; `coverage.json` contains the current Rustcord evidence and a pointer to that inventory. The dashboard paginates rows and caches unchanged logs/reference data to avoid repeatedly parsing the atlas. Partial action-log tails are left pending rather than counted as completed actions.

## Actual Discord exploration

The reference recorder drives the official Discord web app, independently of Rustcord and the mock. It uses the existing local Rustcord login and cached channel inventory. Node 24, Python and Playwright are required. Install the recorder's browser without collecting other agents' browser installations:

```powershell
npm install --prefix e2e/artifacts/discord-tooling playwright
$env:PLAYWRIGHT_SKIP_BROWSER_GC = '1'
node e2e/artifacts/discord-tooling/node_modules/playwright/cli.js install chromium
node e2e/discord_explore.cjs --playwright e2e/artifacts/discord-tooling/node_modules/playwright --out e2e/artifacts/discord-reference --hours 16 --headless
python e2e/discord_journeys.py --ref ref --live e2e/artifacts/discord-reference --out e2e/artifacts/discord-journeys
```

Exploration seeds the home view and one non-NSFW, non-archived channel for each cached channel-kind and owner/member/private context. A seed is a requested route, not proof that its content loaded or that its permissions match another channel. User settings get their own exploration root. Visible controls, their values, selection/checked/disabled states, nested menus and unknown controls enter the backlog. The queue balances surface families and explores new observed states to the depth limit. Rows of one list (channels, messages, members, friends, servers) count as one thing tried per family, so the hundredth channel's menu waits behind the first click on every named control; at most two rows of a kind are queued per state. While a menu, dialog or listbox is open only the controls inside it are queued. Each discovered scroll container gets targeted wheel probes, including sidebars and panels below the first viewport. Unknown buttons and menu items can be clicked; destructive/sensitive actions and dialogs, external links, upload/download actions, voice joins and payments remain excluded. Disabled controls can be hovered. Every discovered control stays in `frontier.json`, including exclusions and controls not reached before the budget.

All non-GET/HEAD/OPTIONS HTTP requests are aborted, including analytics and challenge writes. HTTP reads are limited to Discord domains. Gateway frames are limited to heartbeat, identify, resume and read subscriptions; presence-update and voice-state opcodes are dropped. Service workers are disabled, other sockets are closed, popups/downloads/file pickers are cancelled, and media devices are fake with audio muted. This is a separate ephemeral browser context; it does not modify the user's browser profile. Copy actions may replace the clipboard. Normal authenticated connection/session metadata can still reach Discord. Blocked operations do not establish successful server-side behavior.

Each input has contemporaneous before/after screenshots, accessibility trees and semantic state JSON; `actions.jsonl` retains the target, exact setup/input, context check, blocked requests and outcome probe. The whole browser session is recorded in `video/`. Source hashes, browser/Discord builds, budgets and requested viewport are in `run.json`. Replays require matching route, open layers and target identity/state; unrelated member-list changes do not invalidate an otherwise matching target. A list row or message is identified by its Discord id, because its label carries unread state and who is in voice; a member by display name. Each replay first asks Discord's own router for the route and reloads the page only when the context then differs, since every load is a new gateway session on the account (`run.json` counts `loads`; `reloaded_after` on an action says why). A setup that still fails after a reload drops every queued task that shares it (`unreachable`). A click that flips a switch or a show/hide button is clicked back (`flipped_back`), because that state outlives a page load. Keyboard probes also require matching composer context. Replays that lose context, failed inputs and no-observable-change results stay separate.

Outcome probes currently check tab selection, context/submenu appearance, Escape dismissal, scrolling and composer autocomplete/newline behavior. They are reference-side probes: a failed check can expose an unsupported expectation, absent context menu or recorder issue. It is not a Rustcord defect. Other outcomes remain `unreviewed`. Rustcord needs an explicitly paired journey and matching outcome assertions before any parity claim. No whole-app coverage percentage is measured.

Use `--actions`, `--depth`, `--width`, `--height` and `--routes <file.json>` for bounded pilots or different conditions. A routes file contains objects with `route` and `family`; routes must be Discord channel paths. The default limit is 10,000 actions and depth four. Sixteen hours is an upper budget, not padding: exhaustion, three consecutive page loads that fail or less than 1 GiB free disk ends the run earlier. A named-pipe lock prevents concurrent writers to one output folder. Create `<recorder-directory>/stop` for a clean stop. Remove it yourself before `--resume`; source/policy changes require a new run. Resumption retains the original budget, viewport, evidence and frontier.

`discord_journeys.py --watch` refreshes its report every thirty seconds until the referenced runs finish. The report also inventories the atlas's actual before/after pairs, complete nested menu trees, hover sweeps and source-only QoL gaps. These evidence types are counted separately and are not distinct feature counts. `--plans <directory>` can attach archived action/setup plans by exact before/after capture names. Plans are not proof that the inputs executed; ambiguous plans stay ambiguous. Source-only obligations and missing replay recipes stay open. This recorder does not cover native desktop chrome, every permission/content variant, real server writes, real voice/media success or years of timing-dependent behavior.

## Pairing Rustcord with Discord

```powershell
python e2e/discord_pair.py journeys --live e2e/artifacts/discord-reference --out e2e/artifacts/journeys.json
python e2e/discord_pair.py replay --journeys e2e/artifacts/journeys.json --out e2e/artifacts/rustcord-replay
python e2e/discord_pair.py diff --live e2e/artifacts/discord-reference --replay e2e/artifacts/rustcord-replay --out e2e/artifacts/pair
```

Every recorder capture also holds each text Discord drew: its box, font, size, weight and colour. `journeys` turns each recorded input into one Rustcord can repeat, aimed by Discord id where the control has one (`chan:`, `guild:`, `friend:`, `msg:`), by a named rect for icon buttons, by its visible text, and at Discord's own coordinates when nothing else matches. `replay` runs them in Rustcord with `RUSTCORD_REPLAY`, read-only, on the saved login, in a scratch data directory, at the recorder's 1707x913 at 150%; each journey starts from a cleared view. After every input Rustcord writes `<id>.<n>.scene.json` (every text and filled rect it painted, and its named rects) beside a screenshot. `replay.json` says how each input was aimed.

`diff` pairs the two clients' texts state by state and writes `differences.json` and `index.html`: text missing from Rustcord or only in it, font, size, colour and width differences, `spacing` where the gap to the previous matched text changes (one finding where a divider is missing, not one per row under it), and `nothing changed in Rustcord` where Discord drew new text after the input and Rustcord drew none. The same finding in many states is one row with a count. Journeys whose starting screens share under half their text are left out and listed.

To work on one finding, `python e2e/discord_pair.py show --live <recorder> --replay <replay> --state action-000014 --side before --box 376,60,1331,853 --crop voice.png` prints both clients' texts inside that box with their boxes, sizes, weights and colours, Discord's controls and Rustcord's named rects there, and saves the two screenshots' box side by side, Discord on the left.

What it cannot see: icons, images, fills and borders (the scene has Rustcord's rects, Discord's side has none yet), animation and timing. Live content moves between the two captures, so chat rows can differ by a message. The web client has no window buttons, so its title-bar and popout positions sit about 138 px right of Rustcord's. A thread the fresh store has not loaded is skipped (`route is not in the store`). A finding is a lead to check against the two screenshots, not a verdict.

Recorder checks run with `node --test e2e/test_discord_reference.cjs`. Set `DISCORD_AUDIT_PLAYWRIGHT` to the absolute Playwright module directory to include the real-browser fixture test. Python report checks run with the other `test_*.py` tool tests.
