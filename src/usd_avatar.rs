//! Bounded static-avatar geometry import from Universal Scene Description.
//!
//! Container decoding and composition are delegated to the pure-Rust
//! [`openusd`] crate, which reads USDA text, USDC binary and USDZ archives
//! without a C++ USD installation. This module is a policy layer on top of it:
//! it selects visible triangle meshes, applies the static transform stack and
//! normalizes the result to the editor's metres / right-handed Y-up frame.
//!
//! # Supported subset
//! - Visible `Mesh` prims only. `points`, `faceVertexCounts` and
//!   `faceVertexIndices` are required; a prim missing either topology array is
//!   skipped with a warning. Faces are fan-triangulated, matching the OBJ
//!   importer. `orientation = "leftHanded"` reverses winding.
//! - Stage `metersPerUnit` scales the result; the USD fallback of `0.01`
//!   (centimetres) applies when the layer authors no value. Stage `upAxis`
//!   `Y` is kept and `Z` is rotated to Y-up; any other token is rejected.
//! - The static `Xform` stack from the pseudo-root to the mesh is composed from
//!   `xformOpOrder`: `transform`, `translate`, `translateX/Y/Z`, `scale`,
//!   `scaleX/Y/Z`, `rotateX/Y/Z`, the six Euler orders (`rotateXYZ`, ...),
//!   `orient`, inverted ops (`!invert!...`) and `!resetXformStack!`.
//!   Unsupported ops, or a stack whose names the reader cannot resolve, are
//!   reported in the warnings instead of silently changing the placement.
//!
//! # Losses and limits
//! This is static geometry for an avatar collider, not a USD interchange
//! pipeline. UVs, normals, materials, textures, purpose, instancing, skinned
//! binding, animation, time samples, camera/lights and non-mesh geometry are
//! not imported. Only composed default (non-time-sampled) values are used; a
//! time-varying `points` or transform is imported at its default value and
//! reported. A subdivision cage is imported as the control mesh. The reader
//! does not repair initial intersections, open surfaces or inconsistent
//! winding, and it is not confined to the asset directory: composition follows
//! the layer's own references and payloads. Each document and expanded mesh is
//! limited by [`Limits`].
//!
//! Validation matches the other avatar importers: non-finite coordinates and
//! out-of-range indices are errors, while zero-area triangles produced by the
//! triangulation are dropped and counted in the warnings so a dense avatar is
//! not rejected for one malformed face.

use crate::{avatar::SurfaceMesh, gltf_avatar::Limits};
use glam::{DMat4, DQuat, DVec3};
use openusd::{
    gf,
    sdf::{self, Value},
    tf::Token,
    usd::{Prim, Stage},
};
use std::path::Path;

/// USD's fallback stage linear unit: one centimetre per unit.
///
/// The fallback is part of USD's stage metadata contract, not a ClothCraft
/// guess; a layer that means metres must author `metersPerUnit = 1`.
const USD_DEFAULT_METERS_PER_UNIT: f64 = 0.01;
/// Composed-stage metadata keys honored by this reader.
const METERS_PER_UNIT: &str = "metersPerUnit";
const UP_AXIS: &str = "upAxis";
/// `xformOpOrder` token that makes a prim ignore every ancestor transform.
const RESET_XFORM_STACK: &str = "!resetXformStack!";
/// Prefix marking an inverted entry in `xformOpOrder`.
const INVERTED_OP_PREFIX: &str = "!invert!";

/// Geometry and loss report for one static USD stage.
#[derive(Debug)]
pub struct ImportedAvatar {
    /// Every visible mesh merged; the avatar importer selects which prims collide.
    pub mesh: SurfaceMesh,
    /// Named per-prim meshes with the composed transforms applied, in traversal order.
    pub parts: Vec<(String, SurfaceMesh)>,
    pub warnings: Vec<String>,
}

/// Read `path` as a composed USD stage and flatten its static meshes.
pub fn load(path: &Path, limits: Limits) -> Result<ImportedAvatar, String> {
    let resolved = std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot read avatar {}: {e}", path.display()))?;
    let bytes = std::fs::metadata(&resolved)
        .map_err(|e| format!("Cannot read avatar {}: {e}", path.display()))?
        .len();
    if bytes > limits.document_bytes as u64 {
        return Err("Avatar document exceeds the byte limit".into());
    }
    let identifier = resolved
        .to_str()
        .ok_or("USD layer paths must be valid UTF-8")?;
    let stage = Stage::open(identifier)
        .map_err(|e| format!("Cannot open USD stage {}: {e}", path.display()))?;
    import(&stage, limits)
}

/// Flatten an already-open stage. Split from [`load`] for tests and callers
/// that compose the stage themselves.
pub fn import(stage: &Stage, limits: Limits) -> Result<ImportedAvatar, String> {
    let mut warnings = Vec::new();
    let meters_per_unit = match stage
        .stage_metadata(METERS_PER_UNIT)
        .map_err(|e| format!("Cannot read stage {METERS_PER_UNIT}: {e}"))?
    {
        Some(Value::Double(v)) => Some(v),
        Some(Value::Float(v)) => Some(f64::from(v)),
        Some(Value::Int(v)) => Some(f64::from(v)),
        Some(_) => return Err(format!("Stage {METERS_PER_UNIT} must be a number")),
        None => None,
    };
    let meters_per_unit = meters_per_unit.unwrap_or_else(|| {
        warnings.push(format!(
            "The stage authors no {METERS_PER_UNIT}; USD's {USD_DEFAULT_METERS_PER_UNIT} m per unit (centimetres) is applied. Author metersPerUnit = 1 for metre coordinates."
        ));
        USD_DEFAULT_METERS_PER_UNIT
    });
    if !(meters_per_unit.is_finite() && meters_per_unit > 0.0) {
        return Err(format!("Stage {METERS_PER_UNIT} must be positive"));
    }
    let up_axis = match stage
        .stage_metadata(UP_AXIS)
        .map_err(|e| format!("Cannot read stage {UP_AXIS}: {e}"))?
    {
        Some(Value::Token(token)) => token.as_str().to_owned(),
        Some(_) => return Err(format!("Stage {UP_AXIS} must be a token")),
        None => "Y".into(),
    };
    let to_y_up = match up_axis.as_str() {
        "Y" => DMat4::IDENTITY,
        "Z" => {
            warnings.push(
                "The stage is Z-up; geometry is rotated to the editor's Y-up frame about X.".into(),
            );
            DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2)
        }
        other => return Err(format!("Unsupported USD stage {UP_AXIS} {other:?}")),
    };

    let mut collector = Collector {
        limits,
        warnings,
        to_y_up,
        scale: meters_per_unit,
        parts: Vec::new(),
        dropped_triangles: 0,
        skipped: Skipped::default(),
    };
    let root = stage
        .prim(sdf::Path::abs_root())
        .map_err(|e| format!("Cannot read the USD root: {e}"))?;
    let children = root
        .children()
        .map_err(|e| format!("Cannot read the USD root children: {e}"))?;
    for child in children {
        collector.visit(&child, DMat4::IDENTITY, 0)?;
    }
    collector.finish()
}

/// Non-mesh content counted for the report instead of imported.
#[derive(Default)]
struct Skipped {
    gprims: usize,
    invisible: Vec<String>,
    non_default_purpose: usize,
}

struct Collector {
    limits: Limits,
    warnings: Vec<String>,
    to_y_up: DMat4,
    scale: f64,
    parts: Vec<(String, SurfaceMesh)>,
    dropped_triangles: usize,
    skipped: Skipped,
}

impl Collector {
    fn visit(&mut self, prim: &Prim, parent: DMat4, depth: usize) -> Result<(), String> {
        if depth > self.limits.depth {
            return Err("USD prim hierarchy exceeds the depth limit".into());
        }
        let world = self.world_transform(prim, parent)?;
        let path = prim.path().to_string();
        let type_name = prim
            .type_name()
            .map_err(|e| format!("Cannot read the type of {path}: {e}"))?
            .map(|token| token.as_str().to_owned());
        match type_name.as_deref() {
            Some("Mesh") => {
                if self.invisible(prim)? {
                    self.skipped.invisible.push(path.clone());
                } else {
                    if let Ok(Some(token)) = prim.attribute("purpose").get::<Token>()
                        && token.as_str() != "default"
                    {
                        self.skipped.non_default_purpose += 1;
                    }
                    self.mesh(prim, world)?;
                }
            }
            Some(
                "Sphere" | "Cube" | "Cylinder" | "Capsule" | "Cone" | "Plane" | "Points"
                | "BasisCurves" | "NurbsCurves" | "PointInstancer",
            ) => {
                self.skipped.gprims += 1;
            }
            _ => {}
        }
        for child in prim
            .children()
            .map_err(|e| format!("Cannot read the children of {path}: {e}"))?
        {
            self.visit(&child, world, depth + 1)?;
        }
        Ok(())
    }

    /// Visibility is inherited, so any invisible ancestor hides the subtree.
    fn invisible(&self, prim: &Prim) -> Result<bool, String> {
        let mut current = Some(prim.clone());
        while let Some(prim) = current {
            if let Some(Value::Token(token)) = prim
                .attribute("visibility")
                .get::<Value>()
                .map_err(|e| format!("Cannot read visibility of {}: {e}", prim.path()))?
                && token.as_str() == "invisible"
            {
                return Ok(true);
            }
            current = prim.parent();
        }
        Ok(false)
    }

    fn world_transform(&mut self, prim: &Prim, parent: DMat4) -> Result<DMat4, String> {
        let path = prim.path().to_string();
        let order = prim
            .attribute("xformOpOrder")
            .get::<Vec<Token>>()
            .map_err(|e| format!("Cannot read xformOpOrder of {path}: {e}"))?
            .unwrap_or_default();
        let resets = order
            .iter()
            .any(|token| token.as_str() == RESET_XFORM_STACK);
        let mut local = DMat4::IDENTITY;
        for token in &order {
            let name = token.as_str();
            if name == RESET_XFORM_STACK {
                continue;
            }
            let (inverted, name) = match name.strip_prefix(INVERTED_OP_PREFIX) {
                Some(name) => (true, name),
                None => (false, name),
            };
            let Some(kind) = xform_op_kind(name) else {
                self.warnings.push(format!(
                    "Unsupported transform op {name:?} on {path} was ignored; the mesh may be misplaced."
                ));
                continue;
            };
            let Some(value) = prim
                .attribute(name)
                .get::<Value>()
                .map_err(|e| format!("Cannot read {name} of {path}: {e}"))?
            else {
                continue;
            };
            let matrix = op_matrix(kind, value, inverted)
                .ok_or_else(|| format!("Unsupported {name} value on {path}"))?;
            // Row-vector USD composes op1 * op2 * ... * opN for the listed
            // order, so the column-vector equivalent prepends each op.
            local = matrix * local;
        }
        let world = if resets { local } else { parent * local };
        if !world.is_finite() {
            return Err(format!("The transform of {path} is not finite"));
        }
        Ok(world)
    }

    fn mesh(&mut self, prim: &Prim, world: DMat4) -> Result<(), String> {
        let path = prim.path().to_string();
        let points = points(prim).map_err(|e| format!("Cannot read points of {path}: {e}"))?;
        let Some(points) = points else {
            self.warnings.push(format!(
                "Mesh {path} has no readable points attribute and was skipped."
            ));
            return Ok(());
        };
        let counts = prim
            .attribute("faceVertexCounts")
            .get::<Vec<i32>>()
            .map_err(|e| format!("Cannot read faceVertexCounts of {path}: {e}"))?
            .unwrap_or_default();
        let indices = prim
            .attribute("faceVertexIndices")
            .get::<Vec<i32>>()
            .map_err(|e| format!("Cannot read faceVertexIndices of {path}: {e}"))?
            .unwrap_or_default();
        if counts.is_empty() || indices.is_empty() {
            self.warnings.push(format!(
                "Mesh {path} has no triangle topology and was skipped."
            ));
            return Ok(());
        }
        let total: i64 = counts.iter().map(|&n| i64::from(n)).sum();
        if total != indices.len() as i64 {
            return Err(format!(
                "Mesh {path} declares {total} face vertex slots but has {} indices",
                indices.len()
            ));
        }
        let left_handed = matches!(
            prim.attribute("orientation").get::<Token>(),
            Ok(Some(token)) if token.as_str() == "leftHanded"
        );
        if left_handed {
            self.warnings.push(format!(
                "Mesh {path} is left-handed; triangle winding is reversed to keep outward normals."
            ));
        }
        // USD's fallback scheme is catmullClark, so an unauthored attribute also
        // means the renderable surface is subdivided while the cage is imported.
        let subdivision = prim
            .attribute("subdivisionScheme")
            .get::<Token>()
            .ok()
            .flatten()
            .map(|token| token.as_str().to_owned())
            .unwrap_or_else(|| "catmullClark".into());
        if subdivision != "none" {
            self.warnings.push(format!(
                "Mesh {path} uses subdivision scheme {subdivision:?} (catmullClark is USD's fallback when unauthored); the imported geometry is the control cage, not the subdivided surface."
            ));
        }
        if prim
            .attribute("points")
            .value_might_be_time_varying()
            .unwrap_or(false)
        {
            self.warnings.push(format!(
                "Mesh {path} has time-sampled points; only the default sample is imported."
            ));
        }

        if points.len() > self.limits.vertices {
            return Err("Avatar geometry exceeds the import limit".into());
        }
        let mut vertices = Vec::with_capacity(points.len());
        for point in &points {
            let p = world.transform_point3(*point);
            let p = self.to_y_up.transform_point3(p * self.scale);
            if !p.is_finite() {
                return Err(format!("Mesh {path} has non-finite transformed points"));
            }
            vertices.push(p);
        }
        let mut cursor = 0usize;
        let mut local_triangles = Vec::new();
        for &count in &counts {
            if count < 3 {
                self.warnings.push(format!(
                    "Mesh {path} has a face with {count} vertices; it cannot be triangulated and was skipped."
                ));
                cursor += count.max(0) as usize;
                continue;
            }
            let face = &indices[cursor..cursor + count as usize];
            cursor += count as usize;
            let mut ring = Vec::with_capacity(face.len());
            for &raw in face {
                let index = usize::try_from(raw)
                    .ok()
                    .filter(|&i| i < points.len())
                    .ok_or_else(|| format!("Mesh {path} has an invalid face index {raw}"))?;
                ring.push(index);
            }
            for k in 1..ring.len() - 1 {
                let mut triangle = [ring[0], ring[k], ring[k + 1]];
                if left_handed {
                    triangle.swap(1, 2);
                }
                local_triangles.push(triangle);
            }
        }
        if local_triangles.len() > self.limits.triangles {
            return Err("Avatar geometry exceeds the import limit".into());
        }
        let mut triangles = Vec::with_capacity(local_triangles.len());
        for triangle in local_triangles {
            let [a, b, c] = triangle.map(|i| vertices[i]);
            if (b - a).cross(c - a).length_squared() < 1e-24 {
                self.dropped_triangles += 1;
            } else {
                triangles.push(triangle);
            }
        }
        if triangles.is_empty() {
            self.warnings.push(format!(
                "Mesh {path} has no non-degenerate triangles and was skipped."
            ));
            return Ok(());
        }
        self.parts.push((
            path.trim_start_matches('/').to_owned(),
            SurfaceMesh {
                vertices,
                triangles,
            },
        ));
        Ok(())
    }

    fn finish(mut self) -> Result<ImportedAvatar, String> {
        let mut warnings = std::mem::take(&mut self.warnings);
        if self.dropped_triangles > 0 {
            warnings.push(format!(
                "{} zero-area triangle(s) produced by triangulation were dropped; the rest of the mesh is unchanged.",
                self.dropped_triangles
            ));
        }
        if self.skipped.gprims > 0 {
            warnings.push(format!(
                "{} non-mesh geometry prim(s) were skipped; only UsdGeom Mesh is imported.",
                self.skipped.gprims
            ));
        }
        if self.skipped.non_default_purpose > 0 {
            warnings.push(format!(
                "{} visible mesh prim(s) author a non-default purpose; every visible mesh is imported, so proxy, render or guide helper geometry may be included.",
                self.skipped.non_default_purpose
            ));
        }
        if !self.skipped.invisible.is_empty() {
            let mut listed = self
                .skipped
                .invisible
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if self.skipped.invisible.len() > 8 {
                listed.push_str(", ...");
            }
            warnings.push(format!(
                "{} invisible mesh prim(s) were skipped: {listed}",
                self.skipped.invisible.len()
            ));
        }
        if self.parts.is_empty() {
            return Err("The USD stage contains no visible triangle mesh".into());
        }
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for (_, part) in &self.parts {
            let base = vertices.len();
            vertices.extend_from_slice(&part.vertices);
            triangles.extend(
                part.triangles
                    .iter()
                    .map(|triangle| triangle.map(|index| base + index)),
            );
        }
        let mesh = SurfaceMesh {
            vertices,
            triangles,
        };
        mesh.validate()?;
        warnings.push(
            "USD import keeps static surface positions only: animation, skinning, UVs, materials, textures, normals, instancing and camera/light prims are not retained."
                .into(),
        );
        Ok(ImportedAvatar {
            mesh,
            parts: self.parts,
            warnings,
        })
    }
}

/// The transform op kind named by the second namespace component, together
/// with its axis order for Euler rotations. Mirrors the USD `xformOp` names.
#[derive(Clone, Copy)]
enum OpKind {
    Transform,
    Translate,
    TranslateAxis(usize),
    Scale,
    ScaleAxis(usize),
    RotateAxis(usize),
    RotateEuler([usize; 3]),
    Orient,
}

fn xform_op_kind(name: &str) -> Option<OpKind> {
    Some(match name.split(':').nth(1)? {
        "transform" => OpKind::Transform,
        "translate" => OpKind::Translate,
        "translateX" => OpKind::TranslateAxis(0),
        "translateY" => OpKind::TranslateAxis(1),
        "translateZ" => OpKind::TranslateAxis(2),
        "scale" => OpKind::Scale,
        "scaleX" => OpKind::ScaleAxis(0),
        "scaleY" => OpKind::ScaleAxis(1),
        "scaleZ" => OpKind::ScaleAxis(2),
        "rotateX" => OpKind::RotateAxis(0),
        "rotateY" => OpKind::RotateAxis(1),
        "rotateZ" => OpKind::RotateAxis(2),
        "rotateXYZ" => OpKind::RotateEuler([0, 1, 2]),
        "rotateXZY" => OpKind::RotateEuler([0, 2, 1]),
        "rotateYXZ" => OpKind::RotateEuler([1, 0, 2]),
        "rotateYZX" => OpKind::RotateEuler([1, 2, 0]),
        "rotateZXY" => OpKind::RotateEuler([2, 0, 1]),
        "rotateZYX" => OpKind::RotateEuler([2, 1, 0]),
        "orient" => OpKind::Orient,
        _ => return None,
    })
}

/// The column-vector matrix of one `xformOp`, or `None` for a value kind it
/// cannot hold. USD authors row-vector matrices, so `transform` is transposed.
fn op_matrix(kind: OpKind, value: Value, inverted: bool) -> Option<DMat4> {
    let sign = if inverted { -1.0 } else { 1.0 };
    Some(match kind {
        OpKind::Transform => {
            let Value::Matrix4d(matrix) = value else {
                return None;
            };
            let matrix = DMat4::from_cols_array(&matrix.0);
            if inverted {
                let determinant = matrix.determinant();
                if !(determinant.is_finite() && determinant.abs() > 1e-18) {
                    return None;
                }
                matrix.inverse()
            } else {
                matrix
            }
        }
        OpKind::Translate => {
            let [x, y, z]: [f64; 3] = value.cast().ok()?;
            DMat4::from_translation(DVec3::new(sign * x, sign * y, sign * z))
        }
        OpKind::TranslateAxis(axis) => {
            let amount: f64 = value.cast().ok()?;
            let mut translation = DVec3::ZERO;
            translation[axis] = sign * amount;
            DMat4::from_translation(translation)
        }
        OpKind::Scale => {
            let [x, y, z]: [f64; 3] = value.cast().ok()?;
            let factor = |c: f64| if inverted { 1.0 / c } else { c };
            DMat4::from_scale(DVec3::new(factor(x), factor(y), factor(z)))
        }
        OpKind::ScaleAxis(axis) => {
            let amount: f64 = value.cast().ok()?;
            let mut scale = DVec3::ONE;
            scale[axis] = if inverted { 1.0 / amount } else { amount };
            DMat4::from_scale(scale)
        }
        OpKind::RotateAxis(axis) => {
            let degrees: f64 = value.cast().ok()?;
            rotation(axis, sign * degrees)
        }
        OpKind::RotateEuler(axes) => {
            let degrees: [f64; 3] = value.cast().ok()?;
            // USD's row-vector Euler matrix is Rx * Ry * Rz; the column-vector
            // equivalent prepends each axis rotation in the spelled order.
            let mut matrix = DMat4::IDENTITY;
            for (index, &axis) in axes.iter().enumerate() {
                matrix = rotation(axis, sign * degrees[index]) * matrix;
            }
            matrix
        }
        OpKind::Orient => {
            let [w, x, y, z]: [f64; 4] = value.cast().ok()?;
            let quat = DQuat::from_xyzw(x, y, z, w).normalize();
            let quat = if inverted { quat.conjugate() } else { quat };
            DMat4::from_quat(quat)
        }
    })
}

fn rotation(axis: usize, degrees: f64) -> DMat4 {
    let radians = degrees.to_radians();
    match axis {
        0 => DMat4::from_rotation_x(radians),
        1 => DMat4::from_rotation_y(radians),
        _ => DMat4::from_rotation_z(radians),
    }
}

/// `points` may be authored as `point3f[]`, `point3d[]`, `float3[]` or
/// `double3[]`; all four decode to a Vec3 array the reader accepts.
fn points(prim: &Prim) -> Result<Option<Vec<DVec3>>, String> {
    let attribute = prim.attribute("points");
    if let Some(points) = attribute
        .get::<Vec<gf::Vec3f>>()
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(
            points
                .into_iter()
                .map(|p| DVec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z)))
                .collect(),
        ));
    }
    if let Some(points) = attribute
        .get::<Vec<gf::Vec3d>>()
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(
            points
                .into_iter()
                .map(|p| DVec3::new(p.x, p.y, p.z))
                .collect(),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_op_name_resolves() {
        for name in [
            "xformOp:transform",
            "xformOp:translate",
            "xformOp:translateX",
            "xformOp:scale",
            "xformOp:scaleZ",
            "xformOp:rotateX",
            "xformOp:rotateXYZ",
            "xformOp:rotateZYX",
            "xformOp:orient",
            "xformOp:translate:pivot",
        ] {
            assert!(xform_op_kind(name).is_some(), "{name}");
        }
        assert!(xform_op_kind("xformOp:bogus").is_none());
        assert!(xform_op_kind("notAnOp").is_none());
    }

    #[test]
    fn transform_matrix_is_transposed_and_inverted() {
        // Row-vector USD translation: the offset lives in the fourth row.
        let mut matrix = gf::Matrix4d::IDENTITY;
        matrix.0[12] = 2.0;
        matrix.0[13] = 3.0;
        matrix.0[14] = 4.0;
        let forward = op_matrix(OpKind::Transform, Value::Matrix4d(matrix), false).unwrap();
        assert_eq!(
            forward.transform_point3(DVec3::ZERO),
            DVec3::new(2., 3., 4.)
        );
        let inverse = op_matrix(OpKind::Transform, Value::Matrix4d(matrix), true).unwrap();
        assert_eq!(
            inverse.transform_point3(DVec3::new(2., 3., 4.)),
            DVec3::ZERO
        );
    }

    #[test]
    fn euler_order_applies_x_then_y_then_z() {
        let rotate = |x: f64, y: f64| {
            op_matrix(
                OpKind::RotateEuler([0, 1, 2]),
                Value::Vec3d(gf::Vec3d { x, y, z: 0.0 }),
                false,
            )
            .unwrap()
        };
        // +90 about X sends +Y to +Z.
        let point = rotate(90.0, 0.0).transform_point3(DVec3::Y);
        assert!((point - DVec3::Z).length() < 1e-12, "{point}");
        // X then Y: +Z -> -Y by X, then -Y is unchanged by Y. The reverse
        // order (Y then X) would give +X instead.
        let point = rotate(90.0, 90.0).transform_point3(DVec3::Z);
        assert!((point + DVec3::Y).length() < 1e-12, "{point}");
    }
}
