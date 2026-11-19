//! Lossless serde-JSON <-> USDA mapping for ClothCraft documents.
//!
//! The USD file is not a render snapshot: it is the editable document. Every
//! serde field becomes a USD attribute, a child prim, or an item in an array
//! container, and the reader rebuilds the exact same JSON tree. Part boundaries
//! are additionally authored as standard `BasisCurves` prims so other tools see
//! the pattern geometry; those views are derived on save and skipped on load.
//!
//! Conventions:
//! - layer metadata: `metersPerUnit = 1`, `upAxis = "Y"`, `defaultPrim = "World"`,
//!   and a `clothcraft:document` dictionary carrying the format version;
//! - objects become `Xform` prims, scalar fields become attributes, arrays of
//!   scalars become typed arrays, arrays of objects become child prims;
//! - every child prim carries `clothcraft:kind` (`object`, `array`, or `view`)
//!   and `clothcraft:key` (the original JSON field or map key);
//! - boundary views under `/World/Parts/<hex id>/Boundary` are `BasisCurves`
//!   with `basis = "bezier"`, `type = "cubic"`, `wrap = "nonperiodic"` and one
//!   4-point curve per named edge. A `double3[] clothcraft:controlPoints`
//!   attribute stores the same controls exactly. Load ignores these views.

use std::collections::{BTreeSet, HashMap};

use openusd::gf::{Vec2d, Vec3d, Vec3f};
use openusd::sdf::{ChildrenKey, Data, FieldKey, Path, SpecType, Specifier, Value};
use openusd::usda::TextWriter;
use serde_json::{Map, Number, Value as Json};

use crate::fitting::FittingProject;
use crate::garment::GarmentTemplate;

const DOCUMENT_KEY: &str = "clothcraft:document";
const DOCUMENT_VERSION: i32 = 1;
const KIND_ATTR: &str = "clothcraft:kind";
const KEY_ATTR: &str = "clothcraft:key";
const NULLS_ATTR: &str = "clothcraft:nulls";

/// One derived `BasisCurves` prim written next to the document data.
struct CurveView {
    path: String,
    points: Vec<[f64; 3]>,
    counts: Vec<i32>,
    ids: Vec<String>,
}

/// Serialize a garment template as a complete USDA document.
pub fn template_to_usda(template: &GarmentTemplate) -> Result<String, String> {
    let json = serde_json::to_value(template)
        .map_err(|e| format!("Cannot encode template as JSON values: {e}"))?;
    to_usda(&json, &template_views(template))
}

/// Read a garment template from a USDA document produced by [`template_to_usda`].
pub fn template_from_usda(text: &str) -> Result<GarmentTemplate, String> {
    let json = from_usda(text)?;
    serde_json::from_value(json).map_err(|e| format!("Cannot decode template document: {e}"))
}

/// Serialize a fitting project (template, session settings and cached drape).
pub fn project_to_usda(project: &FittingProject) -> Result<String, String> {
    let json = serde_json::to_value(project)
        .map_err(|e| format!("Cannot encode project as JSON values: {e}"))?;
    to_usda(&json, &template_views(&project.template))
}

/// Read a fitting project from a USDA document produced by [`project_to_usda`].
pub fn project_from_usda(text: &str) -> Result<FittingProject, String> {
    let json = from_usda(text)?;
    serde_json::from_value(json).map_err(|e| format!("Cannot decode project document: {e}"))
}

fn to_usda(json: &Json, views: &[CurveView]) -> Result<String, String> {
    let Json::Object(root) = json else {
        return Err("Document root must be an object".into());
    };
    let mut data = Data::new();
    {
        let spec = data.create_spec(Path::abs_root(), SpecType::PseudoRoot);
        spec.add(
            ChildrenKey::PrimChildren,
            Value::TokenVec(vec!["World".into()]),
        );
        spec.add(FieldKey::DefaultPrim, Value::Token("World".into()));
        spec.add("metersPerUnit", Value::Double(1.0));
        spec.add("upAxis", Value::Token("Y".into()));
        let mut document = HashMap::new();
        document.insert("version".to_string(), Value::Int(DOCUMENT_VERSION));
        document.insert("kind".to_string(), Value::String("clothcraft".into()));
        let mut layer_data = HashMap::new();
        layer_data.insert(DOCUMENT_KEY.to_string(), Value::Dictionary(document));
        spec.add("customLayerData", Value::Dictionary(layer_data));
    }
    data.create_spec(sdf_path("/World")?, SpecType::Prim);
    write_body(&mut data, "/World", root, "object", None)?;
    write_views(&mut data, views)?;
    TextWriter::write_to_string(&data).map_err(|e| format!("Cannot write USDA: {e}"))
}

fn from_usda(text: &str) -> Result<Json, String> {
    let specs = openusd::usda::parser::Parser::new(text)
        .parse()
        .map_err(|e| format!("Cannot parse USDA: {e}"))?;
    let data = Data::from_specs(specs);
    let root = data
        .spec(&Path::abs_root())
        .ok_or("Document has no layer metadata")?;
    let units = double_value(root.get("metersPerUnit"));
    match units {
        Some(1.0) => {}
        Some(units) => {
            return Err(format!(
                "Document is authored in {units} metres per unit; expected 1"
            ));
        }
        None => return Err("Document must declare metersPerUnit = 1".into()),
    }
    match token_value(root.get("upAxis")) {
        Some("Y") => {}
        Some(axis) => {
            return Err(format!("Document up axis is {axis}; expected Y"));
        }
        None => return Err("Document must declare upAxis = \"Y\"".into()),
    }
    let Some(Value::Dictionary(layer_data)) = root.get("customLayerData") else {
        return Err("Document is missing customLayerData".into());
    };
    let Some(Value::Dictionary(document)) = layer_data.get(DOCUMENT_KEY) else {
        return Err(format!("Document is missing the {DOCUMENT_KEY} dictionary"));
    };
    match document.get("version") {
        Some(Value::Int(version)) if *version == DOCUMENT_VERSION => {}
        Some(Value::Int(version)) => {
            return Err(format!(
                "Unsupported ClothCraft document version {version}; expected {DOCUMENT_VERSION}"
            ));
        }
        _ => return Err("Document is missing a document version".into()),
    }
    read_body(&data, "/World")
}

fn double_value(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Double(value)) => Some(*value),
        _ => None,
    }
}

fn token_value(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::Token(token)) => Some(token.as_str()),
        _ => None,
    }
}

fn template_views(template: &GarmentTemplate) -> Vec<CurveView> {
    let mut views = Vec::new();
    for panel in &template.panels {
        let Some(polygon) = &panel.polygon else {
            continue;
        };
        let count = polygon.points.len();
        if count < 3 || polygon.edges.len() != count {
            continue;
        }
        let mut points = Vec::with_capacity(count * 4);
        let mut counts = Vec::with_capacity(count);
        let mut ids = Vec::with_capacity(count);
        for i in 0..count {
            let start = polygon.points[i];
            let end = polygon.points[(i + 1) % count];
            let [c1, c2] = polygon
                .curves
                .get(&polygon.edges[i])
                .copied()
                .unwrap_or([start, end]);
            points.extend([
                [start.x, start.y, 0.0],
                [c1.x, c1.y, 0.0],
                [c2.x, c2.y, 0.0],
                [end.x, end.y, 0.0],
            ]);
            counts.push(4);
            ids.push(polygon.edges[i].clone());
        }
        views.push(CurveView {
            path: format!("/World/Parts/{}/Boundary", prim_id(&panel.id)),
            points,
            counts,
            ids,
        });
    }
    views
}

fn prim_id(value: &str) -> String {
    let mut out = String::from("p_");
    for byte in value.bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

fn write_body(
    data: &mut Data,
    path: &str,
    object: &Map<String, Json>,
    kind: &str,
    key: Option<&str>,
) -> Result<(), String> {
    let prim_path = sdf_path(path)?;
    if data.spec(&prim_path).is_none() {
        data.create_spec(prim_path, SpecType::Prim);
    }
    let mut used = BTreeSet::new();
    let mut property_children = Vec::new();
    let mut prim_children = Vec::new();
    let mut null_fields = Vec::new();

    for (field, value) in object {
        match value {
            Json::Null => null_fields.push(field.clone()),
            Json::Object(child) => {
                let name = unique_name(container_name(field), &mut used);
                write_body(
                    data,
                    &format!("{path}/{name}"),
                    child,
                    "object",
                    Some(field),
                )?;
                prim_children.push(name);
            }
            Json::Array(items) if is_object_array(items) => {
                let name = unique_name(container_name(field), &mut used);
                write_array(data, &format!("{path}/{name}"), items, field)?;
                prim_children.push(name);
            }
            _ => {
                let (type_name, value) = encode_attribute(value)
                    .ok_or_else(|| format!("Unsupported value shape for field {field}"))?;
                add_attribute(data, path, field, type_name, value)?;
                property_children.push(field.clone());
            }
        }
    }

    if !null_fields.is_empty() {
        add_attribute(
            data,
            path,
            NULLS_ATTR,
            "string[]",
            Value::StringVec(null_fields),
        )?;
        property_children.push(NULLS_ATTR.to_string());
    }

    add_attribute(
        data,
        path,
        KIND_ATTR,
        "token",
        Value::Token(kind.to_string().into()),
    )?;
    property_children.push(KIND_ATTR.to_string());
    if let Some(key) = key {
        add_attribute(
            data,
            path,
            KEY_ATTR,
            "string",
            Value::String(key.to_string()),
        )?;
        property_children.push(KEY_ATTR.to_string());
    }

    let spec = data
        .spec_mut(&sdf_path(path)?)
        .ok_or_else(|| format!("Missing prim spec {path}"))?;
    spec.add(FieldKey::Specifier, Value::Specifier(Specifier::Def));
    spec.add(FieldKey::TypeName, Value::Token("Xform".into()));
    spec.add(
        ChildrenKey::PropertyChildren,
        Value::TokenVec(
            property_children
                .into_iter()
                .map(|name| name.into())
                .collect(),
        ),
    );
    spec.add(
        ChildrenKey::PrimChildren,
        Value::TokenVec(prim_children.into_iter().map(|name| name.into()).collect()),
    );
    Ok(())
}

fn write_array(data: &mut Data, path: &str, items: &[Json], field: &str) -> Result<(), String> {
    let prim_path = sdf_path(path)?;
    if data.spec(&prim_path).is_none() {
        data.create_spec(prim_path, SpecType::Prim);
    }
    let mut used = BTreeSet::new();
    let mut prim_children = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Json::Object(object) = item else {
            return Err(format!("Array {field} mixes objects with other values"));
        };
        let base = object
            .get("id")
            .and_then(Json::as_str)
            .map(prim_id)
            .unwrap_or_else(|| format!("i{index}"));
        let name = unique_name(&base, &mut used);
        write_body(
            data,
            &format!("{path}/{name}"),
            object,
            "object",
            Some(&base),
        )?;
        prim_children.push(name);
    }
    add_attribute(data, path, KIND_ATTR, "token", Value::Token("array".into()))?;
    add_attribute(
        data,
        path,
        KEY_ATTR,
        "string",
        Value::String(field.to_string()),
    )?;
    let spec = data
        .spec_mut(&sdf_path(path)?)
        .ok_or_else(|| format!("Missing array prim {path}"))?;
    spec.add(FieldKey::Specifier, Value::Specifier(Specifier::Def));
    spec.add(FieldKey::TypeName, Value::Token("Xform".into()));
    spec.add(
        ChildrenKey::PropertyChildren,
        Value::TokenVec(vec![KIND_ATTR.into(), KEY_ATTR.into()]),
    );
    spec.add(
        ChildrenKey::PrimChildren,
        Value::TokenVec(prim_children.into_iter().map(|name| name.into()).collect()),
    );
    Ok(())
}

fn add_attribute(
    data: &mut Data,
    prim_path: &str,
    name: &str,
    type_name: &str,
    value: Value,
) -> Result<(), String> {
    let path = sdf_path(&format!("{prim_path}.{name}"))?;
    if data.spec(&path).is_some() {
        return Err(format!("Duplicate attribute {path}"));
    }
    let spec = data.create_spec(path, SpecType::Attribute);
    spec.add(FieldKey::TypeName, Value::Token(type_name.into()));
    if name.starts_with("clothcraft:") {
        spec.add("custom", Value::Bool(true));
    }
    spec.add(FieldKey::Default, value);
    Ok(())
}

fn write_views(data: &mut Data, views: &[CurveView]) -> Result<(), String> {
    for view in views {
        let (panel_path, _) = view
            .path
            .rsplit_once('/')
            .ok_or_else(|| format!("Invalid view path {}", view.path))?;
        ensure_prim(data, "/World/Parts")?;
        ensure_prim(data, panel_path)?;
        ensure_prim(data, &view.path)?;
        {
            let spec = data
                .spec_mut(&sdf_path(&view.path)?)
                .ok_or_else(|| format!("Missing curve view {}", view.path))?;
            spec.add(FieldKey::TypeName, Value::Token("BasisCurves".into()));
        }
        add_attribute(
            data,
            &view.path,
            "points",
            "point3f[]",
            Value::Vec3fVec(
                view.points
                    .iter()
                    .map(|[x, y, z]| Vec3f {
                        x: *x as f32,
                        y: *y as f32,
                        z: *z as f32,
                    })
                    .collect(),
            ),
        )?;
        add_attribute(
            data,
            &view.path,
            "curveVertexCounts",
            "int[]",
            Value::IntVec(view.counts.clone()),
        )?;
        add_attribute(
            data,
            &view.path,
            "basis",
            "token",
            Value::Token("bezier".into()),
        )?;
        add_attribute(
            data,
            &view.path,
            "type",
            "token",
            Value::Token("cubic".into()),
        )?;
        add_attribute(
            data,
            &view.path,
            "wrap",
            "token",
            Value::Token("nonperiodic".into()),
        )?;
        add_attribute(
            data,
            &view.path,
            "clothcraft:controlPoints",
            "double3[]",
            Value::Vec3dVec(
                view.points
                    .iter()
                    .map(|[x, y, z]| Vec3d {
                        x: *x,
                        y: *y,
                        z: *z,
                    })
                    .collect(),
            ),
        )?;
        add_attribute(
            data,
            &view.path,
            "clothcraft:boundaryIds",
            "string[]",
            Value::StringVec(view.ids.clone()),
        )?;
        set_property_children(
            data,
            &view.path,
            &[
                KIND_ATTR,
                "points",
                "curveVertexCounts",
                "basis",
                "type",
                "wrap",
                "clothcraft:controlPoints",
                "clothcraft:boundaryIds",
            ],
        )?;
    }
    Ok(())
}

fn ensure_prim(data: &mut Data, path: &str) -> Result<(), String> {
    let prim_path = sdf_path(path)?;
    if data.spec(&prim_path).is_some() {
        return Ok(());
    }
    data.create_spec(prim_path, SpecType::Prim);
    let name = path
        .rsplit('/')
        .next()
        .ok_or_else(|| format!("Invalid prim path {path}"))?;
    let parent = path
        .rsplit_once('/')
        .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
        .ok_or_else(|| format!("Prim {path} has no parent"))?;
    if parent != "/" {
        ensure_prim(data, parent)?;
    }
    add_attribute(data, path, KIND_ATTR, "token", Value::Token("view".into()))?;
    set_property_children(data, path, &[KIND_ATTR])?;
    {
        let spec = data
            .spec_mut(&sdf_path(path)?)
            .ok_or_else(|| format!("Missing prim {path}"))?;
        spec.add(FieldKey::Specifier, Value::Specifier(Specifier::Def));
        spec.add(FieldKey::TypeName, Value::Token("Xform".into()));
    }
    let parent_spec = data
        .spec_mut(&sdf_path(parent)?)
        .ok_or_else(|| format!("Missing parent prim {parent}"))?;
    let mut children = child_names(parent_spec, "primChildren");
    if !children.iter().any(|child| child == name) {
        children.push(name.to_string());
    }
    parent_spec.add(
        ChildrenKey::PrimChildren,
        Value::TokenVec(children.into_iter().map(|child| child.into()).collect()),
    );
    Ok(())
}

fn child_names(spec: &openusd::sdf::SpecData, key: &str) -> Vec<String> {
    match spec.get(key) {
        Some(Value::TokenVec(tokens)) => tokens
            .iter()
            .map(|token| token.as_str().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

fn set_property_children(data: &mut Data, path: &str, names: &[&str]) -> Result<(), String> {
    let spec = data
        .spec_mut(&sdf_path(path)?)
        .ok_or_else(|| format!("Missing prim {path}"))?;
    spec.add(
        ChildrenKey::PropertyChildren,
        Value::TokenVec(names.iter().map(|name| (*name).into()).collect()),
    );
    Ok(())
}

fn container_name(field: &str) -> &str {
    match field {
        "panels" => "Parts",
        "fabrics" => "Fabrics",
        "seams" => "Seams",
        "template" => "Template",
        "state" => "Session",
        "properties" => "Properties",
        "polygon" => "Polygon",
        "placement" => "Placement",
        "curves" => "Curves",
        "holes" => "Holes",
        "lines" => "Lines",
        "notches" => "Notches",
        "measurements" => "Measurements",
        "fabric_appearance" => "Appearance",
        "graphics" => "Graphics",
        "trims" => "Trims",
        "mirror_links" => "Mirrors",
        "pin_targets" => "PinTargets",
        "frozen_panels" => "FrozenPanels",
        other => other,
    }
}

fn unique_name(base: &str, used: &mut BTreeSet<String>) -> String {
    let mut name = sanitize(base);
    let mut index = 2;
    while !used.insert(name.clone()) {
        name = format!("{}_{index}", sanitize(base));
        index += 1;
    }
    name
}

fn sanitize(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("item");
    }
    if out.as_bytes()[0].is_ascii_digit() {
        out.insert_str(0, "p_");
    }
    out
}

fn is_object_array(items: &[Json]) -> bool {
    !items.is_empty() && items.iter().all(Json::is_object)
}

fn encode_attribute(value: &Json) -> Option<(&'static str, Value)> {
    match value {
        Json::Bool(value) => Some(("bool", Value::Bool(*value))),
        Json::Number(number) => encode_number(number),
        Json::String(value) => Some(("string", Value::String(value.clone()))),
        Json::Array(items) => encode_array(items),
        _ => None,
    }
}

fn encode_number(number: &Number) -> Option<(&'static str, Value)> {
    if number.is_f64() {
        return Some(("double", Value::Double(number.as_f64()?)));
    }
    if let Some(value) = number.as_i64() {
        return Some(if i32::try_from(value).is_ok() {
            ("int", Value::Int(value as i32))
        } else {
            ("int64", Value::Int64(value))
        });
    }
    let value = number.as_u64()?;
    Some(if u32::try_from(value).is_ok() {
        ("uint", Value::Uint(value as u32))
    } else {
        ("uint64", Value::Uint64(value))
    })
}

fn encode_array(items: &[Json]) -> Option<(&'static str, Value)> {
    if items.is_empty() {
        return Some(("string[]", Value::StringVec(Vec::new())));
    }
    if items.iter().all(Json::is_boolean) {
        return Some((
            "bool[]",
            Value::BoolVec(items.iter().map(|v| v.as_bool().unwrap_or(false)).collect()),
        ));
    }
    if items.iter().all(Json::is_number) {
        return Some(encode_number_array(items));
    }
    if items.iter().all(Json::is_string) {
        return Some((
            "string[]",
            Value::StringVec(
                items
                    .iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect(),
            ),
        ));
    }
    encode_vec_array(items)
}

fn encode_number_array(items: &[Json]) -> (&'static str, Value) {
    if items
        .iter()
        .any(|value| matches!(value, Json::Number(number) if number.is_f64()))
    {
        return (
            "double[]",
            Value::DoubleVec(items.iter().filter_map(Json::as_f64).collect()),
        );
    }
    if let Some(values) = items.iter().map(Json::as_i64).collect::<Option<Vec<_>>>() {
        return ("int64[]", Value::Int64Vec(values));
    }
    (
        "uint64[]",
        Value::Uint64Vec(items.iter().filter_map(Json::as_u64).collect()),
    )
}

fn encode_vec_array(items: &[Json]) -> Option<(&'static str, Value)> {
    let mut points = Vec::with_capacity(items.len());
    let mut width = None;
    for item in items {
        let Json::Array(values) = item else {
            return None;
        };
        if !values.iter().all(Json::is_number) {
            return None;
        }
        match width {
            None => width = Some(values.len()),
            Some(width) if width != values.len() => return None,
            Some(_) => {}
        }
        points.push(values.iter().filter_map(Json::as_f64).collect::<Vec<_>>());
    }
    match width {
        Some(2) => Some((
            "double2[]",
            Value::Vec2dVec(
                points
                    .into_iter()
                    .map(|p| Vec2d { x: p[0], y: p[1] })
                    .collect(),
            ),
        )),
        Some(3) => Some((
            "double3[]",
            Value::Vec3dVec(
                points
                    .into_iter()
                    .map(|p| Vec3d {
                        x: p[0],
                        y: p[1],
                        z: p[2],
                    })
                    .collect(),
            ),
        )),
        _ => None,
    }
}

fn sdf_path(path: &str) -> Result<Path, String> {
    Path::new(path).map_err(|error| format!("Invalid USD path {path}: {error}"))
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

fn read_body(data: &Data, path: &str) -> Result<Json, String> {
    let spec = data
        .spec(&sdf_path(path)?)
        .ok_or_else(|| format!("Missing prim {path}"))?;
    let mut object = Map::new();
    for name in child_names(spec, "propertyChildren") {
        if name == KIND_ATTR || name == KEY_ATTR || name == NULLS_ATTR {
            continue;
        }
        let attr_path = sdf_path(&format!("{path}.{name}"))?;
        let attribute = data
            .spec(&attr_path)
            .ok_or_else(|| format!("Missing attribute {attr_path}"))?;
        let type_name = token_value(attribute.get("typeName"))
            .ok_or_else(|| format!("Attribute {attr_path} has no typeName"))?
            .to_string();
        let value = attribute
            .get("default")
            .ok_or_else(|| format!("Attribute {attr_path} has no default"))?;
        object.insert(name, decode_value(&type_name, value)?);
    }
    for name in child_names(spec, "primChildren") {
        let child_path = sdf_path(&format!("{path}/{name}"))?;
        let child = data
            .spec(&child_path)
            .ok_or_else(|| format!("Missing prim {child_path}"))?;
        match attribute_string(data, child_path.as_str(), KIND_ATTR)
            .unwrap_or_else(|| "object".to_string())
            .as_str()
        {
            "view" => continue,
            "array" => {
                let mut items = Vec::new();
                for item in child_names(child, "primChildren") {
                    let item_path = format!("{child_path}/{item}");
                    items.push(read_body(data, &item_path)?);
                }
                let key = attribute_string(data, child_path.as_str(), KEY_ATTR)
                    .ok_or_else(|| format!("Array prim {child_path} is missing {KEY_ATTR}"))?;
                object.insert(key, Json::Array(items));
            }
            _ => {
                let value = read_body(data, child_path.as_str())?;
                let key = attribute_string(data, child_path.as_str(), KEY_ATTR)
                    .ok_or_else(|| format!("Prim {child_path} is missing {KEY_ATTR}"))?;
                object.insert(key, value);
            }
        }
    }
    let nulls_path = sdf_path(&format!("{path}.{NULLS_ATTR}"))?;
    if let Some(attribute) = data.spec(&nulls_path)
        && let Some(Value::StringVec(names)) = attribute.get("default")
    {
        for name in names {
            object.insert(name.clone(), Json::Null);
        }
    }
    Ok(Json::Object(object))
}

fn attribute_string(data: &Data, prim_path: &str, name: &str) -> Option<String> {
    let attribute = data.spec(&sdf_path(&format!("{prim_path}.{name}")).ok()?)?;
    match attribute.get("default") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Token(value)) => Some(value.as_str().to_string()),
        _ => None,
    }
}

fn decode_value(type_name: &str, value: &Value) -> Result<Json, String> {
    let scalar = match (type_name, value) {
        ("bool", Value::Bool(value)) => Json::Bool(*value),
        ("int", Value::Int(value)) => json_int(i64::from(*value)),
        ("int64", Value::Int64(value)) => json_int(*value),
        ("uint", Value::Uint(value)) => json_uint(u64::from(*value)),
        ("uint64", Value::Uint64(value)) => json_uint(*value),
        ("double", Value::Double(value)) => float(*value),
        ("string", Value::String(value)) => Json::String(value.clone()),
        ("bool[]", Value::BoolVec(values)) => {
            Json::Array(values.iter().map(|v| Json::Bool(*v)).collect())
        }
        ("int64[]", Value::Int64Vec(values)) => {
            Json::Array(values.iter().map(|v| json_int(*v)).collect())
        }
        ("uint64[]", Value::Uint64Vec(values)) => {
            Json::Array(values.iter().map(|v| json_uint(*v)).collect())
        }
        ("int[]", Value::IntVec(values)) => {
            Json::Array(values.iter().map(|v| json_int(i64::from(*v))).collect())
        }
        ("double[]", Value::DoubleVec(values)) => {
            Json::Array(values.iter().map(|v| float(*v)).collect())
        }
        ("double2[]", Value::Vec2dVec(values)) => Json::Array(
            values
                .iter()
                .map(|p| Json::Array(vec![float(p.x), float(p.y)]))
                .collect(),
        ),
        ("double3[]", Value::Vec3dVec(values)) => Json::Array(
            values
                .iter()
                .map(|p| Json::Array(vec![float(p.x), float(p.y), float(p.z)]))
                .collect(),
        ),
        ("string[]", Value::StringVec(values)) => {
            Json::Array(values.iter().map(|v| Json::String(v.clone())).collect())
        }
        ("point3f[]", Value::Vec3fVec(values)) => Json::Array(
            values
                .iter()
                .map(|p| {
                    Json::Array(vec![
                        float(f64::from(p.x)),
                        float(f64::from(p.y)),
                        float(f64::from(p.z)),
                    ])
                })
                .collect(),
        ),
        ("dictionary", Value::Dictionary(values)) => {
            let mut object = Map::new();
            for (key, value) in values {
                object.insert(key.clone(), decode_value(value_type(value), value)?);
            }
            Json::Object(object)
        }
        _ => {
            return Err(format!(
                "Unsupported attribute type {type_name} for value {value:?}"
            ));
        }
    };
    Ok(scalar)
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Int64(_) => "int64",
        Value::Uint(_) => "uint",
        Value::Uint64(_) => "uint64",
        Value::Double(_) => "double",
        Value::String(_) => "string",
        Value::BoolVec(_) => "bool[]",
        Value::Int64Vec(_) => "int64[]",
        Value::Uint64Vec(_) => "uint64[]",
        Value::IntVec(_) => "int[]",
        Value::DoubleVec(_) => "double[]",
        Value::Vec2dVec(_) => "double2[]",
        Value::Vec3dVec(_) => "double3[]",
        Value::StringVec(_) => "string[]",
        Value::Dictionary(_) => "dictionary",
        _ => "unknown",
    }
}

fn json_int(value: i64) -> Json {
    Json::Number(Number::from(value))
}

fn json_uint(value: u64) -> Json {
    Json::Number(Number::from(value))
}

fn float(value: f64) -> Json {
    Number::from_f64(value).map_or(Json::Null, Json::Number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> GarmentTemplate {
        let text = match name {
            "two-panel" => include_str!("../tests/fixtures/legacy-templates/two-panel.json"),
            "curved-panels" => {
                include_str!("../tests/fixtures/legacy-templates/curved-panels.json")
            }
            other => panic!("unknown fixture {other}"),
        };
        serde_json::from_str(text).expect("fixture parses")
    }

    #[test]
    fn template_round_trips_through_usda() {
        for name in ["two-panel", "curved-panels"] {
            let template = fixture(name);
            let usda = template_to_usda(&template).expect("writes usda");
            assert!(usda.contains("metersPerUnit = 1"), "{name}: units");
            assert!(usda.contains("upAxis = \"Y\""), "{name}: up axis");
            if template.panels.iter().any(|panel| panel.polygon.is_some()) {
                assert!(usda.contains("BasisCurves"), "{name}: curve view");
            }
            let back = template_from_usda(&usda).expect("reads usda");
            assert_eq!(
                serde_json::to_value(&template).unwrap(),
                serde_json::to_value(&back).unwrap(),
                "{name}: round trip"
            );
        }
    }

    #[test]
    fn rejects_unknown_document_version() {
        let template = fixture("two-panel");
        let usda = template_to_usda(&template).unwrap();
        let broken = usda.replace("int version = 1", "int version = 2");
        let error = template_from_usda(&broken).expect_err("must reject");
        assert!(error.contains("version"), "{error}");
    }

    #[test]
    fn rejects_foreign_units() {
        let template = fixture("two-panel");
        let usda = template_to_usda(&template).unwrap();
        let broken = usda.replace("metersPerUnit = 1.0", "metersPerUnit = 0.01");
        let error = template_from_usda(&broken).expect_err("must reject");
        assert!(error.contains("metres"), "{error}");
    }

    #[test]
    fn project_round_trips_with_drape_state() {
        use crate::model::State;
        use crate::solver::SolverSettings;
        use glam::DVec3;

        let project = FittingProject {
            version: 1,
            template: fixture("curved-panels"),
            avatar_path: std::path::PathBuf::from("avatar.obj"),
            contact_stiffness: 1200.0,
            contact_clearance: 0.001,
            prearrange: false,
            quasistatic: false,
            solver: SolverSettings::default(),
            simulation: None,
            state: Some(State {
                x: vec![
                    DVec3::new(0.1, -0.2, 0.3),
                    DVec3::new(f64::MIN_POSITIVE, 1.0 / 3.0, -0.0),
                ],
                v: vec![DVec3::new(-1.0e-9, 2.0e-9, 0.0); 2],
                time: 1.0 / 60.0,
            }),
            mesh_signature: Some("0123456789abcdef".into()),
        };
        let usda = project_to_usda(&project).expect("writes project");
        let back = project_from_usda(&usda).expect("reads project");
        assert_eq!(
            serde_json::to_value(&project).unwrap(),
            serde_json::to_value(&back).unwrap()
        );
    }

    #[test]
    fn templates_are_saved_as_usd_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("garment.usda");
        let template = fixture("curved-panels");
        template.save(&path).expect("saves usda");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("#usda 1.0"));
        let loaded = GarmentTemplate::load(&path).expect("loads usda");
        assert_eq!(
            serde_json::to_value(&template).unwrap(),
            serde_json::to_value(&loaded).unwrap()
        );

        let json_path = directory.path().join("garment.json");
        let error = template.save(&json_path).expect_err("must reject JSON");
        assert!(error.contains("USD"), "{error}");

        // JSON stays readable as a legacy import.
        std::fs::write(&json_path, serde_json::to_vec(&template).unwrap()).unwrap();
        let loaded = GarmentTemplate::load(&json_path).expect("loads json");
        assert_eq!(
            serde_json::to_value(&template).unwrap(),
            serde_json::to_value(&loaded).unwrap()
        );
    }
}
