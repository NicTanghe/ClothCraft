# USD document format (editable templates and projects)

Status: S1 of [#51](https://github.com/NicTanghe/ClothCraft/issues/51). `openusd` is a normal dependency; there is no feature gate. The bundled templates in `examples/templates/*.usda` and the app catalog are USD.

JSON remains readable as a legacy import. Templates are saved as USD only;
fitting projects still choose their encoding by extension.

| document | load | save |
|---|---|---|
| `GarmentTemplate` | `.usda`/`.usd`, or legacy `.json` | `.usda`/`.usd` only; JSON is rejected |
| `FittingProject` | `.usda`/`.usd`, or legacy `.json` | `.usda`/`.usd` when the path says so, JSON otherwise |

`GarmentTemplate::save` returns an error for a non-USD path.
`FittingProject::load`/`save` keep the extension dispatch so existing scripts
that expect `fitting-project.json` still work.
`cloth-template convert input.json output.usda` migrates a template.

## What is stored

The document is the same serde model as JSON, mapped losslessly:

- objects become `Xform` prims;
- scalar fields become typed attributes (`bool`, `int`, `int64`, `uint`, `double`,
  `string`), arrays of scalars become typed arrays (`double[]`, `int64[]`,
  `double2[]`, `double3[]`, `string[]`, ...);
- arrays of objects become child prims of an array container prim;
- maps become child prims keyed by the original string;
- every child prim carries `custom token clothcraft:kind`
  (`object`, `array`, or `view`) and `custom string clothcraft:key` (the original
  field or map key). These two attributes are structural and skipped when
  reading values.

The layer declares:

```text
defaultPrim = "World"
metersPerUnit = 1
upAxis = "Y"
customLayerData = { dictionary "clothcraft:document" = { version = 1, kind = "clothcraft" } }
```

The namespaced metadata key is quoted. A bare colon is legal in a namespaced
property name but not in a bare USDA dictionary key; Pixar OpenUSD and Houdini
reject `dictionary clothcraft:document = {`. The writer quotes the key even
though `openusd` 0.8's text writer emits it unquoted, so the file opens in
`usdcat`, `usdview` and Houdini as well as in ClothCraft.

Loading rejects a different `metersPerUnit`, a different up axis, an unsupported
document version, and a missing envelope. Values are never scaled.

## Derived panel scene

So that a DCC or `usdview` shows the garment without ClothCraft, every panel prim
also carries a derived scene next to the lossless document attributes:

```text
/World/Parts/<hex panel id>            Xform: placement
    Fill                               Mesh: compiled rest triangulation
    Boundary                           BasisCurves: cubic Bézier boundary
```

- **Placement.** The panel prim has `matrix4d xformOp:transform` and
  `uniform token[] xformOpOrder = ["xformOp:transform"]`. The matrix is the
  authored `Placement` exactly: panel-local `(u, -v, 0)` is rotated by
  `R = Rx Ry Rz` and translated, which is the pre-simulation arrangement. The
  matrix is rowwise (USD's row-vector convention), with the translation in the
  last row. `rotation_degrees` and `translation` remain document attributes on
  the `Placement` child prim.
- **Fill.** `/Fill` is a `Mesh` of the compiled rest triangulation from
  `PolygonPattern::build` or `Pattern::build`. `points` is `point3f[]` in
  panel-local metres, `faceVertexCounts` is all triangles, `faceVertexIndices`
  are panel-local transient indices, and smooth `normal3f[]` normals use
  `vertex` interpolation. `primvars:st` is the standard `texCoord2f[]` copy of
  the material coordinates, while `custom double2[] clothcraft:restUV` keeps
  them exactly. `custom string clothcraft:panelId` repeats the semantic panel
  ID. `subdivisionScheme = "none"`, `orientation = "rightHanded"` and
  `doubleSided = true` are explicit. This is display geometry, not a new
  triangulation path.
- **Boundary.** `/Boundary` is a `BasisCurves` with
  `basis = "bezier"`, `type = "cubic"`, `wrap = "nonperiodic"` and
  `curveVertexCounts = [4, 4, ...]`, one 4-point curve per named edge. For v1
  rectangular panels the four derived sides use `top`, `right`, `bottom`,
  `left`. `points` is the standard `point3f[]` interop copy,
  `custom double3[] clothcraft:controlPoints` is the exact f64 copy in the same
  panel-local frame, and `custom string[] clothcraft:boundaryIds` matches curve
  order.

The views are derived on save and skipped on load; they also carry
`custom token clothcraft:kind = "view"`. The template attributes remain
authoritative, and `PolygonPattern::build` is still the only curve-to-mesh
converter. Loading ignores `xformOp:transform` and `xformOpOrder` so a
round trip does not grow document fields.

Holes, internal lines, notches, allowances and measurements are not yet
authored as USD curve or marker views; the `Fill` mesh includes the triangulated
holes, and the remaining draft data stays in the template attributes.

## Limitations

- No registered custom schema: attributes are namespaced by convention. A typed
  schema, unit prompts and incremental stage authoring are follow-up work in #51.
- The derived `Fill` and `Boundary` views are display-only; the viewport still
  consumes the compiled mesh and does not read views back as geometry.
- The whole fitting project, including the f64 drape state, is serialized; there
  is no transient session layer yet. The derived panel scene uses the template
  placement, not the cached draped state.
- Only USDA text is written. `.usd` is written as text; USDC/USDZ are not
  supported.
- No size or depth limit is applied by this reader yet; documents come from local
  files.
- In the fitting editor, Save writes `fitting-project.usda` in the session's
  output directory when the session has no source file. A session opened from
  a project saves back to that path. The `cloth-fit` command still writes
  `fitting-project.json` for scripts that consume its output.

## Validation

```powershell
cargo test --locked --no-default-features -j 2 --lib usd_document
usdcat examples/templates/tshirt.usda
```

The tests cover template and project round trips (including f64 control points
and drape state), row-vector placement equivalence, one `Fill`/`Boundary` view
per panel, quoted metadata keys, JSON/USD file dispatch, foreign unit rejection
and unknown version rejection. `usdcat` is the independent OpenUSD syntax check.
