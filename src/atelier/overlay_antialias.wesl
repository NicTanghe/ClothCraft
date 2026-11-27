// Center a regular supersampling grid within one device pixel.
public fn subpixel_offset(sample_index: vec2<u32>, count_per_axis: u32) -> vec2<f32> {
    return (vec2<f32>(sample_index) + vec2(0.5)) / f32(count_per_axis) - vec2(0.5);
}

public fn distance_coverage(edge: f32) -> f32 {
    // Derivatives convert logical units to a device-pixel coverage ramp.
    return clamp(0.5 - edge / max(fwidth(edge), 0.0001), 0.0, 1.0);
}
