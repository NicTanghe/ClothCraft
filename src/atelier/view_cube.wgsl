struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) colour: vec3<f32>,
}

@vertex
fn vertex(@location(0) position: vec2<f32>, @location(1) colour: vec3<f32>) -> Varying {
    return Varying(vec4<f32>(position, 0.0, 1.0), colour);
}

@fragment
fn fragment(input: Varying) -> @location(0) vec4<f32> {
    return vec4<f32>(input.colour, 1.0);
}
