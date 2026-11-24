import package::contact::separated_beyond_envelope;
import package::main::contacts;

@compute @workgroup_size(64)
fn probe_filter(@builtin(global_invocation_id) id: vec3u) {
    if id.x < 64u {
        contacts.pairs[id.x].normal_curvature.w =
            select(0.0, 1.0, separated_beyond_envelope(contacts.pairs[id.x]));
    }
}
