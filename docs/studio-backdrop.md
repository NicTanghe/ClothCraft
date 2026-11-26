# Studio infinity cove

The native 3D viewport authors a display-only studio backdrop at
`/InfinityCove`. It fixes issue #68's visible backdrop without touching the
fitting scene, cloth, avatar or contacts.

## Geometry

`usd_scene::infinity_cove_mesh` builds a seam-free cyclorama from three parts:

- a floor disk out to a 22 m inner radius;
- an 8 m quarter-circle fillet;
- a cylindrical wall at 30 m radius and 40 m height.

The floor is at world `Y = 0`, so the avatar's feet and the garment arrangement
line up with it. The wall surrounds the subject, so an orbit cannot reveal an
edge behind the avatar. The mesh is low-poly (128 angular segments, 10 fillet
steps) and carries smooth vertex normals.

This mesh is published only in the transient viewport stage next to `/Collider`
and garment panels. It is not part of `FittingScene`, has no thickness and is
never used for collision. Camera framing still uses the garment and avatar
bounds, not the cove.

## Theme

`StudioBackdropTheme` maps the runner's light/dark theme to a constant
`primvars:displayColor`:

| Theme | Display colour |
| --- | --- |
| Dark | `(0.12, 0.135, 0.16)` |
| Light | `(0.84, 0.85, 0.87)` |

Toggling the theme updates the authored attribute; the renderer's stage revision
re-extracts the mesh, so the colour changes without rebuilding the application
or reloading the scene.

## Rendering limits

The cove receives the studio key's shadows on devices with sampled-depth
support. The 2048×2048 map fits the casters in light space by default. Settings
→ 3D shadows → Coverage widens that fit within a small range near the
subject. Expansion follows the floor footprint, independent of the cove walls'
height. Resolution controls the
number of pixels per side. Wider coverage spreads the texels over more space
and softens fine shadow detail. The cove stays out of the depth pass at all
settings. The sidewall fade described
in issue #68 would require per-mesh opacity and remains deferred.

Tests in `tests/usd_scene.rs` cover the cove bounds, topology and normals, the
two theme colours, live display-colour updates, and that the cove is absent from
the fitted garment scene. Validation: 724 CPU tests passed with one ignored
preview, the twelve desktop import UI tests passed, CPU-only and desktop
all-target Clippy with warnings denied passed, the desktop build and formatting
passed, and the native `--ui-smoke --cpu` capture was inspected for both the
cove and the unchanged avatar/garment framing.
