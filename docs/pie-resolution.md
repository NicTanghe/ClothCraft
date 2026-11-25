# Physics-inspired mesh resolution: independent CPU experiment

Branch: `codex/physics-inspired-mesh-resolution`, based on upstream
`7d0f78171757ab0be538c0bb9fa1c66921bb402e`.

This is an original Rust implementation of the **material estimate and static
sizing-map/meshing pipeline** described by Zhang et al., *Physics-inspired
Estimation of Optimal Cloth Mesh Resolution*, SIGGRAPH 2025,
[DOI 10.1145/3721238.3730619](https://doi.org/10.1145/3721238.3730619).
The [full paper](https://wanghmin.github.io/publication/zhang-2025-pie/Zhang-2025-PIE.pdf)
was read, including equations and figures in sections 3--4 and Table 1.
It now includes opt-in static mesh preparation for the native simulator. It is
not a new solver backend, automatic physical calibration, a full reproduction
of the paper's simulator, or proof of optimal resolution.

## Matched samples on sewn boundaries

The static preparation introduced in [upstream PR #230](https://github.com/erythrofr/ClothCraft/pull/230)
and wired into the garment editor by [PR #8](https://github.com/NicTanghe/ClothCraft/pull/8)
now prepares both sides of an eligible seam together. The compiler uses the
same normalized rest-arclength mapping as the stitch integrator. It combines
the samples from both sides with trim endpoints, chain joints and notch
correspondences, then inserts a separate vertex on each panel at each paired
station. Reversal and easing can map a station to a different fraction of the
other edge. The pattern curves stay in material space and each panel keeps
its own vertices and stitch coupling.

The pass applies when every seam segment belongs to a polygon panel and at
least one participating panel has been prepared with seam matching. An ordinary polygon can
receive matching samples from a prepared mate. Structured rectangles and seams
between ordinary panels continue to use the existing interpolated stitch path.
The final triangulation retains the first pass's boundary samples and protects
its constraints from independent splitting. It checks area and vertex budgets
again. The angle target can be missed near protected constraints.

Apply **Physics-inspired mesh preparation → Remesh and reset** to enable
matching. Older saved preparation plans keep their mesh topology and cached
drape until preparation is reapplied.

[Seam sampling report (PDF)](reports/pie-seam-sampling.pdf) · [Typst source](reports/pie-seam-sampling.typ)

## Run a simulation example

The default cloth-over-sphere demo now exposes the same static mesher:

```sh
cargo run --locked -j 2
```

Open the right-hand **Properties** tab and find **Physics-inspired mesh
preparation** under **REST PATTERN**. The same controls and draft values are
also available in **Simulation**. Enter warp/weft E estimates in Pa, the boundary/base spacing
ratio and transition distance, then click **Remesh and reset**. This applies
the current pattern/fabric settings and returns cloth to its flat initial pose
above the same sphere. Pause and wait for an outstanding step before applying.
The mesh is compiled from the 2D rectangle with the same constrained mesher,
mass and hinge construction as polygon panels. Hanging mode retains its two
top corner pins. The optional sizing plan survives Project v1 save/load and
reset, and rebinds to the current bending/grain settings on compilation.
**Use uniform grid and reset** returns to the original Cells X/Y grid.
Historical projects without the optional field retain the original grid;
the default scene, executable and reference solver are unchanged.

For the separate garment editor and gathered example:

Generate the native project, then open it in the existing workspace:

```sh
cargo run --locked --no-default-features -j 2 --example pie_resolution -- \
  --simulation-demo output/pie-simulation/fitting-project.json
cargo run --locked -j 2 --bin cloth-fit -- \
  --project output/pie-simulation/fitting-project.json --steps 0 --view \
  --output output/pie-simulation-view
```

Press **Simulate**. The original example has a 260 mm skirt waist sewn to a
130 mm pinned waistband using the existing eased sewing objective. The panels
start outside the avatar. Its mesh is prepared before the first step and stays
fixed for the run. This is a small functional example, not a calibrated garment
or a validated wrinkle benchmark. The native window uses graphics hardware for
rendering; the default solver remains CPU unless GPU compute is explicitly set.

In **Properties > Simulation > Physics-inspired mesh preparation**, enter explicit
warp/weft Young's modulus estimates in Pa, a boundary/base spacing ratio and a
transition distance in mm. The controls are always expanded. With no polygon
selected, preparation targets all polygon pieces; select a polygon to enable
the single-piece scope. Choose one selected polygon or all polygons, then
click **Prepare mesh and reset**. This compiles a new rest mesh and resets the
cloth arrangement; it adds one history entry. Invalid input or failed compile
preserves the existing garment. **Use uniform mesh and reset** clears the plan.
Pause simulation and finish/cancel unfinished edits before preparing.

The estimated base spacings use each panel's fabric bending properties and
grain, rebound whenever it compiles. A boundary band refines all closed rims
and internal sewing paths with the requested ratio; this is an explicit
geometric choice, not automatic recognition of shirring or collision regions.
The normal compiler constructs masses, material terms, hinges, seam quadrature,
pins and arrangement. Ordinary save/open and mesh signatures include the
optional plan. Legacy settings omit it and retain the old meshing path. Reset
rebuilds deterministically; starting or continuing solver steps does not call
this mesher. No live topology adaptation or draped-state transfer was added.
Frozen or pinned configurations that cannot survive a new mesh fail explicitly
through existing compile checks. Highly sampled internal paths exceeding the
research adapter's 64-point line limit are also refused rather than truncated.

**Mesh resolution lab...** in the same inspector provides three comparison
presets: a small shirring patch, a gathered skirt panel and an anisotropic sleeve.
Click **Generate comparison** to run on a separate CPU thread, then inspect
coarse/adaptive/fine previews with one shared color scale. A narrow window uses
tabs. Changing inputs marks old results stale. Cancel or close cancels the
bounded job; a late result cannot replace the retained comparison. Optional
**Export meshes** writes to `<session output>/mesh-resolution-lab`; it does not
apply meshes to your garment. The separate preparation controls above do.

## Run the mesh-only comparison

```sh
cargo run --locked --no-default-features -j 2 --example pie_resolution -- --preset skirt --compare
cargo run --locked --no-default-features -j 2 --example pie_resolution -- \
  --input examples/pie_resolution/shirring.json --output output/pie-custom --compare
cargo test --locked --no-default-features -j 2 --test pie_resolution -- --test-threads=2
cargo clippy --locked --no-default-features -j 2 --example pie_resolution --test pie_resolution -- -D warnings
```

All commands are CPU-only. No desktop feature or GPU/window is required.
No dependency or solver changes are required. The default mesher stays unchanged.
The optional preparation plan is an additive, omitted-when-absent setting in
`MeshingSettings`. CLI, native UI and tests share `src/mesh_resolution/`.

The default original fixture is a 180 by 240 mm rectangle with an internal
120 mm constraint representing a gather line. Its base fabric has B = 2e-6
N m and E = 110000 Pa. The explicit shirring computation uses sewn/rest length
ratio 0.5, added Young's modulus zero and exponent n = 1, yielding 4.603 mm
source spacing from 9.206 mm base spacing. The transition doubles at 40 mm.
The example does not simulate the gathering forces or produce a draped garment.

The output directory receives:

- `input.json`: resolved research inputs, seed, calibration and budgets.
- `adaptive.json`: material-space vertices, triangles, per-vertex directional
  spacing, named directed constraint chains and measured mesh quality.
- `adaptive.obj`: flat geometry and UVs in metres, with aligned vertex/UV indices.
- `adaptive.svg`: millimetre-scale mesh preview; dark blue denotes finer local
  spacing and orange denotes constraints. Preview colors are scaled per mesh.
- `sizing.csv`: a 101 by 101 bounding-box sampling grid, restricted to material.
- `summary.json`: mesh counts, area, quality and complete CPU generation time.
- With `--compare`, corresponding uniform-base and uniform-fine mesh artifacts.

The JSON is a separate research input/output contract, **not** a new
`GarmentTemplate`, `FittingProject` or persistent measurement representation.
Output indices are rebuildable mesh indices, not editable semantic identities.
The mesh-only exporter does not install its output into a running simulation.
The native preparation path instead goes through the existing garment compiler
and scene replacement, rebuilding physical terms and invalidating state caches.

## Implemented method and units

`src/mesh_resolution/sizing.rs` implements equation 4:

```text
c1 * r^6 + c2 * r^4 = B/E
c1 = 32.08 m^-3
c2 = -1.876e-4 m^-1
B: N m; E: N/m^2 = Pa; r: m
```

Bisection selects the positive monotone branch over the explicit interval
[2.5, 50] mm. Inputs below/above that interval's ratios return the corresponding
endpoint spacing; invalid, zero or overflowing B/E values are rejected. Custom
coefficients must define a positive, monotone branch throughout their interval.
Warp and weft estimates use separate B/E ratios and one shared material-space
axis angle. The calibration is the paper's system-specific empirical choice;
it is not measured or fitted for ClothCraft's constitutive law.

**Do not feed ClothCraft's membrane coefficients directly into E.** They are
in N/m and have a different constitutive interpretation. Converting a sheet
modulus to a volumetric Young's modulus requires an explicit effective thickness
and a constitutive assumption. This tool requires E in Pa rather than guessing
that conversion from garment thickness or inventing calibrated fabric values.

`Fabric::shirring` implements section 4.2.2:
`r_source = rho^n * r(B/(E + E_sh))`. Here `rho` is sewn/rest length, in (0,1],
not `1-rho`. Additional E_sh is in Pa and n is positive. Local sources can
therefore be finer than the material estimator's 2.5 mm floor, as in the paper.
Research field spacings below 1 micrometre are rejected as a resource guard.
The CLI's resolved JSON accepts explicit source spacings; rerunning a JSON file
after changing its fabric does not silently recompute its authored sources.

Section 4.2.1's dimensional wrinkle scaling utilities are also available:

```text
L_tension = prefactor * lambda^2 * sqrt(T/B)
L_compression = prefactor * lambda^(3/2) * (delta/h^2)^(1/4)
```

Lambda, L and h are metres; T is tension in N/m, B is N m and delta is a
dimensionless compressive fraction. The proportionality prefactor is explicit.
Wavelength and mesh spacing are different inputs: this tool does not silently
equate them. These utilities do not infer a calibration or replace explicitly authored
transition inputs.

## Explicit independent design choices

The paper writes a transition `r + x^m` and derives `m = log(r)/log(L_w)`,
absorbing scale factors into the exponent. Those expressions depend on the
chosen numerical length unit. This implementation instead uses:

```text
h_j(p) = min(base_j, source_ij * (1 + (distance_i(p)/L_i)^exponent_i))
```

This is a deliberate normalized transition, not a literal reproduction of
that exponent fit. It doubles the source spacing exactly at the declared L,
until capped by the base spacing, and changes consistently when every length
unit changes. The default exponent is 1/2 for the tension-like demo; 2/3 is
also tested for the compression-like law. Spacing components combine by
minimum, so sources cannot coarsen the base field. Each source supplies two
directional spacings; all sources share the fabric's axis orientation.

Sources support a material-space polyline, a labelled disk region, or a
forward-facing point with a direction. The latter models the directional
influence convention described for folding points, using a half-plane gate.
That gate can be discontinuous across its plane; no smoothness is claimed there.
Region and polyline transitions are continuous. Source geometry controls the
field only; constraints in `domain.lines` must be declared separately.

The original research JSON path requires explicitly authored fold sources and
parallel constraints. The native extension below can derive endpoint sources
and parallel constraints from named fold paths, and provides an explicitly
normalized pressure sizing estimate. Curvature extrema, elbow compression,
actual fold mechanics, internal pressure forces, unexpected collision adaptation
and dynamic remeshing remain outside this tool.

## Geometry, sampling and checked bounds

`src/mesh_resolution/mesh.rs` accepts a straight sampled CCW outer ring,
clockwise holes and disjoint internal polylines, in material metres. Each ring
segment has a unique semantic name; each internal line has a name. Curves can
be sampled before supplying this input, but the tool does not change or promote
editable curve points. Current ClothCraft domain validation is reused for
holes/internal paths; additional bounded outer-ring validation prevents malformed
indexing and rejects crossings, duplicate names and segments below 1 micrometre.

The sampling process is original code informed by
[Bridson's 2007 sketch](https://www.cs.ubc.ca/~rbridson/docs/bridson-siggraph07-poissondisk.pdf).
It uses deterministic xorshift randomness, an active list and uniform-area
annular candidates in the local directional metric. Endpoint ellipse exclusion
is symmetric. Boundary and internal constraint samples are compulsory and can
be closer together than the Poisson exclusion distance. This variable-spacing
extension does not inherit Bridson's uniform-radius linear-time proof or prove
maximal coverage. A spatial grid accelerates proximity queries.

The existing pinned **Spade 2.15.1** dependency supplies constrained Delaunay
triangulation. Its base metric is fixed by the base directional spacings and
material axis. Local sizing controls sampling/refinement only: the result is
not claimed Delaunay-optimal in a varying Riemannian metric. No fabric energy
or material parameter is passed to Spade itself.

Constraint segments are subdivided until their metric length, evaluated with a
conservative lower spacing bound over the entire segment, is at most one.
Material triangles are refined at their centroids until every face satisfies:

```text
face_material_area / (h0_lower_bound * h1_lower_bound) <= sqrt(3)/4
```

The lower bounds cover a ball enclosing the whole face. Distance to each
source is 1-Lipschitz, so tiny features between ordinary sample locations are
covered. This can overrefine a face, but does not hide a source by checking
only its centroid. For directed sources, a ball intersecting the forward
half-plane conservatively includes their influence.

An exterior frame separates the convex hull from rounded constraint samples,
preventing zero-area hull slivers under rotated metrics. Frame and nonmaterial
faces are excluded. Every returned named path is reconstructed in directed
order, its endpoints are checked, and every chain link must be a real mesh
edge. Whole-domain area is checked to relative tolerance 1e-8. Exhausted point,
depth or refinement-pass budgets return an error, never a partial mesh.

The enforced size condition is **area**, not a maximum edge or minimum angle
guarantee. Actual maximum metric edge and minimum metric angle are reported;
the comparison mesh has no 25-degree angle guarantee. Static simulation
preparation additionally requests Spade angle refinement in the fixed base
metric and alternates it with spatial area checks within the same vertex/pass
budgets. Acute input angles retain the usual constrained-refinement limitations.
Variable-metric angle optimization and physical convergence remain open gates.

## Evidence and handoff

The original 12 focused CPU tests cover the published table, equation residuals,
monotonicity/clamps, invalid units and coefficients, directional estimates,
shirring ratios, wrinkle scaling powers, exact transition doubling, source
ordering, length-unit covariance, conservative region/directional bounds,
repeatable meshes, rotated axes, holes/internal constraints, area and boundary
length conservation, fixed-metric local Delaunay checks, concave material,
small features and explicit budget failure.

Table 1 and the printed equation are not numerically identical for every
rounded entry. In particular, Dark Blue's published B = 1.6e-4, E = 1.3e5 and
the printed coefficients give **18.418278 mm**, whereas the table says 18.5 mm.
The test allows 0.1 mm difference from the rounded table but separately requires
the actual equation's relative residual below 1e-12. We retain this discrepancy
rather than changing the implementation to fit that table entry.

Observed on the original demo with seed 42, CPU development profile:

| Mesh | Vertices | Triangles | Area (m2) | Min metric angle |
|---|---:|---:|---:|---:|
| Uniform base | 1,039 | 1,948 | 0.0432 | 21.37 degrees |
| Adaptive | 1,313 | 2,496 | 0.0432 | 20.72 degrees |
| Uniform fine | 4,062 | 7,866 | 0.0432 | 19.22 degrees |

Adaptive has about 68% fewer vertices than uniform fine on this fixture.
One local run measured approximately 25 ms adaptive, 10 ms uniform base and
39 ms uniform fine for complete CPU mesh generation. These are illustrative
single-run wall times, not a statistical performance result, simulation step
times or rendering timings. SVG output was rendered and visually inspected.
The initial mesh-only pass did not test cloth dynamics. The simulation integration
adds short CPU dynamics checks; wrinkle quality, GPU computation and garment-scale
speedup remain unvalidated.

CLI acceptance also replayed the saved research input and compared all three
mesh JSON files byte-for-byte after parsing: vertices, faces, boundary chains,
spacings and quality were identical. A 64-vertex budget failed before creating
the requested output directory. Focused strict Clippy and rustfmt checks passed.
The full locked CPU regression command also completed successfully:
`cargo test --locked --no-default-features -j 2 -- --test-threads=2`, including
the sizing/preparation cases and the existing library, integration and documentation tests.

Simulation/UI integration acceptance on the final branch:

- Full locked CPU suite: **505 passed**, zero failed.
- Full desktop `cloth-fit` CPU suite: **161 passed**, zero failed, two optional
  captures ignored; the new capture was also explicitly run successfully.
- The eight static-preparation integration cases cover material area/mass,
  partial sewing and names, curved boundaries, deterministic rebuild/reload,
  material/grain rebinding, stale fitting-cache rejection, history, expected
  free fall with fixed topology, and real gathered sewing with retained pins.
- Four named-widget/transport cases cover explicit generation, cancellation,
  stale exports, narrow layouts, numeric validation, text focus and paused-only
  preparation. The CPU-rasterized egui preview was visually inspected at
  `output/pie-preview/mesh-lab.png`. It is not a native GPU/window capture.
- Strict Clippy passed for the changed binary, example and integration targets.
  Broad `--all-targets` Clippy remains blocked by the unchanged
  `src/authoring.rs:731` `items_after_test_module` lint on the current toolchain;
  the reserved authoring file was not modified to hide or fix that unrelated
  failure. The binary recursion limit was raised to 256 to let the compiler
  evaluate the existing wgpu worker's `Send` requirements without its reported
  recursion-depth warning; no lint was suppressed.

The native gathered example also completed a short 12-step CPU CLI run.
This verifies initialization, accepted steps and output with the prepared
mesh; it is an early assembly transient, not a settled fit/strain or wrinkle
accuracy result. GPU simulation and native-window interaction remain untested.

Original implementation and original synthetic fixture; no authors' code,
paper figures, garment assets or EXO source were copied. Existing project
licensing remains unchanged; Spade remains the existing declared dependency.
The next useful experiment is matched rest geometry and physical parameters
on uniform-base, adaptive and uniform-fine meshes, comparing area/mass,
boundary/seam lengths, static deflection, wrinkle measurements and full-step
cost. Do not enable material-driven sizing by default before that evidence.

## Native mesh controls and authored feature hints (FC-0064)

The active `cloth-fit --view` CSSimpler garment window now has a **Mesh
preparation** section in the right property editor. Earlier component-only
controls were not connected to this runner. This section uses the existing CPU
static mesher, independently of the selected simulation backend.

The **Coarse–Fine** control has 17 logarithmic click/drag stops: spacing goes
from 4× at Coarse through 1× at the centre to 0.25× at Fine. Uniform mode uses
`uniform base spacing × scale`; physics mode uses `estimated warp/weft spacing
× scale`. The factor changes discretization, not E, bending, density or grain.
Current vertex/triangle counts appear in the status bar after preparation;
there is no speculative count or simulation-speed prediction.

Choose all polygon pieces or only the selected polygon piece for base settings.
Choose a named boundary, hole boundary or internal line on the selected piece
for feature settings. Enable the desired switches, enter their parameters,
then click **Prepare physics mesh and reset**. Features on other named paths
are retained. Disable a path's switches and apply to remove its hints.
**Use uniform mesh and reset** clears physics sizing (including hints) for the
chosen scope and applies the displayed effective uniform spacing.
**Reload settings from selected piece** discards numeric/toggle drafts and
reads its saved configuration. Changing pieces reloads that piece's settings.
Changing the slider, typing or selecting paths does not compile/reset a mesh.

Preparation requires a paused, idle worker and no active authoring draft,
drag or preview. Compilation must succeed before document/history/scene changes.
Success is one undoable transaction, clears saved state/signature, increments
worker generation and resets the complete drape to the authored arrangement.
Invalid values, unresolved paths, impossible rails or resource exhaustion keep
the existing document. Save/Open persists the sizing settings and path hints.
Old plans default to scale 1, smooth transitions enabled, and no feature hints.

### Feature definitions and limits

These are original, static mesh-sizing implementations informed by Zhang et al.,
sections 4.2.2–4.2.5 and Figures 8–11. The paper's “Ours-I/II” labels vary between
figures, so the controls describe behaviors instead of exposing those labels.

- **Smooth refinement transitions** uses the existing normalized distance law.
  When disabled, sources use a hard refinement band out to their transition
  distance and then return to base spacing. The hard-band comparison is an
  independent design choice, not an exact reproduction of Figure 8.
- **Refine fold endpoints** places forward-facing sources at the two ends of
  the named path, pointing inward along the path. Source spacing is base
  spacing multiplied by `fold angle / 360`; the angle is deviation from flat
  (1–180 degrees). The transition exponent is 2/3. No automatic curved-fold
  curvature extrema discovery or physical crease energy is added.
- **Add parallel fold lines** creates actual temporary constrained mesh paths
  on both sides of an **internal** fold line, with the specified offset from
  its centreline (0.1–10 mm; default 0.5 mm). Bounded miter offsets support up
  to 64 sampled points. Sharp reversals, crossings, holes/outside paths and
  exhausted budgets fail explicitly. Rails are neither persisted geometry nor
  new sewing/fold forces. Authored named paths stay unchanged.
- **Gathering refinement** uses the existing `Fabric::shirring` estimator:
  `sewn/rest ratio × estimate(B / (E + added E)) × scale`, with independent
  warp/weft added-modulus estimates in Pa. Ratio 1 means no compression;
  0.5 means the intended sewn length is half the rest length. These estimates
  do not change seam rest lengths or apply a shrinkage force.
- **Down-fill sizing estimate** uses the same ratio-based source spacing,
  together with **Sizing pressure (Pa)** and **Pressure reference (Pa)**.
  Let `q = pressure / reference`, restricted to 0–2, and `f = 2^q`. This
  implementation retains transition exponent `0.5` and shortens transition
  distance to `L / f`. Increasing pressure therefore never refines the field
  further: it makes the same seam wrinkles dissipate over a shorter distance. Pressure zero matches the gathering estimator with zero added E.
  This is an explicitly normalized, unit-consistent analogue of the paper's
  empirical pressure response, **not its literal dimensional power fit**.
  Down-filled compartments are designated by their stitched boundary/internal
  paths; adding multiple paths models multiple compartment edges. No region
  discovery, enclosed-volume pressure, inflation or physical down model exists.

Overlapping hints combine by the minimum spacing, so they can only refine the
base field. The existing conservative area bound, angle checks and vertex
budget remain in force. Hints reference stable path IDs, resolved against the
current sampled 2D geometry each compile; missing paths are rejected. Unknown
future collisions are not predicted. This remains static preparation, not the
2026 r-adaptive method tracked in
[issue #7](https://github.com/NicTanghe/ClothCraft/issues/7).

Focused verification: `cargo test --locked --no-default-features -j 2 --test
mesh_features --test mesh_preparation --test pie_resolution --test
reference_mesh_preparation -- --test-threads=2` and `cargo test --locked
--features desktop -j 2 --bin cloth-fit native_mesh_ -- --test-threads=2`.
These checks establish geometry/field/transaction properties on synthetic
fixtures, not calibrated wrinkle accuracy or garment-scale solver speedups.
