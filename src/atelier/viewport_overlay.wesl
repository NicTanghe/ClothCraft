// Screen-aligned overlays share a top-left origin and premultiplied alpha.
fn viewport_clip_position(normalized: vec2<f32>) -> vec4<f32> {
    return vec4(normalized * vec2(2.0, -2.0) + vec2(-1.0, 1.0), 0.0, 1.0);
}

struct FacePaint {
    origin_and_u: vec4<f32>,
    v_and_glyph_right: vec4<f32>,
    colour_and_hover: vec4<f32>,
    glyph_and_silhouette: vec4<f32>,
}

struct CubePaint {
    bounds: vec4<f32>,
    viewport: vec4<f32>,
    grid: vec4<f32>,
    faces: array<FacePaint, 3>,
}

@group(0) @binding(0) var<uniform> paint: CubePaint;

@vertex
fn cube_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    let position = mix(paint.bounds.xy, paint.bounds.zw, corners[index]);
    return viewport_clip_position(position / paint.viewport.xy);
}

fn sample_cube(position: vec2<f32>) -> vec4<f32> {
    for (var index = 0u; index < u32(paint.viewport.w); index++) {
        let face = paint.faces[index];
        let offset = position - face.origin_and_u.xy;
        let du = face.origin_and_u.zw;
        let dv = face.v_and_glyph_right.xy;
        let uv = vec2(dot(offset, du), dot(offset, dv));
        if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
            continue;
        }

        // Convert parametric distances to device pixels. Each side of an
        // interior seam contributes half the stroke, including cube creases.
        let units_per_pixel = vec2(length(du), length(dv));
        let half_width = 0.625 * paint.viewport.z;
        let internal = min(abs(uv - paint.grid.x), abs(uv - paint.grid.y));
        let distance = internal / units_per_pixel;
        let silhouette = u32(face.glyph_and_silhouette.y);
        let outer = vec4(uv.x, 1.0 - uv.x, uv.y, 1.0 - uv.y) / units_per_pixel.xxyy;
        let widths = half_width * select(
            vec4(1.0), vec4(2.0), (vec4(1u, 2u, 4u, 8u) & vec4(silhouette)) != vec4(0u)
        );
        if any(distance < vec2(half_width)) || any(outer < widths) {
            return vec4(0.035, 0.044, 0.055, 1.0);
        }

        let cell = select(vec2(0u), vec2(1u), uv >= vec2(paint.grid.x))
            + select(vec2(0u), vec2(1u), uv >= vec2(paint.grid.y));
        var colour = face.colour_and_hover.rgb * select(0.74, 1.0, all(cell == vec2(1u)));
        if f32(cell.y * 3u + cell.x) == face.colour_and_hover.w {
            colour = mix(colour, vec3(1.0), 0.23);
        }
        // Sample the bitmap in face coordinates so it foreshortens with the face.
        let right = face.v_and_glyph_right.zw;
        let down = vec2(right.y, -right.x);
        let local = uv - vec2(0.5);
        let glyph_pos = floor(vec2(dot(local, right), dot(local, down)) / 0.034 + vec2(1.5, 2.5));
        if all(glyph_pos >= vec2(0.0)) && all(glyph_pos < vec2(3.0, 5.0)) {
            let bit = (4u - u32(glyph_pos.y)) * 3u + (2u - u32(glyph_pos.x));
            if (u32(face.glyph_and_silhouette.x) & (1u << bit)) != 0u {
                colour = vec3(0.95);
            }
        }
        return vec4(colour, 1.0);
    }
    return vec4(0.0);
}

@fragment
fn cube_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // Sample the whole cube together so adjacent faces share edge coverage.
    var colour = vec4(0.0);
    for (var y = 0u; y < 4u; y++) {
        for (var x = 0u; x < 4u; x++) {
            let offset = vec2(f32(x), f32(y)) * 0.25 - vec2(0.375);
            colour += sample_cube(position.xy + offset);
        }
    }
    return colour / 16.0;
}

struct GizmoVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) dimensions: vec4<f32>,
    @location(2) @interpolate(flat) colour: vec4<f32>,
}

@vertex
fn gizmo_vertex(
    @location(0) position: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) dimensions: vec4<f32>,
    @location(3) colour: vec4<f32>,
) -> GizmoVertex {
    return GizmoVertex(viewport_clip_position(position), local, dimensions, colour);
}

fn box_distance(p: vec2<f32>, span: f32, half_width: f32) -> f32 {
    let half_size = vec2(span * 0.5, half_width);
    let d = abs(p - vec2(half_size.x, 0.0)) - half_size;
    return length(max(d, vec2(0.0))) + min(max(d.x, d.y), 0.0);
}

fn edge_distance(p: vec2<f32>, start: vec2<f32>, end: vec2<f32>) -> f32 {
    let edge = end - start;
    let along = clamp(dot(p - start, edge) / max(dot(edge, edge), 1e-8), 0.0, 1.0);
    return distance(p, start + edge * along);
}

fn arrow_distance(point: vec2<f32>, tip: f32, half_width: f32, head: f32) -> f32 {
    // Reflect across the shaft centerline. Only the exterior tail, shaft,
    // shoulder and slope contribute distance. The head and shaft share a fill.
    let p = vec2(point.x, abs(point.y));
    let base = tip - head;
    let wing = head * 0.48;
    let a = vec2(0.0, half_width);
    let b = vec2(base, half_width);
    let c = vec2(base, wing);
    let nearest = min(
        min(edge_distance(p, vec2(0.0), a), edge_distance(p, a, b)),
        min(edge_distance(p, b, c), edge_distance(p, c, vec2(tip, 0.0))),
    );
    let inside = p.x >= 0.0 && p.x <= tip
        && p.y <= select(half_width, (tip - p.x) * (wing / head), p.x >= base);
    return select(nearest, -nearest, inside);
}

fn coverage(edge: f32) -> f32 {
    // Derivatives convert logical units to a device-pixel coverage ramp.
    return clamp(0.5 - edge / max(fwidth(edge), 0.0001), 0.0, 1.0);
}

@fragment
fn gizmo_fragment(input: GizmoVertex) -> @location(0) vec4<f32> {
    let dims = input.dimensions;
    var edge = abs(input.local.y) - dims.y;
    if dims.z > 0.0 {
        edge = arrow_distance(input.local, dims.x, dims.y, dims.z);
    } else if dims.w == 0.0 {
        // The toggle uses closed rectangles. Ring strokes meet at ends
        // clipped by the mesh so the shader blends only their side edges.
        edge = box_distance(input.local, dims.x, dims.y);
    }
    let interior = coverage(edge);
    let outline = coverage(edge - dims.w);
    let fill = interior * input.colour.a;
    let ink = select(
        0.0, 0.85 * (max(outline - interior, 0.0) + interior - fill), dims.w > 0.0,
    );
    let alpha = fill + ink;
    let premultiplied = input.colour.rgb * fill + vec3(0.016, 0.020, 0.030) * ink;
    return vec4(premultiplied, alpha);
}
