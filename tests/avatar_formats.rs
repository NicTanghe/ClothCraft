//! Independent fixtures for the FBX and USD avatar importers.
//!
//! Every fixture is generated here, so the tests do not depend on exported
//! third-party assets. Geometry values, units, axis settings and transforms
//! are stated explicitly rather than inferred from the importer.

use clothcraft::{
    avatar_import::Candidate,
    fitting::FittingProject,
    garment::presets::GarmentKind,
    vbd::{VbdSettings, VbdSolver},
};
use glam::DVec3;
use openusd::{
    gf,
    sdf::{self, ChildrenKey, FieldKey, SpecType, Specifier, Value},
    usd::Stage,
    usd_utils::create_new_usdz_package,
    usdc::CrateWriter,
};
use std::io::Write;
use std::path::{Path, PathBuf};

fn project(dir: &Path) -> FittingProject {
    let path = dir.join("old.obj");
    std::fs::write(&path, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").unwrap();
    FittingProject::basic(GarmentKind::Dress, 1, path)
}

fn geometry(path: &Path) -> (Vec<DVec3>, Vec<[usize; 3]>) {
    let dir = tempfile::tempdir().unwrap();
    let current = project(dir.path());
    let candidate = Candidate::load(path).unwrap();
    let prepared = candidate.prepare(&current, VbdSettings::default()).unwrap();
    let (_, scene, _) = prepared.into_parts();
    (
        scene.collider.surface.vertices.clone(),
        scene.collider.surface.triangles.clone(),
    )
}

fn assert_close(actual: DVec3, expected: DVec3) {
    assert!(
        (actual - expected).length() < 1e-9,
        "expected {expected:?}, got {actual:?}"
    );
}

// ---------------------------------------------------------------------------
// FBX fixtures
// ---------------------------------------------------------------------------

fn ascii_fbx(global: &str, translation: &str) -> String {
    format!(
        r#"; FBX 7.4.0 project file
FBXHeaderExtension: {{
    FBXHeaderVersion: 1003
    FBXVersion: 7400
}}
GlobalSettings: {{
    Version: 1000
    Properties70: {{
{global}    }}
}}
Objects: {{
    Geometry: 100, "Geometry::Body", "Mesh" {{
        Vertices: *9 {{
            a: 0,0,0,1,0,0,0,1,0
        }}
        PolygonVertexIndex: *3 {{
            a: 0,1,-3
        }}
    }}
    Model: 200, "Model::Root", "Null" {{
        Properties70: {{
{translation}        }}
    }}
    Model: 201, "Model::Body", "Mesh" {{
    }}
}}
Connections: {{
    C: "OO", 100, 201
    C: "OO", 201, 200
    C: "OO", 200, 0
}}
"#
    )
}

const Y_UP_CENTIMETRES: &str = r#"        P: "UnitScaleFactor", "double", "Number", "",1
        P: "UpAxis", "int", "Integer", "",1
        P: "UpAxisSign", "int", "Integer", "",1
        P: "FrontAxis", "int", "Integer", "",2
        P: "FrontAxisSign", "int", "Integer", "",1
        P: "CoordAxis", "int", "Integer", "",0
        P: "CoordAxisSign", "int", "Integer", "",1
"#;

const Z_UP_METRES: &str = r#"        P: "UnitScaleFactor", "double", "Number", "",100
        P: "UpAxis", "int", "Integer", "",2
        P: "UpAxisSign", "int", "Integer", "",1
        P: "FrontAxis", "int", "Integer", "",1
        P: "FrontAxisSign", "int", "Integer", "",-1
        P: "CoordAxis", "int", "Integer", "",0
        P: "CoordAxisSign", "int", "Integer", "",1
"#;

const TRANSLATED_METRES: &str = r#"            P: "Lcl Translation", "Lcl Translation", "", "A",1,2,3
"#;

#[test]
fn ascii_fbx_applies_model_translation_and_centimetre_units() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("avatar.fbx");
    std::fs::write(&path, ascii_fbx(Y_UP_CENTIMETRES, TRANSLATED_METRES)).unwrap();
    let (vertices, triangles) = geometry(&path);
    // 1 FBX unit = 1 cm, so (1, 2, 3) model units are (0.01, 0.02, 0.03) m.
    assert_close(vertices[0], DVec3::new(0.01, 0.02, 0.03));
    assert_close(vertices[1], DVec3::new(0.02, 0.02, 0.03));
    assert_close(vertices[2], DVec3::new(0.01, 0.03, 0.03));
    assert_eq!(triangles, vec![[0, 1, 2]]);
}

#[test]
fn ascii_fbx_z_up_scene_is_rotated_to_y_up() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("z-up.fbx");
    let text = ascii_fbx(Z_UP_METRES, "").replace("a: 0,0,0,1,0,0,0,1,0", "a: 0,0,0,1,0,0,0,0,1");
    std::fs::write(&path, text).unwrap();
    let candidate = Candidate::load(&path).unwrap();
    assert!(
        candidate
            .warnings
            .iter()
            .any(|warning| warning.contains("axis") || warning.contains("Y-up")),
        "{:?}",
        candidate.warnings
    );
    let (vertices, _) = geometry(&path);
    // Z-up: the authored +Z point becomes +Y and the authored +X stays +X.
    assert_close(vertices[0], DVec3::ZERO);
    assert_close(vertices[1], DVec3::X);
    assert_close(vertices[2], DVec3::Y);
}

/// One binary FBX node, encoded with absolute end offsets once its start is
/// known. FBX stores the absolute byte offset of each node's end.
struct FbxNode {
    name: String,
    props: Vec<Vec<u8>>,
    children: Vec<FbxNode>,
}

impl FbxNode {
    fn new(name: &str, props: Vec<Vec<u8>>, children: Vec<FbxNode>) -> Self {
        Self {
            name: name.to_owned(),
            props,
            children,
        }
    }
}

fn encode_fbx_node(node: &FbxNode, start: usize, wide: bool) -> Vec<u8> {
    let mut property_bytes = Vec::new();
    for prop in &node.props {
        property_bytes.extend_from_slice(prop);
    }
    // FBX 7500 widened the record header fields to 64 bits; property and
    // array headers keep their 32-bit sizes.
    let header = if wide { 25 } else { 13 } + node.name.len() + property_bytes.len();
    let mut offset = start + header;
    let mut child_bytes = Vec::new();
    for child in &node.children {
        let bytes = encode_fbx_node(child, offset, wide);
        offset += bytes.len();
        child_bytes.extend(bytes);
    }
    if !node.children.is_empty() {
        let sentinel = if wide { 25 } else { 13 };
        child_bytes.extend(vec![0u8; sentinel]);
        offset += sentinel;
    }
    let mut out = Vec::new();
    if wide {
        out.extend((offset as u64).to_le_bytes());
        out.extend((node.props.len() as u64).to_le_bytes());
        out.extend((property_bytes.len() as u64).to_le_bytes());
    } else {
        out.extend((offset as u32).to_le_bytes());
        out.extend((node.props.len() as u32).to_le_bytes());
        out.extend((property_bytes.len() as u32).to_le_bytes());
    }
    out.push(node.name.len() as u8);
    out.extend(node.name.as_bytes());
    out.extend(property_bytes);
    out.extend(child_bytes);
    out
}

fn fbx_i32(value: i32) -> Vec<u8> {
    let mut out = vec![b'I'];
    out.extend(value.to_le_bytes());
    out
}

fn fbx_string(value: &str) -> Vec<u8> {
    let mut out = vec![b'S'];
    out.extend((value.len() as u32).to_le_bytes());
    out.extend(value.as_bytes());
    out
}

fn fbx_f64_array(values: &[f64], compressed: bool) -> Vec<u8> {
    let mut out = vec![b'd'];
    out.extend((values.len() as u32).to_le_bytes());
    let raw: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    if compressed {
        out.extend(1u32.to_le_bytes());
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();
        out.extend((compressed.len() as u32).to_le_bytes());
        out.extend(compressed);
    } else {
        out.extend(0u32.to_le_bytes());
        out.extend((raw.len() as u32).to_le_bytes());
        out.extend(raw);
    }
    out
}

fn fbx_i32_array(values: &[i32]) -> Vec<u8> {
    let mut out = vec![b'i'];
    out.extend((values.len() as u32).to_le_bytes());
    let raw: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    out.extend(0u32.to_le_bytes());
    out.extend((raw.len() as u32).to_le_bytes());
    out.extend(raw);
    out
}

fn fbx_f64(value: f64) -> Vec<u8> {
    let mut out = vec![b'D'];
    out.extend(value.to_le_bytes());
    out
}

fn fbx_property(name: &str, kind: &str, value: f64) -> FbxNode {
    FbxNode::new(
        "P",
        vec![
            fbx_string(name),
            fbx_string(kind),
            fbx_string("Number"),
            fbx_string(""),
            fbx_f64(value),
        ],
        vec![],
    )
}

fn fbx_int_property(name: &str, kind: &str, value: i32) -> FbxNode {
    FbxNode::new(
        "P",
        vec![
            fbx_string(name),
            fbx_string(kind),
            fbx_string("Integer"),
            fbx_string(""),
            fbx_i32(value),
        ],
        vec![],
    )
}

/// Binary FBX with a zlib-compressed vertex array, in the 7.4 or 7.5 record
/// layout.
fn binary_fbx(version: u32) -> Vec<u8> {
    let wide = version >= 7500;
    let mut out = b"Kaydara FBX Binary  \x00".to_vec();
    out.extend([0x1a, 0x00]);
    out.extend(version.to_le_bytes());

    let global = FbxNode::new(
        "GlobalSettings",
        vec![],
        vec![FbxNode::new(
            "Properties70",
            vec![],
            vec![
                fbx_property("UnitScaleFactor", "double", 100.0),
                fbx_int_property("UpAxis", "int", 1),
                fbx_int_property("FrontAxis", "int", 2),
                fbx_int_property("CoordAxis", "int", 0),
            ],
        )],
    );
    let geometry = FbxNode::new(
        "Geometry",
        vec![
            fbx_i32(100),
            fbx_string("Geometry::Body"),
            fbx_string("Mesh"),
        ],
        vec![
            FbxNode::new(
                "Vertices",
                vec![fbx_f64_array(
                    &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                    true,
                )],
                vec![],
            ),
            FbxNode::new(
                "PolygonVertexIndex",
                vec![fbx_i32_array(&[0, 1, -3])],
                vec![],
            ),
        ],
    );
    let model = FbxNode::new(
        "Model",
        vec![fbx_i32(200), fbx_string("Model::Root"), fbx_string("Null")],
        vec![FbxNode::new(
            "Properties70",
            vec![],
            vec![FbxNode::new(
                "P",
                vec![
                    fbx_string("Lcl Translation"),
                    fbx_string("Lcl Translation"),
                    fbx_string(""),
                    fbx_string("A"),
                    fbx_f64(1.0),
                    fbx_f64(2.0),
                    fbx_f64(3.0),
                ],
                vec![],
            )],
        )],
    );
    let child_model = FbxNode::new(
        "Model",
        vec![fbx_i32(201), fbx_string("Model::Body"), fbx_string("Mesh")],
        vec![],
    );
    let objects = FbxNode::new("Objects", vec![], vec![geometry, model, child_model]);
    let connections = FbxNode::new(
        "Connections",
        vec![],
        vec![
            FbxNode::new(
                "C",
                vec![fbx_string("OO"), fbx_i32(100), fbx_i32(201)],
                vec![],
            ),
            FbxNode::new(
                "C",
                vec![fbx_string("OO"), fbx_i32(201), fbx_i32(200)],
                vec![],
            ),
            FbxNode::new(
                "C",
                vec![fbx_string("OO"), fbx_i32(200), fbx_i32(0)],
                vec![],
            ),
        ],
    );
    for node in [global, objects, connections] {
        let bytes = encode_fbx_node(&node, out.len(), wide);
        out.extend(bytes);
    }
    out.extend(vec![0u8; if wide { 25 } else { 13 }]);
    out
}

#[test]
fn binary_fbx_reads_compressed_arrays_and_parent_transform() {
    for version in [7400u32, 7500, 7700] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("avatar-{version}.fbx"));
        std::fs::write(&path, binary_fbx(version)).unwrap();
        let (vertices, triangles) = geometry(&path);
        assert_close(vertices[0], DVec3::new(1.0, 2.0, 3.0));
        assert_close(vertices[1], DVec3::new(2.0, 2.0, 3.0));
        assert_close(vertices[2], DVec3::new(1.0, 3.0, 3.0));
        assert_eq!(triangles, vec![[0, 1, 2]], "version {version}");
    }
}

#[test]
fn ascii_fbx_with_a_byte_order_mark_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bom.fbx");
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(ascii_fbx(Y_UP_CENTIMETRES, "").as_bytes());
    std::fs::write(&path, bytes).unwrap();
    let (vertices, triangles) = geometry(&path);
    assert_eq!(vertices.len(), 3);
    assert_eq!(triangles, vec![[0, 1, 2]]);
}

#[test]
fn fbx_6_is_rejected_with_an_explicit_message() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.fbx");
    std::fs::write(
        &path,
        "FBXHeaderExtension: {\n  FBXVersion: 6100\n}\nObjects: {\n}\n",
    )
    .unwrap();
    let error = Candidate::load(&path).unwrap_err();
    assert!(error.contains("FBX version"), "{error}");
}

#[test]
fn degenerate_fbx_triangles_are_dropped_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("degenerate.fbx");
    let text = ascii_fbx(Y_UP_CENTIMETRES, "").replace("a: 0,1,-3", "a: 0,1,-3,0,0,-1");
    std::fs::write(&path, text).unwrap();
    let candidate = Candidate::load(&path).unwrap();
    assert!(
        candidate
            .warnings
            .iter()
            .any(|warning| warning.contains("zero-area")),
        "{:?}",
        candidate.warnings
    );
    let (_, triangles) = geometry(&path);
    assert_eq!(triangles.len(), 1);
}

// ---------------------------------------------------------------------------
// USD fixtures
// ---------------------------------------------------------------------------

fn usda(meters_per_unit: f64, up_axis: &str, translation: &str) -> String {
    format!(
        r#"#usda 1.0
(
    defaultPrim = "World"
    metersPerUnit = {meters_per_unit}
    upAxis = "{up_axis}"
)

def Xform "World"
{{
    double3 xformOp:translate = {translation}
    uniform token[] xformOpOrder = ["xformOp:translate"]

    def Mesh "Body"
    {{
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        uniform token orientation = "rightHanded"
        uniform token subdivisionScheme = "none"
    }}
}}
"#
    )
}

#[test]
fn usda_applies_stage_units_and_xform_stack() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("avatar.usda");
    // 0.01 m per unit with a (100, 200, 300) offset gives (1, 2, 3) m.
    std::fs::write(&path, usda(0.01, "Y", "(100, 200, 300)")).unwrap();
    let (vertices, triangles) = geometry(&path);
    assert_close(vertices[0], DVec3::new(1.0, 2.0, 3.0));
    assert_close(vertices[1], DVec3::new(1.01, 2.0, 3.0));
    assert_close(vertices[2], DVec3::new(1.0, 2.01, 3.0));
    assert_eq!(triangles, vec![[0, 1, 2]]);
}

#[test]
fn usd_z_up_scene_is_rotated_to_y_up() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("z-up.usda");
    std::fs::write(&path, usda(1.0, "Z", "(0, 0, 0)")).unwrap();
    let (vertices, _) = geometry(&path);
    // The default Y-up image of the same file is unchanged; Z-up rotates +Y
    // into -Z, so the second point moves from +Y to -Z.
    assert_close(vertices[0], DVec3::ZERO);
    assert_close(vertices[1], DVec3::X);
    assert_close(vertices[2], -DVec3::Z);
}

#[test]
fn usd_missing_meters_per_unit_uses_the_usd_centimetre_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("no-units.usda");
    let text = r#"#usda 1.0
(
    defaultPrim = "World"
)
def Xform "World"
{
    def Mesh "Body"
    {
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 0), (100, 0, 0), (0, 100, 0)]
    }
}
"#;
    std::fs::write(&path, text).unwrap();
    let candidate = Candidate::load(&path).unwrap();
    assert!(
        candidate
            .warnings
            .iter()
            .any(|warning| warning.contains("metersPerUnit")),
        "{:?}",
        candidate.warnings
    );
    let (vertices, _) = geometry(&path);
    assert_close(vertices[1], DVec3::X);
}

#[test]
fn usdc_binary_stage_follows_the_same_import_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("avatar.usdc");
    let mut data = sdf::Data::new();
    let root = data.create_spec(sdf::Path::abs_root(), SpecType::PseudoRoot);
    root.add(
        ChildrenKey::PrimChildren,
        Value::TokenVec(vec!["World".into()]),
    );
    root.add(FieldKey::DefaultPrim, Value::Token("World".into()));
    root.add("metersPerUnit", Value::Double(1.0));
    root.add("upAxis", Value::Token("Y".into()));

    let world = data.create_spec(sdf::Path::new("/World").unwrap(), SpecType::Prim);
    world.add(FieldKey::Specifier, Value::Specifier(Specifier::Def));
    world.add(FieldKey::TypeName, Value::Token("Xform".into()));
    world.add(
        ChildrenKey::PrimChildren,
        Value::TokenVec(vec!["Body".into()]),
    );
    world.add(
        ChildrenKey::PropertyChildren,
        Value::TokenVec(vec!["xformOpOrder".into(), "xformOp:translate".into()]),
    );

    let translate = data.create_spec(
        sdf::Path::new("/World.xformOp:translate").unwrap(),
        SpecType::Attribute,
    );
    translate.add(FieldKey::TypeName, Value::Token("double3".into()));
    translate.add(
        FieldKey::Default,
        Value::Vec3d(gf::Vec3d {
            x: 100.0,
            y: 200.0,
            z: 300.0,
        }),
    );
    let order = data.create_spec(
        sdf::Path::new("/World.xformOpOrder").unwrap(),
        SpecType::Attribute,
    );
    order.add(FieldKey::TypeName, Value::Token("token[]".into()));
    order.add(
        FieldKey::Default,
        Value::TokenVec(vec!["xformOp:translate".into()]),
    );

    let body = data.create_spec(sdf::Path::new("/World/Body").unwrap(), SpecType::Prim);
    body.add(FieldKey::Specifier, Value::Specifier(Specifier::Def));
    body.add(FieldKey::TypeName, Value::Token("Mesh".into()));
    body.add(
        ChildrenKey::PropertyChildren,
        Value::TokenVec(vec![
            "faceVertexCounts".into(),
            "faceVertexIndices".into(),
            "points".into(),
        ]),
    );
    let counts = data.create_spec(
        sdf::Path::new("/World/Body.faceVertexCounts").unwrap(),
        SpecType::Attribute,
    );
    counts.add(FieldKey::TypeName, Value::Token("int[]".into()));
    counts.add(FieldKey::Default, Value::IntVec(vec![3]));
    let indices = data.create_spec(
        sdf::Path::new("/World/Body.faceVertexIndices").unwrap(),
        SpecType::Attribute,
    );
    indices.add(FieldKey::TypeName, Value::Token("int[]".into()));
    indices.add(FieldKey::Default, Value::IntVec(vec![0, 1, 2]));
    let points = data.create_spec(
        sdf::Path::new("/World/Body.points").unwrap(),
        SpecType::Attribute,
    );
    points.add(FieldKey::TypeName, Value::Token("point3f[]".into()));
    points.add(
        FieldKey::Default,
        Value::Vec3fVec(vec![
            gf::Vec3f {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            gf::Vec3f {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            gf::Vec3f {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ]),
    );
    CrateWriter::write_to_file(&data, path.to_str().unwrap()).unwrap();

    let (vertices, triangles) = geometry(&path);
    assert_close(vertices[0], DVec3::new(100.0, 200.0, 300.0));
    assert_close(vertices[1], DVec3::new(101.0, 200.0, 300.0));
    assert_close(vertices[2], DVec3::new(100.0, 201.0, 300.0));
    assert_eq!(triangles, vec![[0, 1, 2]]);

    // The same stage packaged as USDZ must follow the same path.
    let usdz = dir.path().join("avatar.usdz");
    let stage = Stage::open(path.to_str().unwrap()).unwrap();
    create_new_usdz_package(&stage, path.to_str().unwrap(), &usdz, None).unwrap();
    let (packaged, packaged_triangles) = geometry(&usdz);
    assert_eq!(packaged.len(), 3);
    assert_close(packaged[0], DVec3::new(100.0, 200.0, 300.0));
    assert_eq!(packaged_triangles, vec![[0, 1, 2]]);
}

#[test]
fn usd_left_handed_mesh_flips_winding_and_invisible_meshes_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("winding.usda");
    let text = r#"#usda 1.0
(
    metersPerUnit = 1
)
def Xform "World"
{
    def Mesh "Body"
    {
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
        uniform token orientation = "leftHanded"
    }
    def Mesh "Hidden"
    {
        token visibility = "invisible"
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(0, 0, 1), (1, 0, 1), (0, 1, 1)]
    }
}
"#;
    std::fs::write(&path, text).unwrap();
    let candidate = Candidate::load(&path).unwrap();
    assert!(
        candidate
            .warnings
            .iter()
            .any(|warning| warning.contains("left-handed"))
    );
    assert!(
        candidate
            .warnings
            .iter()
            .any(|warning| warning.contains("invisible"))
    );
    let (vertices, triangles) = geometry(&path);
    assert_eq!(vertices.len(), 3);
    assert_eq!(triangles, vec![[0, 2, 1]]);
}

#[test]
fn unsupported_avatar_extension_is_still_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("mesh.stl");
    std::fs::write(&path, "solid mesh\nendsolid mesh\n").unwrap();
    let error = Candidate::load(&path).unwrap_err();
    assert!(error.contains("OBJ"), "{error}");
}

// ---------------------------------------------------------------------------
// Multi-mesh selection and arrangement warnings
// ---------------------------------------------------------------------------

/// Mixamo-style FBX: a body mesh with two triangles and a smaller helper mesh.
fn ascii_fbx_two_meshes() -> String {
    r#"; FBX 7.4.0 project file
FBXHeaderExtension: {
    FBXHeaderVersion: 1003
    FBXVersion: 7400
}
GlobalSettings: {
    Version: 1000
    Properties70: {
        P: "UnitScaleFactor", "double", "Number", "",100
        P: "UpAxis", "int", "Integer", "",1
        P: "FrontAxis", "int", "Integer", "",2
        P: "CoordAxis", "int", "Integer", "",0
    }
}
Objects: {
    Geometry: 100, "Geometry::Body", "Mesh" {
        Vertices: *12 {
            a: 0,0,0,1,0,0,1,1,0,0,1,0
        }
        PolygonVertexIndex: *6 {
            a: 0,1,-3,0,2,-4
        }
    }
    Geometry: 101, "Geometry::Marker", "Mesh" {
        Vertices: *9 {
            a: 2,0,0,3,0,0,2,1,0
        }
        PolygonVertexIndex: *3 {
            a: 0,1,-3
        }
    }
    Model: 200, "Model::Root", "Null" {
        Properties70: {
            P: "Lcl Translation", "Lcl Translation", "", "A",0,0,0
        }
    }
    Model: 201, "Model::Body", "Mesh" {
    }
    Model: 202, "Model::Marker", "Mesh" {
    }
}
Connections: {
    C: "OO", 100, 201
    C: "OO", 101, 202
    C: "OO", 201, 200
    C: "OO", 202, 200
    C: "OO", 200, 0
}
"#
    .to_owned()
}

#[test]
fn fbx_meshes_are_listed_and_the_largest_mesh_is_selected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two-meshes.fbx");
    std::fs::write(&path, ascii_fbx_two_meshes()).unwrap();
    let mut candidate = Candidate::load(&path).unwrap();
    assert_eq!(candidate.meshes.len(), 2);
    let names: Vec<_> = candidate
        .meshes
        .iter()
        .map(|mesh| mesh.name.as_str())
        .collect();
    assert_eq!(names, ["Body", "Marker"]);
    // 4 vertices / 2 triangles in the body, 3 vertices / 1 triangle in the marker.
    assert_eq!((candidate.vertices, candidate.triangles), (4, 2));
    assert!(candidate.meshes[0].selected);
    assert!(!candidate.meshes[1].selected);
    assert!(
        candidate
            .warnings
            .iter()
            .any(|warning| warning.contains("Alpha_Joints")
                || warning.contains("marker")
                || warning.contains("Marker"))
    );

    candidate.toggle_mesh(1).unwrap();
    assert_eq!((candidate.vertices, candidate.triangles), (7, 3));
    candidate.toggle_mesh(0).unwrap();
    assert_eq!((candidate.vertices, candidate.triangles), (3, 1));
    assert!(
        candidate.toggle_mesh(1).is_err(),
        "the last mesh stays selected"
    );

    // The staged OBJ follows the selection, so preparation uses only the marker.
    let current = project(dir.path());
    let prepared = candidate.prepare(&current, VbdSettings::default()).unwrap();
    let (_, scene, _) = prepared.into_parts();
    assert_eq!(scene.collider.surface.vertices.len(), 3);
    assert_eq!(scene.collider.surface.triangles.len(), 1);
}

#[test]
fn usd_mesh_selection_lists_visible_prims() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two-meshes.usda");
    let text = r#"#usda 1.0
(
    metersPerUnit = 1
)
def Xform "World"
{
    def Mesh "Body"
    {
        int[] faceVertexCounts = [3, 3]
        int[] faceVertexIndices = [0, 1, 2, 0, 2, 3]
        point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
    }
    def Mesh "Helper"
    {
        int[] faceVertexCounts = [3]
        int[] faceVertexIndices = [0, 1, 2]
        point3f[] points = [(2, 0, 0), (3, 0, 0), (2, 1, 0)]
    }
}
"#;
    std::fs::write(&path, text).unwrap();
    let candidate = Candidate::load(&path).unwrap();
    assert_eq!(candidate.meshes.len(), 2);
    assert_eq!(
        candidate
            .meshes
            .iter()
            .map(|mesh| mesh.name.as_str())
            .collect::<Vec<_>>(),
        ["World/Body", "World/Helper"]
    );
    assert!(candidate.meshes[0].selected, "the body has more triangles");
    assert!(!candidate.meshes[1].selected);
    assert_eq!((candidate.vertices, candidate.triangles), (4, 2));
}

/// Outward-wound cube around the T-shirt front panel's authored plane.
fn cube_obj() -> &'static str {
    "v -0.2 1.0 0.36
v 0.2 1.0 0.36
v 0.2 1.2 0.36
v -0.2 1.2 0.36
v -0.2 1.0 0.40
v 0.2 1.0 0.40
v 0.2 1.2 0.40
v -0.2 1.2 0.40
f 5 6 7
f 5 7 8
f 1 4 3
f 1 3 2
f 1 5 8
f 1 8 4
f 2 3 7
f 2 7 6
f 1 2 6
f 1 6 5
f 4 8 7
f 4 7 3
"
}

#[test]
fn arrangement_conflicts_are_separated_before_install() {
    let dir = tempfile::tempdir().unwrap();
    let avatar = dir.path().join("old.obj");
    std::fs::write(&avatar, "v 0 0 -10\nv 1 0 -10\nv 0 1 -10\nf 1 2 3\n").unwrap();
    let current = FittingProject::basic(GarmentKind::Tshirt, 1, avatar);
    let before = current
        .template
        .panels
        .iter()
        .find(|panel| panel.id == "front")
        .unwrap()
        .placement
        .translation;
    let cube = dir.path().join("cube.obj");
    std::fs::write(&cube, cube_obj()).unwrap();
    let candidate = Candidate::load(&cube).unwrap();
    let prepared = candidate.prepare(&current, VbdSettings::default()).unwrap();
    let warnings = prepared.warnings();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains("moved outward") && warning.contains("front")),
        "{warnings:?}"
    );
    assert!(warnings.iter().any(|warning| warning.contains("2D board")));
    let (project, scene, _) = prepared.into_parts();
    let front = project
        .template
        .panels
        .iter()
        .find(|panel| panel.id == "front")
        .unwrap();
    assert_ne!(
        front.placement.translation, before,
        "the front panel's authored placement must move"
    );
    assert!(
        VbdSolver::new(
            scene.garment.clone(),
            Some(&scene.collider.surface),
            &scene.state
        )
        .is_ok(),
        "the separated arrangement must pass the solver's initial-state check"
    );
}
