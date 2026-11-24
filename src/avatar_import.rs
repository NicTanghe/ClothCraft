//! CPU preparation for replacing a static avatar without changing project v1.
use crate::{
    avatar::SurfaceMesh,
    fbx_avatar,
    fitting::{FittingProject, FittingScene},
    garment::CompiledGarment,
    gltf_avatar, usd_avatar,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use std::{
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

/// Keep at least this much clearance before reporting a panel as intersecting
/// the avatar. The solver rejects true intersections, not sub-millimetre
/// numerical contact.
const ARRANGEMENT_CONFLICT_M: f64 = 1e-4;
/// Rigid outward step applied to a panel that starts inside the new avatar.
const SEPARATION_STEP_M: f64 = 0.01;
/// Bound on automatic separation so a pathological arrangement cannot loop.
const SEPARATION_MAX_STEPS: usize = 8;

/// Keeps the canonical OBJ alive while the editable project refers to it.
/// Portable project saves copy it into their own content-addressed asset folder.
#[derive(Clone, Debug)]
pub struct StagedAvatar(Arc<tempfile::TempDir>);

impl StagedAvatar {
    fn path(&self) -> std::path::PathBuf {
        self.0.path().join("avatar.obj")
    }
}

/// One importable source surface. Multi-mesh documents (for example Mixamo FBX
/// files with `Alpha_Surface` and `Alpha_Joints`) expose every mesh so the
/// reviewer can keep helper geometry out of the collider.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshEntry {
    pub name: String,
    pub vertices: usize,
    pub triangles: usize,
    /// Total triangle area in square metres; the default collider choice.
    pub area_m2: f64,
    pub selected: bool,
}

#[derive(Debug)]
pub struct Candidate {
    pub source: std::path::PathBuf,
    pub vertices: usize,
    pub triangles: usize,
    pub bounds: [DVec3; 2],
    pub warnings: Vec<String>,
    pub meshes: Vec<MeshEntry>,
    parts: Vec<SurfaceMesh>,
    /// Every source mesh for the viewport, including meshes not used as colliders.
    display: Arc<SurfaceMesh>,
    base_warnings: Vec<String>,
    asset: StagedAvatar,
}

impl Candidate {
    /// Geometry only. No implicit scene selection, scale normalization or source writes.
    pub fn load(path: &Path) -> Result<Self, String> {
        let limits = gltf_avatar::Limits::default();
        let extension = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let fallback_name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("avatar")
            .to_owned();
        let (parts, mut warnings) = match extension.as_str() {
            "gltf" | "glb" => {
                let imported = gltf_avatar::load(path, None, limits)?;
                (imported.parts, imported.warnings)
            }
            "fbx" => {
                let imported = fbx_avatar::load(path, limits)?;
                (imported.parts, imported.warnings)
            }
            "usd" | "usda" | "usdc" | "usdz" => {
                let imported = usd_avatar::load(path, limits)?;
                (imported.parts, imported.warnings)
            }
            "obj" => {
                let mut bytes = Vec::new();
                std::fs::File::open(path)
                    .map_err(|e| e.to_string())?
                    .take(limits.document_bytes as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > limits.document_bytes {
                    return Err("Avatar document exceeds the byte limit".into());
                }
                let text =
                    std::str::from_utf8(&bytes).map_err(|e| format!("OBJ must be UTF-8: {e}"))?;
                let mesh = SurfaceMesh::parse_obj(text)?;
                if mesh.vertices.len() > limits.vertices || mesh.triangles.len() > limits.triangles
                {
                    return Err("Avatar geometry exceeds the import limit".into());
                }
                (vec![(fallback_name, mesh)], vec!["OBJ has no declared units: coordinates are interpreted as metres, Y-up. No rescaling is applied.".into(),
                    "Geometry only: materials, textures, UVs, authored normals and object groups are not retained. Polygon faces use the existing fan triangulation; use triangulated OBJ for unambiguous geometry.".into()])
            }
            _ => return Err("Choose a static OBJ, glTF/GLB, FBX or USD avatar".into()),
        };
        if parts.is_empty() {
            return Err("The avatar contains no usable mesh".into());
        }
        let default_selection = default_selection(&parts);
        let asset = StagedAvatar(Arc::new(tempfile::tempdir().map_err(|e| e.to_string())?));
        let meshes = parts
            .iter()
            .zip(&default_selection)
            .map(|((name, mesh), selected)| MeshEntry {
                name: name.clone(),
                vertices: mesh.vertices.len(),
                triangles: mesh.triangles.len(),
                area_m2: surface_area(mesh),
                selected: *selected,
            })
            .collect();
        let mut candidate = Self {
            source: path.to_owned(),
            vertices: 0,
            triangles: 0,
            bounds: [DVec3::ZERO, DVec3::ZERO],
            warnings: Vec::new(),
            meshes,
            parts: parts.into_iter().map(|(_, mesh)| mesh).collect(),
            display: Arc::new(SurfaceMesh {
                vertices: vec![],
                triangles: vec![],
            }),
            base_warnings: std::mem::take(&mut warnings),
            asset,
        };
        candidate.rebuild()?;
        Ok(candidate)
    }

    /// Select or deselect one source mesh and restage the canonical OBJ. At
    /// least one mesh must remain selected.
    pub fn toggle_mesh(&mut self, index: usize) -> Result<(), String> {
        let Some(entry) = self.meshes.get(index) else {
            return Err("Unknown avatar mesh".into());
        };
        if entry.selected && self.meshes.iter().filter(|mesh| mesh.selected).count() == 1 {
            return Err("Keep at least one avatar mesh selected as the collider".into());
        }
        self.meshes[index].selected = !self.meshes[index].selected;
        self.rebuild()
    }

    /// Rebuild the merged collider, bounds, warnings and staged OBJ from the
    /// current selection.
    fn rebuild(&mut self) -> Result<(), String> {
        let limits = gltf_avatar::Limits::default();
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let mut merged_vertices = 0usize;
        let mut dropped_triangles = 0usize;
        for (entry, part) in self.meshes.iter().zip(&self.parts) {
            if !entry.selected {
                continue;
            }
            // glTF/FBX exporters often split vertices at normal/UV seams. The
            // surface is watertight in space but topologically open, which
            // breaks penetration signs and contact coverage; weld exact
            // duplicates inside each part before merging.
            let (part, merged, dropped) = weld_coincident_vertices(part.clone());
            merged_vertices += merged;
            dropped_triangles += dropped;
            let base = vertices.len();
            vertices.extend_from_slice(&part.vertices);
            triangles.extend(
                part.triangles
                    .iter()
                    .map(|triangle| triangle.map(|index| base + index)),
            );
        }
        if vertices.is_empty() || triangles.is_empty() {
            return Err("Keep at least one non-empty avatar mesh selected".into());
        }
        if vertices.len() > limits.vertices || triangles.len() > limits.triangles {
            return Err("Avatar geometry exceeds the import limit".into());
        }
        let mut mesh = SurfaceMesh {
            vertices,
            triangles,
        };
        mesh.validate()?;
        let mut bounds = [DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)];
        for &p in &mesh.vertices {
            bounds[0] = bounds[0].min(p);
            bounds[1] = bounds[1].max(p);
        }
        // Avatars authored with geometry below the floor (for example a
        // dressform whose stand extends below its origin) otherwise sit half
        // underground and the garment floats above them. Use every source part,
        // not only the selected collider, so deselecting the stand keeps the
        // form at its standing height.
        let asset_floor = self
            .parts
            .iter()
            .flat_map(|part| part.vertices.iter().map(|point| point.y))
            .fold(f64::INFINITY, f64::min);
        let mut ground_shift = 0.0;
        if asset_floor.is_finite() && asset_floor < -1e-6 {
            ground_shift = -asset_floor;
            for point in &mut mesh.vertices {
                point.y += ground_shift;
            }
            bounds[0].y += ground_shift;
            bounds[1].y += ground_shift;
        }
        if !(bounds[1] - bounds[0]).is_finite()
            || mesh.triangles.iter().any(|triangle| {
                let [a, b, c] = triangle.map(|i| mesh.vertices[i]);
                !(b - a).cross(c - a).length_squared().is_finite()
            })
        {
            return Err("Avatar dimensions overflow geometry calculations".into());
        }
        self.write_staged(&mesh)?;
        self.vertices = mesh.vertices.len();
        self.triangles = mesh.triangles.len();
        self.bounds = bounds;
        self.warnings = self.base_warnings.clone();
        if self.meshes.len() > 1 {
            let selected: Vec<_> = self
                .meshes
                .iter()
                .filter(|mesh| mesh.selected)
                .map(|mesh| mesh.name.clone())
                .collect();
            let skipped: Vec<_> = self
                .meshes
                .iter()
                .filter(|mesh| !mesh.selected)
                .map(|mesh| mesh.name.clone())
                .collect();
            let mut summary = format!(
                "{} meshes in the source; collider uses {}",
                self.meshes.len(),
                selected.join(", ")
            );
            if !skipped.is_empty() {
                summary.push_str(&format!("; not selected: {}", skipped.join(", ")));
            }
            summary.push_str(
                ". The mesh with the largest surface area is selected by default; enable every surface that should collide and leave helper meshes (for example stand or joint-marker meshes) off.",
            );
            self.warnings.push(summary);
        }
        if merged_vertices > 0 {
            self.warnings.push(format!(
                "Merged {merged_vertices} coincident vertices so the collider uses connected surface topology."
            ));
        }
        if dropped_triangles > 0 {
            self.warnings.push(format!(
                "Dropped {dropped_triangles} triangles that became degenerate while welding coincident vertices."
            ));
        }
        let mut display_vertices = Vec::new();
        let mut display_triangles = Vec::new();
        for part in &self.parts {
            let base = display_vertices.len();
            display_vertices.extend_from_slice(&part.vertices);
            display_triangles.extend(
                part.triangles
                    .iter()
                    .map(|triangle| triangle.map(|index| base + index)),
            );
        }
        if ground_shift != 0.0 {
            for point in &mut display_vertices {
                point.y += ground_shift;
            }
        }
        self.display = Arc::new(SurfaceMesh {
            vertices: display_vertices,
            triangles: display_triangles,
        });
        if ground_shift > 1e-6 {
            self.warnings.push(format!(
                "The avatar extended {:.1} mm below the floor and was raised so its lowest point sits at Y = 0. The authored vertical position is not preserved.",
                ground_shift * 1000.0
            ));
        }
        self.warnings.push(
            "Import does not repair intersections, open surfaces or inconsistent winding. Check placement before simulating."
                .into(),
        );
        Ok(())
    }

    fn write_staged(&self, mesh: &SurfaceMesh) -> Result<(), String> {
        let mut out = std::io::BufWriter::new(
            std::fs::File::create(self.asset.path()).map_err(|e| e.to_string())?,
        );
        writeln!(out, "# ClothCraft static avatar; metres; Y-up").map_err(|e| e.to_string())?;
        for p in &mesh.vertices {
            writeln!(out, "v {} {} {}", p.x, p.y, p.z).map_err(|e| e.to_string())?;
        }
        for [a, b, c] in &mesh.triangles {
            writeln!(out, "f {} {} {}", a + 1, b + 1, c + 1).map_err(|e| e.to_string())?;
        }
        out.flush().map_err(|e| e.to_string())
    }

    /// Validate the whole candidate before any active workspace field changes.
    /// Authored rest geometry, placement and seams are copied verbatim.
    pub fn prepare(
        &self,
        current: &FittingProject,
        settings: VbdSettings,
    ) -> Result<Prepared, String> {
        let mut project = current.clone();
        project.avatar_path = self.asset.path();
        project.state = None;
        project.mesh_signature = None;
        project.prearrange = false;
        project.simulation = Some(settings);
        let mut scene = project.compile()?;
        let warnings = separate_initial_arrangement(&mut project, &mut scene);
        Ok(Prepared {
            project,
            scene,
            asset: self.asset.clone(),
            display: Arc::clone(&self.display),
            warnings,
        })
    }
}

/// The collider defaults to the mesh with the largest surface area, which is the
/// body in
/// common multi-mesh exports (Mixamo's body plus joint markers). Every mesh is
/// listed in the review modal and can be selected explicitly.
/// Merge vertices with identical positions and remap triangles. Returns the
/// welded mesh, how many vertices were merged and how many triangles collapsed.
fn weld_coincident_vertices(mesh: SurfaceMesh) -> (SurfaceMesh, usize, usize) {
    use std::collections::HashMap;
    let canonical = |value: f64| if value == 0.0 { 0.0 } else { value };
    let mut lookup: HashMap<[u64; 3], usize> = HashMap::new();
    let mut vertices = Vec::with_capacity(mesh.vertices.len());
    let mut remap = Vec::with_capacity(mesh.vertices.len());
    for point in &mesh.vertices {
        let key = [
            canonical(point.x).to_bits(),
            canonical(point.y).to_bits(),
            canonical(point.z).to_bits(),
        ];
        match lookup.get(&key) {
            Some(&index) => remap.push(index),
            None => {
                let index = vertices.len();
                lookup.insert(key, index);
                vertices.push(*point);
                remap.push(index);
            }
        }
    }
    let merged = mesh.vertices.len().saturating_sub(vertices.len());
    let mut triangles = Vec::with_capacity(mesh.triangles.len());
    let mut dropped = 0usize;
    for triangle in &mesh.triangles {
        let triangle = triangle.map(|index| remap[index]);
        if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[0] == triangle[2] {
            dropped += 1;
        } else {
            triangles.push(triangle);
        }
    }
    // OGC requires manifold edges. Touching but separately-authored pieces can
    // weld into a three-way junction; keep the original split surface for that
    // part instead of producing a collider the solver rejects.
    let mut edges: HashMap<(usize, usize), u8> = HashMap::new();
    let mut manifold = true;
    'faces: for triangle in &triangles {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = (a.min(b), a.max(b));
            let count = edges.entry(key).or_insert(0);
            *count += 1;
            if *count > 2 {
                manifold = false;
                break 'faces;
            }
        }
    }
    if !manifold {
        return (mesh, 0, 0);
    }
    (
        SurfaceMesh {
            vertices,
            triangles,
        },
        merged,
        dropped,
    )
}

fn surface_area(mesh: &SurfaceMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|triangle| {
            let [a, b, c] = triangle.map(|index| mesh.vertices[index]);
            (b - a).cross(c - a).length() * 0.5
        })
        .sum()
}

fn default_selection(parts: &[(String, SurfaceMesh)]) -> Vec<bool> {
    if parts.len() <= 1 {
        return vec![true; parts.len()];
    }
    let largest = parts
        .iter()
        .enumerate()
        .max_by(|a, b| surface_area(&a.1.1).total_cmp(&surface_area(&b.1.1)))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let mut selected = vec![false; parts.len()];
    selected[largest] = true;
    selected
}

/// Make the authored arrangement clear the new collider before installation.
///
/// The solver rejects any initial intersection, including a panel hidden
/// behind the avatar (the common case after replacing the mannequin with a
/// differently shaped body). Compile once, ask the same initial-state check the
/// worker uses, map the reported garment face back to its panel and rigidly
/// move that panel outward in 1 cm steps. Report every move so the reviewer
/// knows the authored placement changed; leave the arrangement untouched when
/// authored pins/frozen panels make a silent move unsafe.
fn separate_initial_arrangement(
    project: &mut FittingProject,
    scene: &mut FittingScene,
) -> Vec<String> {
    if !project.template.pin_targets.is_empty() || !project.template.frozen_panels.is_empty() {
        return arrangement_warnings(scene);
    }
    let avatar = &scene.collider.surface;
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for &p in &avatar.vertices {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let avatar_centre = (lo + hi) * 0.5;
    let mut moved: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut unresolved = None;
    let mut solved = false;
    for _ in 0..SEPARATION_MAX_STEPS {
        let result = VbdSolver::new(
            scene.garment.clone(),
            Some(&scene.collider.surface),
            &scene.state,
        );
        let error = match result {
            Ok(_) => {
                solved = true;
                unresolved = None;
                break;
            }
            Err(error) => error,
        };
        let Some((first_face, second_face)) = error_face_pair(&error) else {
            unresolved = Some(error);
            break;
        };
        let panels = panels_for_faces(first_face, second_face, &scene.garment);
        if panels.is_empty() {
            unresolved = Some(error);
            break;
        }
        let cloth = scene.garment.mesh.triangles.len();
        if first_face < cloth
            && second_face < cloth
            && panel_of_face(first_face, &scene.garment)
                == panel_of_face(second_face, &scene.garment)
        {
            // A rigid panel move cannot separate a panel folded into itself.
            unresolved = Some(error);
            break;
        }
        for id in panels {
            let Some(panel) = scene.garment.panels.iter().find(|panel| panel.id == id) else {
                continue;
            };
            let range = panel.vertices.clone();
            if range.is_empty() {
                continue;
            }
            let centre = range
                .clone()
                .map(|index| scene.state.x[index])
                .sum::<DVec3>()
                / range.len() as f64;
            let sign = if centre.z >= avatar_centre.z {
                1.0
            } else {
                -1.0
            };
            let delta = DVec3::new(0.0, 0.0, sign * SEPARATION_STEP_M);
            let Some(index) = project.template.panels.iter().position(|p| p.id == id) else {
                continue;
            };
            project.template.panels[index].placement.translation += delta;
            for vertex in range {
                scene.state.x[vertex] += delta;
            }
            *moved.entry(id).or_insert(0.0) += SEPARATION_STEP_M;
        }
    }
    if moved.is_empty() {
        if solved {
            return Vec::new();
        }
        let mut warnings = arrangement_warnings(scene);
        if let Some(error) = unresolved {
            warnings.push(format!(
                "The initial arrangement still intersects the new avatar: {error}. Move the affected panels outward in the 3D view before playing; the 2D board layout does not move cloth."
            ));
        }
        return warnings;
    }
    project.state = None;
    project.mesh_signature = None;
    match project.compile() {
        Ok(next) => *scene = next,
        Err(error) => {
            return vec![format!(
                "Panels were moved to clear the new avatar but the arrangement could not be rebuilt: {error}"
            )];
        }
    }
    let listed = moved
        .iter()
        .map(|(id, depth)| format!("{id} ({:.0} mm)", depth * 1000.0))
        .collect::<Vec<_>>()
        .join(", ");
    match VbdSolver::new(
        scene.garment.clone(),
        Some(&scene.collider.surface),
        &scene.state,
    ) {
        Ok(_) => vec![format!(
            "Panels {listed} intersected the new avatar and were moved outward so the initial state is separated. Check the 3D arrangement before playing; the 2D board layout does not move cloth."
        )],
        Err(error) => vec![format!(
            "Panels {listed} were moved outward, but the initial arrangement still intersects the new avatar: {error}. Move the affected panels in the 3D view before playing."
        )],
    }
}

/// Faces named by the OGC initial-intersection error.
fn error_face_pair(error: &str) -> Option<(usize, usize)> {
    let rest = error.split("between faces ").nth(1)?;
    let mut parts = rest.split(" and ");
    let parse = |text: &str| text.split(';').next()?.trim().parse::<usize>().ok();
    Some((parse(parts.next()?)?, parse(parts.next()?)?))
}

fn panel_of_face(face: usize, garment: &CompiledGarment) -> Option<&str> {
    if face >= garment.mesh.triangles.len() {
        return None;
    }
    garment
        .panels
        .iter()
        .find(|panel| panel.triangles.contains(&face))
        .map(|panel| panel.id.as_str())
}

/// Panels owning the cloth faces named by the OGC initial-intersection error.
fn panels_for_faces(first: usize, second: usize, garment: &CompiledGarment) -> Vec<String> {
    let mut panels = Vec::new();
    for face in [first, second] {
        if let Some(id) = panel_of_face(face, garment)
            && !panels.iter().any(|panel: &String| panel == id)
        {
            panels.push(id.to_owned());
        }
    }
    panels
}

/// Report panels whose authored arrangement starts inside the new collider for
/// arrangements that are not moved automatically.
fn arrangement_warnings(scene: &FittingScene) -> Vec<String> {
    let mut conflicts: Vec<(&str, f64)> = Vec::new();
    for panel in &scene.garment.panels {
        let mut deepest: f64 = 0.0;
        for index in panel.vertices.clone() {
            let point = scene.state.x[index];
            if !point.is_finite() {
                continue;
            }
            deepest = deepest.max(-scene.collider.signed_distance(point));
        }
        if deepest > ARRANGEMENT_CONFLICT_M {
            conflicts.push((&panel.id, deepest));
        }
    }
    if conflicts.is_empty() {
        return Vec::new();
    }
    conflicts.sort_by(|a, b| b.1.total_cmp(&a.1));
    let listed = conflicts
        .iter()
        .take(4)
        .map(|(id, depth)| format!("{id} ({:.1} mm)", depth * 1000.0))
        .collect::<Vec<_>>()
        .join(", ");
    let more = conflicts.len().saturating_sub(4);
    let suffix = if more > 0 { ", ..." } else { "" };
    vec![format!(
        "The authored panel arrangement intersects the new avatar: {listed}{suffix} inside its surface. Move those panels outward in the 3D view before playing; the 2D board layout does not move cloth in 3D."
    )]
}

pub struct Prepared {
    project: FittingProject,
    scene: FittingScene,
    asset: StagedAvatar,
    display: Arc<SurfaceMesh>,
    warnings: Vec<String>,
}
impl Prepared {
    /// Every imported source mesh, including meshes not selected as colliders.
    pub fn display(&self) -> &SurfaceMesh {
        &self.display
    }

    /// Shared display surface for the runner's USD stage.
    pub fn display_surface(&self) -> Arc<SurfaceMesh> {
        Arc::clone(&self.display)
    }
    /// Conflicts detected after compiling the replacement scene, before any
    /// workspace field is installed.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    pub fn into_parts(self) -> (FittingProject, FittingScene, StagedAvatar) {
        (self.project, self.scene, self.asset)
    }
}
