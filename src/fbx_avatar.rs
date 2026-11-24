//! Bounded static-geometry FBX import for avatar replacement.
//!
//! Independently implemented from the public FBX 7.x container documentation:
//! the binary node/property record layout and the ASCII node grammar. No FBX
//! SDK is linked and no third-party FBX parser is embedded.
//!
//! # Supported subset
//! - FBX 7.x ASCII documents and FBX 7.x binary documents, including
//!   zlib-compressed property arrays. FBX 6.x and non-FBX files are rejected.
//! - `Geometry` objects of type `Mesh` with `Vertices` and
//!   `PolygonVertexIndex`. Polygons are fan-triangulated, matching the OBJ
//!   importer; the final index of each polygon carries FBX's `~`/negative
//!   end marker.
//! - `Connections` `OO` links place geometry under its `Model`s and compose the
//!   model hierarchy. `Model` transforms use `Lcl Translation`,
//!   `Lcl Rotation`, `Lcl Scaling`, `PreRotation` and `RotationOrder` when
//!   `RotationActive` is set, plus `GeometricTranslation`, `GeometricRotation`
//!   and `GeometricScaling`. Rotation pivots and offsets are not applied; a
//!   non-default value is reported in the warnings.
//! - `GlobalSettings` `UnitScaleFactor` (centimetres per unit) scales to
//!   metres, and the `UpAxis`/`FrontAxis`/`CoordAxis` basis is rotated to the
//!   editor's right-handed Y-up frame.
//!
//! # Losses and limits
//! Static surface positions only: UVs, authored normals, smoothing groups,
//! materials, textures, blend shapes, skin clusters and bind poses, animation,
//! cameras, lights and constraints are discarded. A skinned mesh is imported
//! in its base (bind) pose and reported. Non-uniform `InheritType` variants are
//! not modelled exactly; a model using one is reported rather than silently
//! rewritten. The reader does not repair initial intersections, open surfaces
//! or inconsistent winding, and it does not prove that the result is closed.
//!
//! Zero-area triangles produced by triangulation are dropped and counted so a
//! dense avatar is not rejected for a single malformed polygon; non-finite
//! coordinates and out-of-range indices remain errors.

use crate::{avatar::SurfaceMesh, gltf_avatar::Limits};
use glam::{DMat3, DMat4, DVec3};
use std::path::Path;

const BINARY_MAGIC: &[u8] = b"Kaydara FBX Binary  \x00";
/// `UnitScaleFactor` is centimetres per source unit, and the editor uses metres.
const CENTIMETRES_PER_METRE: f64 = 100.0;
const FIRST_SUPPORTED_VERSION: u32 = 7000;

/// Geometry and loss report for one static FBX document.
#[derive(Debug)]
pub struct ImportedAvatar {
    /// Every static mesh merged; the avatar importer selects which parts collide.
    pub mesh: SurfaceMesh,
    /// Named per-`Geometry` meshes with model transforms applied, in document order.
    pub parts: Vec<(String, SurfaceMesh)>,
    pub warnings: Vec<String>,
}

/// Read `path` as an FBX 7.x document and flatten its static meshes.
pub fn load(path: &Path, limits: Limits) -> Result<ImportedAvatar, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("Cannot read avatar {}: {e}", path.display()))?;
    if bytes.len() > limits.document_bytes {
        return Err("Avatar document exceeds the byte limit".into());
    }
    import(&bytes, limits)
}

/// Parse FBX bytes. Kept separate so tests can exercise both encodings without
/// touching the file system.
pub fn import(bytes: &[u8], limits: Limits) -> Result<ImportedAvatar, String> {
    let document = if bytes.starts_with(BINARY_MAGIC) {
        Binary::new(bytes, &limits)?.parse()?
    } else {
        Ascii::new(bytes, &limits)?.parse()?
    };
    if document.version < FIRST_SUPPORTED_VERSION {
        return Err(format!(
            "FBX version {} is not supported; export FBX 7.x (2011 or newer)",
            document.version
        ));
    }
    flatten(&document, limits)
}

#[derive(Debug, Clone)]
enum Prop {
    Int(i64),
    Float(f64),
    Str(String),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
    Floats(Vec<f64>),
}

impl Prop {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(*value as f64),
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }
    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Float(value) if value.fract() == 0.0 => Some(*value as i64),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    name: String,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|node| node.name == name)
    }
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |node| node.name == name)
    }
    fn prop(&self, index: usize) -> Option<&Prop> {
        self.props.get(index)
    }
    fn string(&self, index: usize) -> Option<&str> {
        self.prop(index)?.as_str()
    }
    /// Flatten a numeric array, whether it was a binary array property or an
    /// ASCII `*n { a: ... }` child node.
    fn numbers(&self) -> Option<Vec<f64>> {
        if let Some(property) = self.props.first() {
            match property {
                Prop::Ints(values) => {
                    return Some(values.iter().map(|&v| f64::from(v)).collect());
                }
                Prop::Longs(values) => {
                    return Some(values.iter().map(|&v| v as f64).collect());
                }
                Prop::Floats(values) => return Some(values.clone()),
                _ => {}
            }
        }
        if let Some(array) = self.child("a") {
            return array.props.iter().map(Prop::as_f64).collect();
        }
        if !self.props.is_empty() {
            return self.props.iter().map(Prop::as_f64).collect();
        }
        None
    }
    fn integers(&self) -> Option<Vec<i64>> {
        if let Some(Prop::Ints(values)) = self.props.first() {
            return Some(values.iter().map(|&v| i64::from(v)).collect());
        }
        if let Some(Prop::Longs(values)) = self.props.first() {
            return Some(values.clone());
        }
        if let Some(array) = self.child("a") {
            return array.props.iter().map(Prop::as_i64).collect();
        }
        if !self.props.is_empty() {
            return self.props.iter().map(Prop::as_i64).collect();
        }
        None
    }
    /// `P: "name", "type", "label", "flags", <values...>` property records.
    fn p_values(&self) -> Option<(&str, &[Prop])> {
        let name = self.string(0)?;
        Some((name, self.props.get(4..)?))
    }
}

struct Document {
    version: u32,
    nodes: Vec<Node>,
}

impl Document {
    fn top(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.name == name)
    }
}

// ---------------------------------------------------------------------------
// Binary FBX 7.x
// ---------------------------------------------------------------------------

struct Binary<'a> {
    bytes: &'a [u8],
    limits: &'a Limits,
    nodes: usize,
    /// FBX 7500 widened the node record's offset and length fields to 64 bits.
    wide: bool,
}

impl<'a> Binary<'a> {
    fn new(bytes: &'a [u8], limits: &'a Limits) -> Result<Self, String> {
        if bytes.len() < 27 {
            return Err("Truncated FBX binary header".into());
        }
        if &bytes[..BINARY_MAGIC.len()] != BINARY_MAGIC || bytes[21..23] != [0x1a, 0x00] {
            return Err("Not an FBX binary document".into());
        }
        let version = u32::from_le_bytes(bytes[23..27].try_into().unwrap());
        Ok(Self {
            bytes,
            limits,
            nodes: 0,
            wide: version >= 7500,
        })
    }

    fn parse(mut self) -> Result<Document, String> {
        let version = u32::from_le_bytes(self.bytes[23..27].try_into().unwrap());
        let mut offset = 27usize;
        let mut nodes = Vec::new();
        while let Some(node) = self.node(&mut offset, 0)? {
            nodes.push(node);
        }
        Ok(Document { version, nodes })
    }

    fn take<'b>(&'b self, offset: &mut usize, count: usize) -> Result<&'a [u8], String> {
        let end = offset.checked_add(count).ok_or("FBX offset overflow")?;
        let bytes = self
            .bytes
            .get(*offset..end)
            .ok_or("Truncated FBX document")?;
        *offset = end;
        Ok(bytes)
    }

    fn u32(&self, offset: &mut usize) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(offset, 4)?.try_into().unwrap(),
        ))
    }

    fn u64(&self, offset: &mut usize) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(offset, 8)?.try_into().unwrap(),
        ))
    }

    fn offset_field(&self, offset: &mut usize) -> Result<u64, String> {
        match self.wide {
            true => self.u64(offset),
            false => Ok(u64::from(self.u32(offset)?)),
        }
    }

    fn node(&mut self, offset: &mut usize, depth: usize) -> Result<Option<Node>, String> {
        if depth > self.limits.depth {
            return Err("FBX object hierarchy exceeds the depth limit".into());
        }
        let start = *offset;
        // A null record terminates a child list: 13 zero bytes before FBX 7500
        // and 25 after it. A top-level null record ends the document.
        let sentinel = if self.wide { 25 } else { 13 };
        let remaining = self.bytes.get(*offset..).unwrap_or_default();
        if remaining.len() < sentinel || remaining[..sentinel].iter().all(|&b| b == 0) {
            *offset = (*offset).saturating_add(sentinel);
            return Ok(None);
        }
        self.nodes += 1;
        if self.nodes > self.limits.nodes {
            return Err("FBX document exceeds the node limit".into());
        }
        let end = self.offset_field(offset)?;
        let property_count = self.offset_field(offset)?;
        let property_bytes = self.offset_field(offset)?;
        if end <= start as u64 || end > self.bytes.len() as u64 {
            return Err("FBX node has an invalid end offset".into());
        }
        if property_count > self.limits.nodes as u64 {
            return Err("FBX node declares too many properties".into());
        }
        let name_length = self.take(offset, 1)?[0] as usize;
        let name = String::from_utf8_lossy(self.take(offset, name_length)?).into_owned();
        let property_start = *offset;
        let mut props = Vec::with_capacity(property_count as usize);
        for _ in 0..property_count {
            props.push(self.property(offset)?);
        }
        let consumed = *offset - property_start;
        if consumed as u64 > property_bytes {
            return Err("FBX node property list is longer than declared".into());
        }
        let mut children = Vec::new();
        while (*offset as u64) < end {
            match self.node(offset, depth + 1)? {
                Some(child) => children.push(child),
                None => break,
            }
        }
        *offset = end as usize;
        Ok(Some(Node {
            name,
            props,
            children,
        }))
    }

    fn property(&self, offset: &mut usize) -> Result<Prop, String> {
        let kind = self.take(offset, 1)?[0];
        Ok(match kind {
            b'Y' => Prop::Int(i64::from(i16::from_le_bytes(
                self.take(offset, 2)?.try_into().unwrap(),
            ))),
            b'C' => Prop::Int(i64::from(self.take(offset, 1)?[0])),
            b'I' => Prop::Int(i64::from(i32::from_le_bytes(
                self.take(offset, 4)?.try_into().unwrap(),
            ))),
            b'F' => Prop::Float(f64::from(f32::from_le_bytes(
                self.take(offset, 4)?.try_into().unwrap(),
            ))),
            b'D' => Prop::Float(f64::from_le_bytes(
                self.take(offset, 8)?.try_into().unwrap(),
            )),
            b'L' => Prop::Int(i64::from_le_bytes(
                self.take(offset, 8)?.try_into().unwrap(),
            )),
            b'i' => Prop::Ints(self.int_array(offset, 4)?),
            b'l' => Prop::Longs(self.long_array(offset)?),
            b'f' => Prop::Floats(self.float_array(offset, 4)?),
            b'd' => Prop::Floats(self.float_array(offset, 8)?),
            b'b' => Prop::Ints(self.int_array(offset, 1)?),
            b'S' => {
                let length = self.u32(offset)? as usize;
                Prop::Str(String::from_utf8_lossy(self.take(offset, length)?).into_owned())
            }
            b'R' => {
                // Raw payloads (embedded media) are consumed but never used.
                let length = self.u32(offset)? as usize;
                Prop::Str(String::from_utf8_lossy(self.take(offset, length)?).into_owned())
            }
            other => {
                return Err(format!("Unsupported FBX property type 0x{other:02x}"));
            }
        })
    }

    fn array_header(&self, offset: &mut usize) -> Result<(usize, usize, usize), String> {
        let length = self.u32(offset)? as usize;
        let encoding = self.u32(offset)? as usize;
        let stored = self.u32(offset)? as usize;
        if length > self.limits.vertices.saturating_mul(8).max(1 << 20) {
            return Err("FBX array exceeds the import limit".into());
        }
        Ok((length, encoding, stored))
    }

    fn array_bytes(
        &self,
        offset: &mut usize,
        length: usize,
        encoding: usize,
        stored: usize,
        element: usize,
    ) -> Result<Vec<u8>, String> {
        let raw = self.take(offset, stored)?;
        let bytes = match encoding {
            0 => {
                if stored != length.saturating_mul(element) {
                    return Err("FBX raw array length does not match its declared count".into());
                }
                raw.to_vec()
            }
            1 => {
                use std::io::Read;
                let mut decoded = Vec::with_capacity(length.saturating_mul(element));
                flate2::read::ZlibDecoder::new(raw)
                    .take(length.saturating_mul(element) as u64 + 1)
                    .read_to_end(&mut decoded)
                    .map_err(|e| format!("Invalid compressed FBX array: {e}"))?;
                if decoded.len() != length.saturating_mul(element) {
                    return Err(
                        "FBX compressed array length does not match its declared count".into(),
                    );
                }
                decoded
            }
            _ => return Err("Unsupported FBX array encoding".into()),
        };
        Ok(bytes)
    }

    fn int_array(&self, offset: &mut usize, element: usize) -> Result<Vec<i32>, String> {
        let (length, encoding, stored) = self.array_header(offset)?;
        let bytes = self.array_bytes(offset, length, encoding, stored, element)?;
        Ok(bytes
            .chunks_exact(element)
            .map(|chunk| match element {
                1 => i32::from(chunk[0]),
                _ => i32::from_le_bytes(chunk.try_into().unwrap()),
            })
            .collect())
    }

    fn long_array(&self, offset: &mut usize) -> Result<Vec<i64>, String> {
        let (length, encoding, stored) = self.array_header(offset)?;
        let bytes = self.array_bytes(offset, length, encoding, stored, 8)?;
        Ok(bytes
            .as_chunks::<8>()
            .0
            .iter()
            .map(|chunk| i64::from_le_bytes(*chunk))
            .collect())
    }

    fn float_array(&self, offset: &mut usize, element: usize) -> Result<Vec<f64>, String> {
        let (length, encoding, stored) = self.array_header(offset)?;
        let bytes = self.array_bytes(offset, length, encoding, stored, element)?;
        Ok(match element {
            4 => bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f64::from(f32::from_le_bytes(*chunk)))
                .collect(),
            _ => bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|chunk| f64::from_le_bytes(*chunk))
                .collect(),
        })
    }
}

// ---------------------------------------------------------------------------
// ASCII FBX 7.x
// ---------------------------------------------------------------------------

struct Ascii<'a> {
    bytes: &'a [u8],
    position: usize,
    limits: &'a Limits,
    nodes: usize,
}

impl<'a> Ascii<'a> {
    fn new(bytes: &'a [u8], limits: &'a Limits) -> Result<Self, String> {
        if !bytes
            .windows(18)
            .any(|window| window == b"FBXHeaderExtension")
            && !bytes.windows(10).any(|window| window == b"FBXVersion")
        {
            return Err("Not an FBX document".into());
        }
        Ok(Self {
            bytes,
            position: if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                3
            } else {
                0
            },
            limits,
            nodes: 0,
        })
    }

    fn parse(mut self) -> Result<Document, String> {
        // The version sits in `FBXHeaderExtension/FBXVersion`; parsing the whole
        // document first keeps the reader grammar-driven rather than
        // header-order dependent.
        let nodes = self.nodes()?;
        let version = nodes
            .iter()
            .find(|node| node.name == "FBXHeaderExtension")
            .and_then(|header| header.child("FBXVersion"))
            .and_then(|version| version.prop(0))
            .and_then(Prop::as_i64)
            .ok_or("FBX document does not declare FBXVersion")?;
        Ok(Document {
            version: version.clamp(0, u32::MAX as i64) as u32,
            nodes,
        })
    }

    fn nodes(&mut self) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        while let Some(node) = self.node(0)? {
            nodes.push(node);
        }
        Ok(nodes)
    }

    fn node(&mut self, depth: usize) -> Result<Option<Node>, String> {
        if depth > self.limits.depth {
            return Err("FBX object hierarchy exceeds the depth limit".into());
        }
        self.nodes += 1;
        if self.nodes > self.limits.nodes {
            return Err("FBX document exceeds the node limit".into());
        }
        self.skip();
        match self.peek() {
            None | Some(b'}') => return Ok(None),
            _ => {}
        }
        let name = self.identifier()?;
        self.skip();
        self.expect(b':')?;
        let mut node = Node {
            name,
            props: Vec::new(),
            children: Vec::new(),
        };
        loop {
            self.skip();
            match self.peek() {
                None | Some(b'}') => break,
                Some(b'{') => {
                    self.position += 1;
                    while let Some(child) = self.node(depth + 1)? {
                        node.children.push(child);
                    }
                    self.skip();
                    self.expect(b'}')?;
                    break;
                }
                Some(b',') => self.position += 1,
                Some(b'*') => {
                    // Array length marker: the data follows in the `a:` child.
                    self.position += 1;
                    self.number()?;
                }
                Some(b'"') => node.props.push(Prop::Str(self.string()?)),
                Some(b'~') => {
                    self.position += 1;
                    let raw = self.number()?;
                    match raw {
                        Prop::Int(value) => node.props.push(Prop::Int(!value)),
                        _ => return Err("Invalid FBX polygon index marker".into()),
                    }
                }
                Some(b'-' | b'+' | b'.' | b'0'..=b'9') => node.props.push(self.number()?),
                Some(_) => {
                    // An identifier here either continues this node's values
                    // (a bare literal such as `T`) or starts the next node.
                    let save = self.position;
                    let identifier = self.identifier()?;
                    self.skip();
                    if self.peek() == Some(b':') {
                        self.position = save;
                        break;
                    }
                    node.props.push(Prop::Str(identifier));
                }
            }
        }
        Ok(Some(node))
    }

    fn skip(&mut self) {
        loop {
            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n' | b'\0')) {
                self.position += 1;
            }
            if self.peek() == Some(b';') {
                while !matches!(self.peek(), None | Some(b'\n')) {
                    self.position += 1;
                }
                continue;
            }
            break;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() != Some(byte) {
            return Err(format!(
                "Invalid FBX text at byte {}: expected {:?}",
                self.position, byte as char
            ));
        }
        self.position += 1;
        Ok(())
    }

    fn identifier(&mut self) -> Result<String, String> {
        let start = self.position;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        ) {
            self.position += 1;
        }
        if self.position == start {
            return Err(format!(
                "Invalid FBX text at byte {}: expected an identifier",
                self.position
            ));
        }
        Ok(String::from_utf8_lossy(&self.bytes[start..self.position]).into_owned())
    }

    fn number(&mut self) -> Result<Prop, String> {
        let start = self.position;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E')
        ) {
            self.position += 1;
        }
        let text =
            std::str::from_utf8(&self.bytes[start..self.position]).map_err(|e| e.to_string())?;
        if text.is_empty() {
            return Err(format!(
                "Invalid FBX text at byte {start}: expected a number"
            ));
        }
        if text.contains(['.', 'e', 'E']) {
            text.parse::<f64>()
                .map(Prop::Float)
                .map_err(|_| format!("Invalid FBX number {text:?}"))
        } else {
            text.parse::<i64>()
                .map(Prop::Int)
                .map_err(|_| format!("Invalid FBX integer {text:?}"))
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut value = Vec::new();
        loop {
            match self.peek() {
                None => return Err("Unterminated FBX string".into()),
                Some(b'"') => {
                    self.position += 1;
                    break;
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escaped = self.peek().ok_or("Unterminated FBX escape")?;
                    value.push(match escaped {
                        b'n' => b'\n',
                        b't' => b'\t',
                        other => other,
                    });
                    self.position += 1;
                }
                Some(byte) => {
                    value.push(byte);
                    self.position += 1;
                }
            }
        }
        Ok(String::from_utf8_lossy(&value).into_owned())
    }
}

// ---------------------------------------------------------------------------
// Scene assembly
// ---------------------------------------------------------------------------

struct Model {
    parent: Option<i64>,
    transform: DMat4,
    inherit_type: i64,
    name: String,
}

struct Geometry {
    id: i64,
    name: String,
    vertices: Vec<DVec3>,
    polygon_indices: Vec<i64>,
    model: Option<i64>,
}

fn flatten(document: &Document, limits: Limits) -> Result<ImportedAvatar, String> {
    let mut warnings = Vec::new();
    let mut models = std::collections::HashMap::new();
    let mut geometry = Vec::new();
    let mut connections = Vec::new();
    let mut skinned = false;
    let mut animated = false;
    let mut non_mesh_objects = 0usize;

    if let Some(objects) = document.top("Objects") {
        for node in &objects.children {
            match node.name.as_str() {
                "Model" => {
                    let Some(id) = node.prop(0).and_then(Prop::as_i64) else {
                        continue;
                    };
                    let name = node.string(1).unwrap_or_default().to_owned();
                    let properties = node.child("Properties70");
                    let translation =
                        read_vector(properties, "Lcl Translation").unwrap_or(DVec3::ZERO);
                    let rotation = read_vector(properties, "Lcl Rotation").unwrap_or(DVec3::ZERO);
                    let scale = read_vector(properties, "Lcl Scaling").unwrap_or(DVec3::ONE);
                    let pre_rotation =
                        read_vector(properties, "PreRotation").unwrap_or(DVec3::ZERO);
                    let rotation_active = read_number(properties, "RotationActive")
                        .map(|value| value != 0.0)
                        .unwrap_or(true);
                    let rotation_order = read_number(properties, "RotationOrder")
                        .map(|value| value.round() as i64)
                        .unwrap_or(0);
                    let inherit_type = read_number(properties, "InheritType")
                        .map(|value| value.round() as i64)
                        .unwrap_or(0);
                    let geometric_translation =
                        read_vector(properties, "GeometricTranslation").unwrap_or(DVec3::ZERO);
                    let geometric_rotation =
                        read_vector(properties, "GeometricRotation").unwrap_or(DVec3::ZERO);
                    let geometric_scale =
                        read_vector(properties, "GeometricScaling").unwrap_or(DVec3::ONE);
                    for pivot in [
                        "RotationOffset",
                        "RotationPivot",
                        "PostRotation",
                        "ScalingOffset",
                        "ScalingPivot",
                    ] {
                        if let Some(value) = read_vector(properties, pivot)
                            && value.length_squared() > 0.0
                        {
                            warnings.push(format!(
                                "Model {name:?} authors a non-default {pivot}; pivots are not applied and the mesh may be misplaced."
                            ));
                        }
                    }
                    let pre_rotation = if rotation_active {
                        pre_rotation
                    } else {
                        DVec3::ZERO
                    };
                    let rotation_order = if rotation_active { rotation_order } else { 0 };
                    let transform = model_transform(
                        translation,
                        rotation,
                        pre_rotation,
                        scale,
                        rotation_order,
                    )?;
                    let transform = transform
                        * model_transform(
                            geometric_translation,
                            geometric_rotation,
                            DVec3::ZERO,
                            geometric_scale,
                            0,
                        )?;
                    models.insert(
                        id,
                        Model {
                            parent: None,
                            transform,
                            inherit_type,
                            name,
                        },
                    );
                }
                "Geometry" => {
                    let Some(id) = node.prop(0).and_then(Prop::as_i64) else {
                        continue;
                    };
                    let name = node.string(1).unwrap_or_default().to_owned();
                    if node.string(2) != Some("Mesh") {
                        non_mesh_objects += 1;
                        continue;
                    }
                    let Some(vertices) = node.child("Vertices").and_then(Node::numbers) else {
                        warnings.push(format!(
                            "Geometry {name:?} has no Vertices array and was skipped."
                        ));
                        continue;
                    };
                    let Some(polygon_indices) =
                        node.child("PolygonVertexIndex").and_then(Node::integers)
                    else {
                        warnings.push(format!(
                            "Geometry {name:?} has no PolygonVertexIndex array and was skipped."
                        ));
                        continue;
                    };
                    if vertices.len() % 3 != 0 {
                        return Err(format!(
                            "Geometry {name:?} has {} coordinates, which is not a whole number of vertices",
                            vertices.len()
                        ));
                    }
                    let vertices = vertices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|chunk| DVec3::new(chunk[0], chunk[1], chunk[2]))
                        .collect();
                    geometry.push(Geometry {
                        id,
                        name,
                        vertices,
                        polygon_indices,
                        model: None,
                    });
                }
                "Deformer" => {
                    if node.string(2) == Some("Skin") {
                        skinned = true;
                    }
                }
                "AnimationCurve" | "AnimationCurveNode" | "AnimationStack" => animated = true,
                _ => {}
            }
        }
    }
    if let Some(node) = document.top("Connections") {
        for connection in node.children_named("C") {
            let Some(kind) = connection.string(0) else {
                continue;
            };
            if kind != "OO" {
                continue;
            }
            let (Some(child), Some(parent)) = (
                connection.prop(1).and_then(Prop::as_i64),
                connection.prop(2).and_then(Prop::as_i64),
            ) else {
                continue;
            };
            connections.push((child, parent));
        }
    }
    if geometry.is_empty() {
        return Err("The FBX document contains no supported static mesh geometry".into());
    }
    for (child, parent) in &connections {
        if let Some(model) = models.get_mut(child) {
            model.parent = (*parent != 0).then_some(*parent);
        }
    }
    for (child, parent) in &connections {
        if let Some(slot) = geometry.iter_mut().find(|geometry| geometry.id == *child) {
            slot.model = (*parent != 0).then_some(*parent);
        }
    }

    let settings = Settings::read(document);
    if geometry
        .iter()
        .any(|geometry| geometry.model.is_some_and(|id| !models.contains_key(&id)))
    {
        warnings.push(
            "Some geometry is connected to a missing model; it is imported at the identity transform."
                .into(),
        );
    }
    if skinned {
        warnings.push(
            "Skin clusters and bind poses are not applied: the mesh is imported in its stored base pose."
                .into(),
        );
    }
    if animated {
        warnings
            .push("Animation curves are ignored; only the static base scene is imported.".into());
    }
    if non_mesh_objects > 0 {
        warnings.push(format!(
            "{non_mesh_objects} non-mesh geometry object(s) (shape/blend shapes) were skipped."
        ));
    }
    if models.values().any(|model| model.inherit_type != 0) {
        warnings.push(
            "A model authors a non-default InheritType; the hierarchy is composed with standard parent transforms, so scaled parent chains may be placed differently than in the authoring tool."
                .into(),
        );
    }
    if settings.axis_converted {
        warnings.push(
            "The FBX global axis settings differ from Y-up; geometry is rotated into the editor's right-handed Y-up frame."
                .into(),
        );
    }
    if settings.fallback_used {
        warnings.push(
            "The FBX global settings do not describe a valid unit/axis basis; the default centimetres and Y-up frame were applied and the mesh may be misplaced."
                .into(),
        );
    }

    let mut cache: std::collections::HashMap<i64, DMat4> = std::collections::HashMap::new();
    let mut parts: Vec<(String, SurfaceMesh)> = Vec::new();
    let mut dropped = 0usize;
    for geometry in &geometry {
        if geometry.vertices.len() > limits.vertices {
            return Err("Avatar geometry exceeds the import limit".into());
        }
        let world = match geometry.model {
            Some(model) if models.contains_key(&model) => {
                world_transform(model, &models, &mut cache)?
            }
            _ => DMat4::IDENTITY,
        };
        let mut vertices = Vec::with_capacity(geometry.vertices.len());
        for &vertex in &geometry.vertices {
            let p = world.transform_point3(vertex) * settings.scale;
            let p = settings.basis * p;
            if !p.is_finite() {
                return Err(format!(
                    "Geometry {:?} has non-finite coordinates",
                    geometry.name
                ));
            }
            vertices.push(p);
        }
        let mut triangles: Vec<[usize; 3]> = Vec::new();
        let mut polygon_start = 0usize;
        while polygon_start < geometry.polygon_indices.len() {
            let mut end = polygon_start;
            let mut ring = Vec::new();
            loop {
                let Some(&raw) = geometry.polygon_indices.get(end) else {
                    return Err(format!(
                        "Geometry {:?} ends inside a polygon",
                        geometry.name
                    ));
                };
                end += 1;
                if raw < 0 {
                    let index = -raw - 1;
                    let index = usize::try_from(index)
                        .ok()
                        .filter(|&index| index < geometry.vertices.len())
                        .ok_or_else(|| {
                            format!(
                                "Geometry {:?} has an invalid polygon index {raw}",
                                geometry.name
                            )
                        })?;
                    ring.push(index);
                    break;
                }
                let index = usize::try_from(raw)
                    .ok()
                    .filter(|&index| index < geometry.vertices.len())
                    .ok_or_else(|| {
                        format!(
                            "Geometry {:?} has an invalid polygon index {raw}",
                            geometry.name
                        )
                    })?;
                ring.push(index);
            }
            polygon_start = end;
            if ring.len() < 3 {
                warnings.push(format!(
                    "Geometry {:?} has a polygon with {} vertices and was skipped.",
                    geometry.name,
                    ring.len()
                ));
                continue;
            }
            for k in 1..ring.len() - 1 {
                let triangle = [ring[0], ring[k], ring[k + 1]];
                let [a, b, c] = triangle.map(|index| vertices[index]);
                if (b - a).cross(c - a).length_squared() < 1e-24 {
                    dropped += 1;
                } else {
                    triangles.push(triangle);
                }
            }
            if triangles.len() > limits.triangles {
                return Err("Avatar geometry exceeds the import limit".into());
            }
        }
        if triangles.is_empty() {
            warnings.push(format!(
                "Geometry {:?} has no non-degenerate triangles and was skipped.",
                geometry.name
            ));
            continue;
        }
        if vertices.len() > limits.vertices || triangles.len() > limits.triangles {
            return Err("Avatar geometry exceeds the import limit".into());
        }
        let model_name = geometry
            .model
            .and_then(|id| models.get(&id))
            .map(|model| model.name.clone())
            .unwrap_or_else(|| geometry.name.clone());
        parts.push((
            short_fbx_name(&model_name),
            SurfaceMesh {
                vertices,
                triangles,
            },
        ));
    }
    if parts.is_empty() {
        return Err("The FBX document contains no supported static mesh geometry".into());
    }
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for (_, part) in &parts {
        let base = vertices.len();
        vertices.extend_from_slice(&part.vertices);
        triangles.extend(
            part.triangles
                .iter()
                .map(|triangle| triangle.map(|index| base + index)),
        );
    }
    if dropped > 0 {
        warnings.push(format!(
            "{dropped} zero-area triangle(s) produced by triangulation were dropped; the rest of the mesh is unchanged."
        ));
    }
    let mesh = SurfaceMesh {
        vertices,
        triangles,
    };
    mesh.validate()?;
    warnings.push(
        "FBX import keeps static surface positions only: UVs, normals, materials, textures, blend shapes, skinning, animation, cameras and lights are not retained."
            .into(),
    );
    Ok(ImportedAvatar {
        mesh,
        parts,
        warnings,
    })
}

/// Strip the FBX `Model::`/`Geometry::` prefix from a display name.
fn short_fbx_name(name: &str) -> String {
    // ASCII documents use `Model::Name`; binary documents separate the name
    // from its class with `\0\u{1}`.
    let name = name.rsplit("::").next().unwrap_or(name);
    let name = name.split('\0').next().unwrap_or(name).trim();
    if name.is_empty() {
        "mesh".into()
    } else {
        name.to_owned()
    }
}

struct Settings {
    scale: f64,
    basis: DMat3,
    axis_converted: bool,
    /// The authored unit or axes were unusable and a default was substituted.
    fallback_used: bool,
}

impl Settings {
    fn read(document: &Document) -> Self {
        let properties = document
            .top("GlobalSettings")
            .and_then(|settings| settings.child("Properties70"));
        let unit_scale = read_number(properties, "UnitScaleFactor").unwrap_or(1.0);
        let axis = |name: &str, default: i64| {
            read_number(properties, name)
                .map(|value| value.round() as i64)
                .unwrap_or(default)
        };
        let sign = |name: &str| read_number(properties, name).unwrap_or(1.0).signum();
        let up = axis("UpAxis", 1);
        let front = axis("FrontAxis", 2);
        let coord = axis("CoordAxis", 0);
        let up_sign = sign("UpAxisSign");
        let front_sign = sign("FrontAxisSign");
        let coord_sign = sign("CoordAxisSign");
        let basis = fbx_basis(up, up_sign, front, front_sign, coord, coord_sign);
        let scale = unit_scale / CENTIMETRES_PER_METRE;
        let unit_valid = scale.is_finite() && scale > 0.0;
        let invalid_axes = basis.is_none() && (up, front, coord) != (1, 2, 0);
        let basis = basis.unwrap_or(DMat3::IDENTITY);
        Self {
            // A non-positive or non-finite unit would turn metres into nonsense;
            // fall back to the FBX default (centimetres) and report it.
            scale: if unit_valid { scale } else { 0.01 },
            basis,
            axis_converted: basis != DMat3::IDENTITY,
            fallback_used: !unit_valid || invalid_axes,
        }
    }
}

/// Basis mapping the authored FBX axes onto the editor's +Y up, +Z front frame.
fn fbx_basis(
    up: i64,
    up_sign: f64,
    front: i64,
    front_sign: f64,
    coord: i64,
    coord_sign: f64,
) -> Option<DMat3> {
    let unit = |axis: i64| -> Option<DVec3> {
        match axis {
            0 => Some(DVec3::X),
            1 => Some(DVec3::Y),
            2 => Some(DVec3::Z),
            _ => None,
        }
    };
    if up == front || up == coord || front == coord {
        return None;
    }
    let x = unit(coord)? * coord_sign;
    let y = unit(up)? * up_sign;
    let z = unit(front)? * front_sign;
    // The basis rows are the new Y/X/Z axes expressed in the authored frame,
    // so a point is converted by `basis * point`.
    let basis = DMat3::from_cols(x, y, z).transpose();
    let determinant = basis.determinant();
    if !determinant.is_finite() || (determinant - 1.0).abs() > 1e-9 {
        return None;
    }
    Some(basis)
}

fn read_number(properties: Option<&Node>, name: &str) -> Option<f64> {
    properties?
        .children_named("P")
        .find_map(|property| match property.p_values() {
            Some((property_name, values)) if property_name == name => {
                values.first().and_then(Prop::as_f64)
            }
            _ => None,
        })
}

fn read_vector(properties: Option<&Node>, name: &str) -> Option<DVec3> {
    properties?.children_named("P").find_map(|property| {
        let (property_name, values) = property.p_values()?;
        if property_name != name || values.len() < 3 {
            return None;
        }
        Some(DVec3::new(
            values[0].as_f64()?,
            values[1].as_f64()?,
            values[2].as_f64()?,
        ))
    })
}

/// FBX local model transform as a column-vector matrix.
///
/// FBX documents compose `T * Rpre * R * S` with row vectors, so the
/// column-vector equivalent applies the translation first and the scale last.
/// Euler rotations follow FBX's order tokens (0 = XYZ ... 5 = ZYX), with the
/// first named axis applied first.
fn model_transform(
    translation: DVec3,
    rotation: DVec3,
    pre_rotation: DVec3,
    scale: DVec3,
    order: i64,
) -> Result<DMat4, String> {
    let axes = match order {
        0 => [0, 1, 2],
        1 => [0, 2, 1],
        2 => [1, 2, 0],
        3 => [1, 0, 2],
        4 => [2, 0, 1],
        5 => [2, 1, 0],
        other => return Err(format!("Unsupported FBX rotation order {other}")),
    };
    let rotation = euler(rotation, axes);
    let pre_rotation = euler(pre_rotation, [0, 1, 2]);
    if !scale.is_finite() || !translation.is_finite() {
        return Err("FBX model transform is not finite".into());
    }
    Ok(DMat4::from_scale(scale) * rotation * pre_rotation * DMat4::from_translation(translation))
}

fn euler(degrees: DVec3, axes: [usize; 3]) -> DMat4 {
    let radians = degrees.map(f64::to_radians);
    let mut matrix = DMat4::IDENTITY;
    // Column-vector equivalent of FBX's row-vector `Rx * Ry * Rz`.
    for axis in axes.into_iter().rev() {
        matrix *= axis_rotation(axis, radians[axis]);
    }
    matrix
}

fn axis_rotation(axis: usize, radians: f64) -> DMat4 {
    match axis {
        0 => DMat4::from_rotation_x(radians),
        1 => DMat4::from_rotation_y(radians),
        _ => DMat4::from_rotation_z(radians),
    }
}

fn world_transform(
    id: i64,
    models: &std::collections::HashMap<i64, Model>,
    cache: &mut std::collections::HashMap<i64, DMat4>,
) -> Result<DMat4, String> {
    if let Some(world) = cache.get(&id) {
        return Ok(*world);
    }
    let mut chain = Vec::new();
    let mut current = Some(id);
    let mut visited = std::collections::HashSet::new();
    while let Some(model_id) = current {
        if !visited.insert(model_id) {
            return Err("FBX model hierarchy contains a cycle".into());
        }
        if chain.len() > 256 {
            return Err("FBX model hierarchy exceeds the depth limit".into());
        }
        if cache.contains_key(&model_id) {
            break;
        }
        let Some(model) = models.get(&model_id) else {
            return Err(format!("FBX model {model_id} is missing"));
        };
        chain.push(model_id);
        current = model.parent;
    }
    let mut world = cache
        .get(&current.unwrap_or(0))
        .copied()
        .unwrap_or(DMat4::IDENTITY);
    for model_id in chain.into_iter().rev() {
        let model = models
            .get(&model_id)
            .ok_or_else(|| format!("FBX model {model_id} is missing"))?;
        world = model.transform * world;
        if !world.is_finite() {
            return Err(format!(
                "FBX model {:?} has a non-finite transform",
                model.name
            ));
        }
        cache.insert(model_id, world);
    }
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_array_round_trip_is_zlib_and_length_checked() {
        // Written by the test, not by an external tool: a raw f64 array.
        let values = [1.0f64, 2.0, 3.0];
        let mut bytes = Vec::new();
        bytes.push(b'd');
        bytes.extend((values.len() as u32).to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(((values.len() * 8) as u32).to_le_bytes());
        for value in values {
            bytes.extend(value.to_le_bytes());
        }
        let limits = Limits::default();
        let reader = Binary {
            bytes: &bytes,
            limits: &limits,
            nodes: 0,
            wide: true,
        };
        let mut offset = 0;
        let Prop::Floats(decoded) = reader.property(&mut offset).unwrap() else {
            panic!("expected floats");
        };
        assert_eq!(decoded, values);
        assert_eq!(offset, bytes.len());
    }

    #[test]
    fn axis_basis_maps_z_up_to_y_up() {
        // Blender's Z-up FBX global settings.
        let basis = fbx_basis(2, 1.0, 1, -1.0, 0, 1.0).unwrap();
        assert!((basis * DVec3::Z - DVec3::Y).length() < 1e-12);
        assert!((basis * DVec3::Y + DVec3::Z).length() < 1e-12);
        assert_eq!(fbx_basis(1, 1.0, 1, 1.0, 0, 1.0), None);
    }

    #[test]
    fn model_transform_places_translation_before_scale() {
        let transform = model_transform(
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::splat(2.0),
            0,
        )
        .unwrap();
        assert_eq!(
            transform.transform_point3(DVec3::ZERO),
            DVec3::new(2.0, 0.0, 0.0)
        );
    }
}
