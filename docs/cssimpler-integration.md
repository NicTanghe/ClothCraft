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

Validation for this migration:

- `cargo check --locked -j 2 --all-targets` and `cargo build --locked -j 2` passed.
- `cargo test --locked -j 2 --test test_2d_pattern_board_proportions -- --test-threads=2`: three passed, covering native leaves/control precision, paint/theme/pan/zoom/cache reuse, and alignment with hit testing in split/2D layouts at three window sizes.
- `cargo test --locked --no-default-features -j 2 -- --test-threads=2`: 658 passed, one ignored.
- The cloth-fit unit suite has 19 passes and three failures: the CAD toolbar's clipped Round button, a top-bar button outside the window, and a headless theme click. An unchanged archive of main `551aa48`, with its original cssimpler pin, reproduces all three at identical bounds/click assertions (19 passes, three failures). The square-board test is renamed for the native host; its failure is the existing toolbar assertion, after the board bounds assertions pass.
- Production desktop Clippy (`--lib --bins -D warnings`) passed. Strict all-target Clippy remains blocked by pre-existing range-loop lints in editor tests and `tests/corner_tools.rs`; repository-wide rustfmt reports existing import-order differences. Targeted rustfmt and `git diff --check` passed.
- A standalone 600 × 600 CPU raster capture was inspected. No native window, GPU workload or frame-cadence benchmark was run.
