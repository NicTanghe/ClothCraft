# cssimpler integration

Both native desktop editors use cssimpler revision `cb805c0`, which includes
captured event handlers, typed UI actions, frame-aligned pointer scheduling and
the transition update described in cssimpler's `docs/`. The fitting runner
delivers commands through one per-instance
`cssimpler::ActionQueue<RunnerAction>`. The queue is FIFO and
duplicate-preserving: two Step clicks enqueue two commands, and a command that
carries a payload shares the same channel as a unit command. This replaces the
former `AtomicU64` action bitmask and the separate
`Mutex<Vec<RunnerUiOp>>` payload inbox. Actions enqueued while an update is
processing a batch wait for the next invocation. Multiple editor instances own
separate queues and therefore do not consume each other's clicks. A nonempty
batch still requests a structural invalidation, so the runner keeps explicit
refresh control instead of adopting the convenience constructor's conservative
full refresh for every batch.

The day/night switch uses CSS transitions. The pinned cssimpler can retain an
active transition through unrelated scene changes when it can match the affected
branches safely. Matching remains conservative for changed sibling lists without
unique IDs, and equal-length anonymous siblings are still matched by position;
this is not a complete browser transition model. Avoid relying on positional
identity when inserting or reordering animated siblings. Its report's synthetic
measurements are not FoldCraft frame timings. Captured handlers allocate shared
handler storage when the tree is built; no application performance gain is claimed.

The simulation worker clock remains independent of rendering. CPU-only and
headless desktop checks do not demonstrate native GPU viewport behavior; that
requires a separately authorized window/GPU run.

During an active layout transition, a paint-only refresh must use the transition's
destination layout as its template. Earlier cssimpler revisions reused the
intermediate sampled layout, so repeated simulation status updates could keep
the theme switch from reaching either endpoint. The upstream fix covers `App`
and `FragmentApp`, including partial refreshes; FoldCraft retains paint-only
worker updates and does not add an animation timer.

A subsequent upstream optimization reuses an existing transition plan when a
refresh changes only content and preserves target geometry, style and identity.
See cssimpler PR #10 and its CPU benchmark; geometry or style changes still use
the conservative rebase path. Native frame cadence has not been measured here.

## Frame-aligned pointer input

The runner opts into `App::with_frame_aligned_input(true)` and feeds every engine
event to `FrameAlignedDrag`. Pointer motion and wheel events still update runner
state immediately (hover, board drags, freehand samples, camera orbit), but the
event handler returns `false` for those events and marks the runner dirty. The
next displayed-frame update consumes one `begin_frame` snapshot and performs one
structural rebuild for all samples that arrived since the previous frame. Press,
release, click, keyboard, text and focus events keep immediate paints.

Number-field drags use the same boundary. Motion only records the latest pointer
position; `tick_number_drag` applies the accumulated delta once per displayed
frame. Release flushes the final delta immediately. Cursor recentering stays local
to ClothCraft because cssimpler does not expose relative pointer capture; the
coalescing and scheduling come from cssimpler. Frame-aligned input is opt-in per
app, so the Studio V2 editor keeps its existing immediate event path.

## Export patterns from the fitting editor

**Export** opens a save dialog for a single-sheet true-scale SVG. It uses the
same millimetre sheet layout, draft markings and 100 mm calibration ruler as
the CLI print export. CSSimpler streams the sewing outlines with their cubic
controls to a buffered file. The export uses the committed material geometry
and is independent of board pan, zoom and theme. Print at 100% and verify the
ruler. The asset browser also offers a separate OBJ save dialog for the current
3D garment.


## Native vector board (FC-0061)

The fitting editor's 2D board uses `Node::vector` and typed `VectorPath` commands
from cssimpler's tiny-skia-backed renderer. Outlines retain cubic controls until
rasterization; the board no longer formats SVG path/point strings or sends
SVG shape nodes through the parser. Millimetre display coordinates are converted
to f32 only when creating commands; authored f64 metre coordinates, semantic
identities, hit testing, transactions, undo/redo and file exports remain in the
application model. Pattern SVG import/export is independent of this display route.

`#board-2d` replaces the former SVG host. Two native leaves share the exact same
viewBox and CSS size matching the full canvas: `#board-2d-background` contains the background/grid/axes
and requests a bounded raster cache; `#board-2d-drawing` contains pieces, handles,
links and previews and stays uncached. Grid changes (including theme, pan, zoom
and resizing) invalidate its raster automatically. The parent is not cached, and
the application does not clear the global cache. Cache budget/ineligibility can
fall back to direct drawing. Panel drawing order is preserved.

The runner retains the background paths (canvas, grid, axes) in its state and
clones them into each view while the theme and computed view box are unchanged.
This keeps cssimpler's shared `VectorGeometry` identity stable, so the cached
background layer survives hover, selection and preview rebuilds. Theme, pan,
zoom, layout or resize changes produce a new key and rebuild the background
geometry; the drawing leaf remains uncached.

The API has solid paints and round caps/joins. Dashed guides are explicit M/L
segments with continuous dash phase; a dashed fillet preview still samples its
curve to place those dashes. Solid panel boundaries and selected curved edges
use native cubic commands. The native renderer remains CPU-based; this is not
a GPU board implementation or a measured frame-rate claim. UI structure
invalidation and full view construction remain separate future work.

Validation:

- Full desktop suite: 771 passed, 27 ignored (opt-in GPU checks remain ignored).
- CPU-only suite: 658 passed, one ignored.
- The board regressions cover exact cubic controls/native leaves, CPU paints in both themes with pan/zoom/cache reuse, and alignment with hit testing in split/2D layouts at three window sizes.
- The full editor tests retain their bounds, click, cancellation, undo/redo and USD-occlusion assertions; the CI cleanup below resolves their prior failures.
- Strict all-target desktop and CPU Clippy, repository formatting and queue validation pass. The existing manifest-level unused `rfd` warning remains informational.
- A standalone 600 × 600 CPU raster capture was inspected. No native window, GPU workload or frame-cadence benchmark was run.

### Opaque fills and scene picking

CSSimpler PR #34 accelerates fully covered opaque solid spans in the renderer.
The 2D board can use this for eligible paints, including its background.
Antialiased edges and translucent paints still use the general compositor.
The upstream benchmark measures static scenes with layer caching disabled.

CSSimpler PR #35 adds picking by painted path and paint order. ClothCraft's CAD
tools use material-space geometry and prioritize corner handles, anchors, and
selected-panel edges. Display paths also include the grid, seam guides, and
previews. The CAD picker retains its existing semantic targets and priority.

## CI cleanup

The follow-up fixes the previously reproduced UI failures without dropping their
bounds, click or USD-occlusion assertions. The top bar uses a shorter app title
and action labels, retaining full titles/shortcuts, and hides redundant button
labels at compact widths. Object-browser tabs use short labels with full titles.
Template metadata and piece identifiers stack inside the narrow sidebar; fabric
metadata wraps instead of painting across the native 3D host.

The CAD bar chooses more conservative row breakpoints, with a seven-row layout
for narrow split panes. Its returned height and CSS height agree so the board's
coordinate mapping stays aligned with hit testing. The existing tests cover the
Round action, all CAD controls, window/layout variants and undo/redo.

Repository formatting now follows the installed stable Rust formatter. Strict
Clippy findings are fixed with iterator-based assertions and typed RGBA chunks;
no lint suppression, disabled test or reduced CI scope is introduced.

The fitting runner's implementation is grouped by state/actions, editing, board,
input and UI category. See [the module map](css-runner-structure.md) when locating
code; its public `atelier::css_runner` entry points remain available.

## Settings and 2D zoom speed

The fitting editor has **Settings** immediately after **Export** in the top bar.
Its only entry is **2D zoom speed**, for mouse-wheel and trackpad zoom. The default
is 1.5× the earlier sensitivity (one positive wheel line changes zoom by 4.5%
instead of 3%). The −/+ controls adjust it in 0.25 increments between 0.25× and
4×; Reset restores 1.5×. Changes apply immediately to the current editor instance.
This initial setting is session-local: it is not written into garment documents
or retained between launches. Toolbar/keyboard zoom steps and 3D zoom retain
their existing behavior.

Close, Escape or a click outside dismisses the panel. Board pointer/wheel input
and editor keyboard shortcuts are blocked while it is open; settings do not
create history entries or mark the document modified. Opening the panel also
closes the solver menu/palette and clears pending edge double-click tracking.
The closed panel builds no menu nodes; the feature retains just an open flag and
a numeric speed value in runner state, with existing per-instance operation
queues handling its controls.

Headless settings tests click the real controls at 1600×980 and 1280×800, check
Close/Escape/outside-click dismissal, ensure modal input does not edit the board,
and verify line/pixel scroll equivalence, cursor anchoring, speed/zoom bounds,
Reset and unchanged 3D zoom. The complete desktop suite passes 775 tests (27
opt-in checks ignored); CPU-only passes 658 (one ignored). Strict desktop/CPU
Clippy, build, formatting and all eight factory tests pass. No native-window or
GPU acceptance was run for this settings change.
