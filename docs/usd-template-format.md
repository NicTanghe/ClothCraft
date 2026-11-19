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
customLayerData = { "clothcraft:document" = { version = 1, kind = "clothcraft" } }
```

Loading rejects a different `metersPerUnit`, a different up axis, an unsupported
document version, and a missing envelope. Values are never scaled.

## Part curves

For every polygon panel the writer also authors a derived view:

```text
/World/Parts/<hex panel id>/Boundary   (BasisCurves)
```

- `basis = "bezier"`, `type = "cubic"`, `wrap = "nonperiodic"`;
- `curveVertexCounts = [4, 4, ...]`, one 4-point curve per named edge;
- `points` is the standard `point3f[]` interop copy;
- `custom double3[] clothcraft:controlPoints` is the exact f64 copy;
- `custom string[] clothcraft:boundaryIds` matches curve order.

The view is derived on save and ignored on load. The template attributes remain
authoritative, and `PolygonPattern::build` is still the only curve-to-mesh
converter.

## Limitations

- No registered custom schema: attributes are namespaced by convention. A typed
  schema, unit prompts and incremental stage authoring are follow-up work in #51.
- `BasisCurves` are written but not rendered by the viewport and not read back as
  geometry; holes and internal lines currently exist only as template attributes.
- The whole fitting project, including the f64 drape state, is serialized; there
  is no transient session layer yet.
- Only USDA text is written. `.usd` is written as text; USDC/USDZ are not
  supported.
- No size or depth limit is applied by this reader yet; documents come from local
  files.
- The default fitting-project save filename remains `fitting-project.json` so existing scripts keep working. Templates never use JSON; pass an explicit `.usda` path for project USD output.

## Validation

```powershell
cargo test --locked --no-default-features -j 2 --lib usd_document
```

The tests cover template and project round trips (including f64 control points
and drape state), JSON/USD file dispatch, foreign unit rejection and unknown
version rejection.
