struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) dimensions: vec4<f32>,
    @location(2) @interpolate(flat) colour: vec4<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) local: vec2<f32>,
    @location(2) dimensions: vec4<f32>,
    @location(3) colour: vec4<f32>,
) -> Output {
    return Output(vec4<f32>(position, 0.0, 1.0), local, dimensions, colour);
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
fn fs_main(input: Output) -> @location(0) vec4<f32> {
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
    return vec4(premultiplied / max(alpha, 0.0001), alpha);
}
