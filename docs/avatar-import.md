# Static avatar import

Use **Assets & Pieces → Import → Avatar** in the fitting editor, then **Choose…** to select a local OBJ, glTF, GLB, FBX or USD file.
The confirmation shows the source, vertex and triangle counts, bounding box,
dimensions in metres and geometry-loss warnings. The action row stays outside
the scrolling details, including at 480 × 360.

**Replace avatar** keeps authored panels, seams, materials, placements and live
simulation settings. It resets the draped positions/velocities to the authored
initial arrangement, clears cached mesh state, unfinished edits and undo history,
and marks the document modified. Save the current drape first if it is needed.
Replacing an avatar is not currently an undoable authoring command.

Opening the importer pauses simulation without changing its worker generation.
A previously running step can still finish normally. Choosing a file only
prepares an isolated candidate. Cancel, Escape, invalid input or failed scene
compilation never install that candidate. Successful installation advances the
generation so a late result from the old avatar cannot overwrite the new scene.
Closing the native window dismisses import/template modals before the ordinary
Save/Discard/Cancel document-close flow; a modal must not bypass that protection.

## Geometry and files

- glTF/GLB uses the existing static geometry importer, including node transforms
  and mirrored winding. Coordinates remain metres, right-handed and Y-up.
  A declared default or sole scene is required; ambiguous scenes are rejected.
  There is no scene selector yet. Animation, skinning and unsupported extensions
  remain explicit errors; see `src/gltf_avatar.rs` for the complete subset.
- OBJ uses the existing parser and interprets coordinates as metres and Y-up.
  No height guessing, scaling or axis conversion occurs. Use triangulated OBJ;
  polygons use the existing fan triangulation, not a general concave triangulator.
- FBX uses an in-tree FBX 7.x reader for both the ASCII and binary containers
  (`src/fbx_avatar.rs`). Only `Geometry` objects of type `Mesh` with
  `Vertices` and `PolygonVertexIndex` are read. `Connections` place geometry
  under its `Model`s, and `Lcl Translation`, `Lcl Rotation`, `Lcl Scaling`,
  `PreRotation` (when `RotationActive` is set) plus the geometric transforms
  compose the static hierarchy. `UnitScaleFactor` is converted from centimetres
  per unit to metres, and the authored `UpAxis`/`FrontAxis`/`CoordAxis` basis is
  rotated to Y-up. Binary zlib property arrays are decoded. FBX 6.x is rejected
  explicitly because its container and scene layout differ. Non-default rotation
  pivots/offsets and `InheritType` variants are reported rather than applied, so
  an unusual hierarchy may need a DCC-side transform reset before import.
- USD (`.usd`, `.usda`, `.usdc`, `.usdz`) is composed by the pure-Rust
  [`openusd` 0.8](https://crates.io/crates/openusd) crate; `src/usd_avatar.rs`
  is the policy layer over it. Every visible `UsdGeom Mesh` prim with `points`,
  `faceVertexCounts` and `faceVertexIndices` is flattened. The static
  `xformOpOrder` stack supports `transform`, `translate`, `translateX/Y/Z`,
  `scale`, `scaleX/Y/Z`, `rotateX/Y/Z`, the six Euler orders, `orient`,
  `!invert!` entries and `!resetXformStack!`. Stage `metersPerUnit` is applied,
  and `upAxis = "Z"` is rotated to Y-up. USD's own fallbacks apply when the
  stage authors neither: 0.01 m per unit and Y-up, with the unit fallback
  reported. `orientation = "leftHanded"` reverses winding, and invisible mesh
  sub-trees are skipped with a report. Non-mesh prims, instancing, animation,
  skinning, UVs, normals, materials and textures are not imported; a
  subdivision mesh is read as its control cage and reported, and meshes with a
  non-default `purpose` are imported with a report because proxy/render helper
  geometry may then be included.
- Multi-mesh sources are listed in the confirmation with each mesh's name,
  vertex and triangle count (FBX `Geometry` objects and USD `Mesh` prims). The
  mesh with the most triangles is selected by default as the collider; enable any additional
  surface that should collide and leave helper meshes such as Mixamo's
  `Alpha_Joints` off. Deselecting every mesh is rejected. OBJ and glTF/GLB
  import as a single mesh.
- Only surface positions and triangles are retained. Materials, textures, UVs,
  authored normals and object identities are lost. This is an avatar collider
  import, not a material-aware interchange workflow. Polygon faces use fan
  triangulation in every format, so use triangulated sources for unambiguous
  geometry. Zero-area triangles produced by that triangulation are dropped and
  counted in the confirmation warnings; non-finite coordinates and invalid
  indices remain errors.
- Import validates finite, nondegenerate geometry; it does not repair initial
  intersections, close holes or prove coherent winding. Check the arrangement
  before starting the simulation. Collision and solver algorithms are unchanged.

The importer writes a canonical OBJ into an owned temporary directory, without
overwriting the source. Its lifetime follows the active document and survives
template replacement and Save As. Save/recovery use the existing v1 portable
asset writer, copying the OBJ into the project's adjacent `.assets` directory.
Saved projects therefore survive temporary-directory cleanup and relocation.
Keep the project JSON and its `.assets` directory together.

Before installation the candidate scene is compiled against the current project
and checked with the same initial-state validation the VBD/OGC solver uses.
When a panel starts inside the new avatar - including a triangle crossing with
no vertex inside, which is hidden behind the body - that panel is rigidly moved
outward along Z in 1 cm steps, up to 8 cm, until the initial state is separated.
The confirmation and the status line report which panels moved and by how much,
together with a note that the 2D board layout does not move cloth in 3D.
Authored pins or frozen panels are never moved automatically: those cases keep
the arrangement, report the conflict and still require manual adjustment. A
multi-mesh source is re-compiled when the reviewer toggles a mesh, so the check
always reflects the current collider selection.

Preparation and compilation are synchronous CPU operations; dense assets can
pause the UI. The existing glTF resource limits apply to the expanded geometry:
2 million vertices, 4 million triangles, finite coordinates and nondegenerate
final triangles. OBJ input is additionally limited to 64 MiB. FBX and USD use
the same 64 MiB root-document limit and the same expanded-geometry limits; FBX
compressed arrays are length-checked before allocation. USD composition follows
the layer's own references and payloads, and those referenced layers are not
counted against the root-document byte limit, so this is a bounded local import,
not a sandbox. A background import worker, mesh simplification and native visual
acceptance remain future work.

## Tests and handoff

`src/avatar_import.rs` owns staging and candidate preparation, independently of
egui/wgpu, and dispatches to `gltf_avatar`, `fbx_avatar` and `usd_avatar` by
extension. `src/atelier/css_runner/ui/import_dialog.rs` owns the confirmation
controls; `src/atelier/css_runner/imports.rs` installs the candidate and retains
the staged asset. The existing worker-generation contract is reused without
modification.

```sh
cargo test --locked --no-default-features -j 2 --test avatar_import -- --test-threads=2
cargo test --locked --no-default-features -j 2 --test avatar_formats -- --test-threads=2
cargo test --locked -j 2 --lib atelier::css_runner::tests::imports -- --test-threads=2
```

The CPU fixtures independently specify transformed coordinates and winding,
preserve source bytes, and save/reload after dropping the staging owner. The UI
tests drive the actual named controls and production installer for cancellation,
errors, confirmation, generation invalidation and a small window. Native OS file
pickers and GPU rendering are outside that harness and are not claimed as tested.
No third-party source or assets are copied by this implementation.

The FBX and USD fixtures in `tests/avatar_formats.rs` are generated by the
tests, not copied from third-party assets. They independently state the
coordinates, units, axis settings and transforms: ASCII and zlib-compressed
binary FBX, a translated model chain, FBX 6.x rejection, zero-area triangle
reporting, USDA and USDC stages with a non-metre `metersPerUnit`, the USD
centimetre fallback, Z-up rotation, left-handed winding and invisible-mesh
skipping. USDZ is exercised by packaging the USDC fixture through `openusd` and
re-importing it.

Local validation, 19 November 2026: `cargo test --locked --no-default-features
-j 2 -- --test-threads=2` passed 715 tests with one ignored opt-in preview and
no failures across the workspace, including the twelve `avatar_formats`
integration tests, the five existing `avatar_import` tests and the FBX/USD
library tests. `cargo clippy --locked --no-default-features --all-targets -- -D
warnings` passed. The desktop `cargo build --locked -j 2` and all-target desktop
Clippy with warnings denied passed, and the nine
`atelier::css_runner::tests::imports` UI tests passed through the production
installer. `cargo fmt --all --check` passed. No GPU or native window ran.

Local validation, 20 November 2026 (multi-mesh colliders and arrangement
preflight): the CPU-only suite passed 722 tests with one ignored opt-in preview
and no failures, including fifteen `avatar_formats` tests and the new
multi-mesh/conflict regressions; the eleven
`atelier::css_runner::tests::imports` desktop UI tests passed through the
production installer. CPU-only and desktop all-target Clippy with warnings
denied, the desktop build and `cargo fmt --all --check` passed.
