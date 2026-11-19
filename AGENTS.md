# Rustcord

Native Discord desktop client in Rust (egui + glow), Windows only. Goal: pixel and behaviour parity with Discord's desktop app.

## Build and run
- `cargo run --release` runs the client; `cargo build --locked --release --bin rustcord --example mockcord` is what CI builds.
- Compiling is the heavy part of working here, not testing: the client is one 120k-line crate, and release is fat LTO on one codegen unit, so a one-line edit costs about 8 minutes and 3 GB there. The loop:
  1. Pure logic gets a unit test, run alone in the dev profile: `cargo test --bin rustcord <module or test name>`. Never `--release` for tests.
  2. What needs the running client gets an e2e step, built on the `iter` profile (no LTO, incremental: the same edit took 23 s and 1.5 GB): `cargo build --profile iter --bin rustcord --example mockcord`, then `python e2e/from_step.py <step> --profile iter` for the group you touched.
  3. Release is built once, when the change is finished: `python e2e/run.py` before the commit or the hand-back. Don't rebuild release between small edits.
  4. No `cargo check` or clippy before a build or test of the same code, which type-checks it anyway. One build or e2e run at a time on a machine.
- `python tools/build_cost.py <command>` prints a command's time and the peak memory of its process tree; the numbers above came from it.
- Unit tests (`cargo test --bin rustcord`) cover pure logic such as the image cache and the emoji in names. The main verification is the e2e harness: `src/ui/e2e.rs` (steps run inside the client), `e2e/*.py` (checks), `examples/mockcord.rs` (fake Discord REST, gateway, CDN, voice with real DAVE).

## E2E
- `python e2e/run.py` builds, then runs the client cold and warm against the mock. `--no-build` reuses what is built. `--profile iter` builds and runs `target/iter` instead of `target/release`; `from_step.py` and the other scripts that start the mock take it too. It needs a real window and GPU.
- Off release the startup and frame time budgets (`NEEDS_RELEASE` in `run.py`) are listed under `skipped`, like the checks that need `--focus`; only a release run judges them.
- Output: `e2e/artifacts/report.json` (`checks`: name -> bool, plus `cold`/`warm` results), `report.md`, screenshots, `mock_requests.jsonl`. The exit code is 1 if any check is false.
- Checks are named strings in `e2e/run.py`, `look.py`, `polish.py`, `qol.py`, `home.py`. Most are `marks_ms` set by `e2e.rs` steps, or assertions on the mock's request log.
- `RUSTCORD_E2E_FROM=<step>` jumps to that step once the first channel has loaded (the groups are in the dispatch in `src/ui/e2e.rs`), to rerun one group against the mock without the rest.
- The run replaces the clipboard (the paste step) and picks random ports, so parallel runs are fine.
- Mock alone: `target/<profile>/examples/mockcord.exe <http> <ws> <log.jsonl> [voice]` (the voice port takes tcp and udp). Point the client at it with `RUSTCORD_API=http://127.0.0.1:<http>/api/v9`, `RUSTCORD_CDN`, `RUSTCORD_EMOJI_CDN`, `RUSTCORD_FONTS`, `RUSTCORD_GATEWAY=ws://127.0.0.1:<ws>`, `RUSTCORD_TOKEN`, `RUSTCORD_DATA`. `run_client` in `run.py` has the full list.
- Real-account checks never run from `run.py`; they need an env var and the saved login (or `RUSTCORD_TOKEN`):
  - `RUSTCORD_TOUR=<dir>` walks the main screens and settings pages and screenshots them into `<dir>`, results in `<dir>/results.json`. Add `RUSTCORD_READONLY=1` so nothing that changes the account goes out. `RUSTCORD_TOUR_CHANNEL` and `RUSTCORD_TOUR_FORUM` take `guild/channel`.
  - `RUSTCORD_E2E=<dir> RUSTCORD_LIVE=<voice channel id>` joins that channel and goes live for `RUSTCORD_LIVE_SECS` (default 30). Never run it without the user's say-so: it is visible to others.
  - Both also run against the mock (use the env above), which is how to check them.

## Perf
- `python e2e/bench.py` measures startup, idle and busy CPU, frames drawn and who asked for them, the cost of one frame, and memory (with what each image loader holds), against the mock at a real account's size (`--guilds 115`: 120 servers, 14k channels). Output: `e2e/artifacts/bench.json`. Run it before and after anything that touches the frame loop, the store or repaints; single runs of idle CPU are noisy (the GPU driver spins on vsync), compare a few.
- Iterate on the `fast` profile (`cargo build --profile fast`, `python e2e/bench.py --profile fast`): thin LTO and incremental, so a one-file rebuild took ~7 min where release's fat LTO took ~20, and it measured within ~10% of release, and it keeps line tables for a profiler. Only the final before/after goes on a real `release` build. `exe_mb` from a fast build isn't comparable (unstripped).
- `bench.json` has `cpu_by_thread` (per thread name, `main` is the oldest) and `app_cpu_pct` (without the bench's own threads). Between runs, whole-process CPU on one machine swings wider than most single changes; compare thread by thread, alternate before/after in one session, and keep a copy of the old exe to run against.
- `python e2e/vs_discord.py prep`, restart, then `python e2e/vs_discord.py run --channel <guild>/<channel>` measures Rustcord against the installed Discord on the real account, both from outside the same way: launch to settled picture cold and warm, then idle, wheel scrolling, Alt+Down through channels and minimized, with CPU, memory and GPU summed over each client's processes. It takes the real mouse and keyboard and puts each client in front, so only run it with the user away and their say-so. Output: `e2e/artifacts/vs-discord/<time>/report.md`.
- Direct3D 12 instead of Vulkan was tried on an NVIDIA laptop (RTX 4060): warm first frame ~30% sooner and ~48 MB less video memory, but the driver's work moves onto the main thread (scrolling: main 32% vs 38% of a core, about the same in total). Reverted, Vulkan stays; AMD and Intel weren't measured.

## Parity
- Judged against Discord's own CSS tokens (surface colours, 0.122 hover overlay, metrics in `look.py` and `run.py`) and live captures of the real client, not by taste. Commit messages and comments cite "live captures".
- `ref/` (captures, `QOL.md`) is gitignored and not in a fresh clone.

## Why CI only builds
`ci.yml` builds on push and PR. The e2e is `workflow_dispatch` only: the runner renders in software (Mesa llvmpipe), so frame and startup timing checks are flaky there.

## Traps
- eframe stops calling `ui()` while the window is minimized, and Rustcord skips drawing while it sits in the tray; only `logic()` runs then. Anything that has to keep up with the gateway (notifications, the taskbar badge, the tray icon, global keybinds) belongs in `logic()`.
- `src/ui/e2e.rs` is compiled into every build, release included; it only activates when `RUSTCORD_E2E`, `RUSTCORD_TOUR` or `RUSTCORD_BENCH` is set.
- `App.store` is a `Tracked` wrapper: taking it mutably drops the sums worked out from the whole store (server badges, the mention count). Code that runs every frame reads first and takes `&mut` only when it changes something.
- Every `request_repaint()` is two frames, and an idle window should draw only for the caret. Ask for repaints only for what's on screen; `bench.json`'s `repaints_asked_by` says who did.
- A step that can't find its widget waits 20s, then fails the whole run: every later check in that run reads false.
- An activity's web view is a native window over the call's stage, so egui can't draw above it: `ui/activity/view.rs` hides it whenever a modal, menu or popup is open. Its page and SDK bridge never see the account token; the bridge's rules are unit-tested in `bridge.rs`, the mock's activity page is `examples/fixtures/activity.html`. `RUSTCORD_E2E_FROM=2200 RUSTCORD_E2E_TO=1` runs just the activity steps.
- A video embed's web view (`webwin/frame.rs`, `ui/embedplay.rs`) lives in a window of its own that is only the part of the embed the chat shows, so Windows clips it; `embedplay::place` runs at the end of `ui()` and drops it unless the chat drew its embed that frame. The e2e screenshot for it (`embed.rs`) lifts the window to the top for a screen grab, since egui's own screenshot leaves native windows out. `RUSTCORD_E2E_FROM=3300 RUSTCORD_E2E_TO=1` runs just those steps.
- Older commits carry `Co-Authored-By: Claude` trailers; the commit-msg hook rejects them now. Don't copy them.
- Lines in `e2e/*.py` that look like tautologies are often the only guard for a mark set far away in `e2e.rs`; trace the mark before deleting.
- Subjects are short lowercase phrases in the project's vocabulary ("go live", "dave", "the rail"); bodies state the root cause.
