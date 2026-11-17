# cssimpler integration

Both native desktop editors use cssimpler revision `31266eb`, which includes
captured event handlers and the transition update described in cssimpler's
`docs/reports/`. Each editor instance owns its action bitmask. Click handlers
capture a shared reference to that instance's mask; the update loop consumes its
own pending actions. Multiple instances therefore do not consume each other's
clicks. Repeated identical clicks before an update remain coalesced, as before.
An action invalidates the UI structure even while simulation is running.

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

## Native vector board (FC-0061)

The fitting editor's 2D board uses `Node::vector` and typed `VectorPath` commands
from cssimpler's tiny-skia-backed renderer. Outlines retain cubic controls until
rasterization; the board no longer formats SVG path/point strings or sends
SVG shape nodes through the parser. Millimetre display coordinates are converted
to f32 only when creating commands; authored f64 metre coordinates, semantic
identities, hit testing, transactions, undo/redo and file exports remain in the
application model. Pattern SVG import/export is independent of this display route.

`#board-2d` replaces the former SVG host. Two native leaves share the exact same
square viewBox and CSS size: `#board-2d-background` contains the background/grid/axes
and requests a bounded raster cache; `#board-2d-drawing` contains pieces, handles,
links and previews and stays uncached. Grid changes (including theme, pan, zoom
and resizing) invalidate its raster automatically. The parent is not cached, and
the application does not clear the global cache. Cache budget/ineligibility can
fall back to direct drawing. Panel drawing order is preserved.

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
