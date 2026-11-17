# CSS runner refactor: changes and decisions

Date: 17 November 2026  
Scope: [PR #5](https://github.com/NicTanghe/ClothCraft/pull/5), FC-0062

## Outcome

The fitting editor's 9,040-line `css_runner.rs` has been replaced by a 62-line
`css_runner/mod.rs` facade and implementation files grouped by responsibility.
The change improves navigation and gives later cleanup work clear boundaries.
It is a structural refactor: the total implementation still exists, and no
rendering-speed improvement is claimed.

Existing public imports remain available, including `CssRunnerState`, action
flags, app creation, UI construction, geometry helpers and `native_board::*`.
The native vector renderer and its cached grid/uncached drawing arrangement from
PR #4 are retained. UI labels/layout, authoring transactions, document formats,
worker behavior, simulation logic and dependency versions are unchanged.

## What moved

| Group | Responsibility |
| --- | --- |
| `app.rs` | Native app construction, viewport hookup and startup |
| `state.rs` | Shared editor state, selection helpers and simulation controls |
| `actions.rs` | Action flags, queued operations and frame updates |
| `documents.rs` | Save/export methods on the existing state type |
| `editing.rs` | Authoring transactions, history and edit previews |
| `board/` | Coordinate conversion, geometry, picking, linked selection, corner tools and native drawing |
| `input/` | Event routing plus keyboard, button, motion and scroll handlers |
| `ui/` | Shell, viewport/CAD toolbar, top bar, tool rails, asset browser, inspector, palette and status |
| `tests/` | Existing board, shell and camera regressions, with shared test support |

Paths above are relative to `src/atelier/css_runner/`. The detailed
[module map](../css-runner-structure.md) identifies the individual files.

## Decisions and reasons

| Decision | Reason and consequence |
| --- | --- |
| Split responsibilities before extracting widgets | Moving existing code is easier to review separately from changing UI construction patterns. Widget design can now start from clearer ownership boundaries. |
| Keep a public facade | Existing callers and integration tests retain their import paths. Internal file organization can evolve without requiring caller rewrites. |
| Keep `CssRunnerState` together initially | Splitting the state and its ownership at the same time would add behavioral risk. Per-instance action queues, worker mailbox, document/history and transient selection/drag data keep their existing owners. |
| Preserve method-style save/export | These methods move into `documents.rs` through another implementation block for the same state type. Call sites and state effects remain the same. |
| Extract existing input match arms | The router retains focus/resize handling and delegates other events. Original event patterns, early returns and capture/release behavior are preserved. |
| Use explicit imports and restricted helper visibility | Dependencies are visible in each file. Shared private helpers are accessible within the runner without becoming new external APIs. |
| Move native drawing helpers into `board/native.rs` | Drawing primitives sit beside board rendering. A re-export preserves the earlier `native_board` path. |
| Move tests with their responsibilities | The tests become easier to locate while retaining their assertions and shared fixture behavior. No test or lint gate is disabled. |

The existing dead-code allowance remains at the runner boundary. Narrow
unused-import allowances apply to facade re-exports because this runner is
compiled both as a library API and within `cloth-fit`; some exports are used only
by library callers/integration tests. Implementation imports remain checked by
strict Clippy.

## Validation and evidence

| Check | Observed result |
| --- | --- |
| Full desktop suite | 771 passed, zero failed, 27 ignored |
| CPU-only suite | 658 passed, zero failed, one ignored |
| All-target compile check and desktop build | Passed |
| Strict all-target desktop and CPU Clippy | Passed |
| Repository formatting and diff whitespace checks | Passed |
| Factory validation | Queue valid; all eight factory tests passed |
| One-off move audit | 83 function/test bodies, 15 state methods and seven event match arms compared unchanged after normalizing formatting and qualified module paths |

The regression suites cover editing gestures, draft cancellation, undo/redo,
instance-local handlers, layout/click bounds, camera controls and native-board
rendering/cache alignment. Existing native-board integration tests also exercise
the preserved public import paths. Ignored GPU checks remain opt-in; no native
window/GPU acceptance or frame-cadence benchmark was run for this refactor.

Reproduction commands from the repository root:

```sh
cargo check --locked --all-targets -j 2
cargo build --locked -j 2
cargo test --locked --features desktop -j 2 -- --test-threads=2
cargo test --locked --no-default-features -j 2 -- --test-threads=2
cargo clippy --locked --features desktop --all-targets -j 2 -- -D warnings
cargo clippy --locked --no-default-features --all-targets -j 2 -- -D warnings
cargo fmt --all -- --check
python3 -m unittest discover -s scripts -p 'test_factory.py'
python3 scripts/factory.py check
```

At report creation, all six CI jobs had passed on implementation revision
`01cb49a4aee651f5cf145a1e67ecf9359cafe9b2`: CPU, desktop, Linux build, macOS
build, Windows build and portable Windows preview. Adding this report creates a
new PR revision. Merge requires all six checks to complete successfully again on
that final revision, with the expected head verified immediately before merging.

## Remaining work

Some functions remain substantial, particularly pointer-button handling, board
scene construction and property accordions. They are now in focused files; this
stage does not claim to have decomposed every function or removed all coupling
through shared state.

The next stage should extract a small set of ordinary Rust UI builders: tool
buttons, property rows, numeric decrement/value/increment controls, toggle groups
and collapsible sections. Document transactions and simulation decisions should
stay in application code. Start with the CAD toolbar and inspector, preserve
instance-local callbacks, and review that abstraction work separately.
