# Rustcord

Native Discord desktop client in Rust (egui + glow), Windows only. Goal: pixel and behaviour parity with Discord's desktop app.

## Build and run
- `cargo run --release` runs the client; `cargo build --locked --release --bin rustcord --example mockcord` is what CI builds.
- A few unit tests (`cargo test --release --bin rustcord`) cover the image cache. The main verification is the e2e harness: `src/ui/e2e.rs` (steps run inside the client), `e2e/*.py` (checks), `examples/mockcord.rs` (fake Discord REST, gateway, CDN, voice with real DAVE).

## E2E
- `python e2e/run.py` builds, then runs the client cold and warm against the mock. `--no-build` reuses `target/release`. It needs a real window and GPU.
- Output: `e2e/artifacts/report.json` (`checks`: name -> bool, plus `cold`/`warm` results), `report.md`, screenshots, `mock_requests.jsonl`. The exit code is 1 if any check is false.
- Checks are named strings in `e2e/run.py`, `look.py`, `polish.py`, `qol.py`, `home.py`. Most are `marks_ms` set by `e2e.rs` steps, or assertions on the mock's request log.
- `RUSTCORD_E2E_FROM=<step>` jumps to that step once the first channel has loaded (the groups are in the dispatch in `src/ui/e2e.rs`), to rerun one group against the mock without the rest.
- The run replaces the clipboard (the paste step) and picks random ports, so parallel runs are fine.
- Mock alone: `target/release/examples/mockcord.exe <http> <ws> <log.jsonl> [voice]` (the voice port takes tcp and udp). Point the client at it with `RUSTCORD_API=http://127.0.0.1:<http>/api/v9`, `RUSTCORD_CDN`, `RUSTCORD_EMOJI_CDN`, `RUSTCORD_FONTS`, `RUSTCORD_GATEWAY=ws://127.0.0.1:<ws>`, `RUSTCORD_TOKEN`, `RUSTCORD_DATA`. `run_client` in `run.py` has the full list.
- Real-account checks never run from `run.py`; they need an env var and the saved login (or `RUSTCORD_TOKEN`):
  - `RUSTCORD_TOUR=<dir>` walks the main screens and settings pages and screenshots them into `<dir>`, results in `<dir>/results.json`. Add `RUSTCORD_READONLY=1` so nothing that changes the account goes out. `RUSTCORD_TOUR_CHANNEL` and `RUSTCORD_TOUR_FORUM` take `guild/channel`.
  - `RUSTCORD_E2E=<dir> RUSTCORD_LIVE=<voice channel id>` joins that channel and goes live for `RUSTCORD_LIVE_SECS` (default 30). Never run it without the user's say-so: it is visible to others.
  - Both also run against the mock (use the env above), which is how to check them.

## Perf
- `python e2e/bench.py` measures startup, idle and busy CPU, frames drawn and who asked for them, the cost of one frame, and memory, against the mock at a real account's size (`--guilds 115`: 120 servers, 14k channels). Output: `e2e/artifacts/bench.json`. Run it before and after anything that touches the frame loop, the store or repaints; single runs of idle CPU are noisy (the GPU driver spins on vsync), compare a few.

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
- Older commits carry `Co-Authored-By: Claude` trailers; the commit-msg hook rejects them now. Don't copy them.
- Lines in `e2e/*.py` that look like tautologies are often the only guard for a mark set far away in `e2e.rs`; trace the mark before deleting.
- Subjects are short lowercase phrases in the project's vocabulary ("go live", "dave", "the rail"); bodies state the root cause.
