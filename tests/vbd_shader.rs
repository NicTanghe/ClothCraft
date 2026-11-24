#[test]
fn contact_bvh_validates_without_a_gpu() {
    let source = format!(
        "{}\n{}",
        include_str!("../src/gpu_vbd/contact_bvh.wgsl"),
        include_str!("../src/gpu_vbd/contact_geometry.wgsl")
    );
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
#[cfg(feature = "desktop")]
fn portable_reduction_uses_local_invocation_index() {
    check_vbd_lane_builtin(
        include_str!(concat!(env!("OUT_DIR"), "/vbd_portable.wgsl")),
        naga::BuiltIn::LocalInvocationIndex,
    );
}

#[test]
#[cfg(feature = "desktop")]
fn subgroup_reduction_uses_subgroup_invocation_id() {
    check_vbd_lane_builtin(
        include_str!(concat!(env!("OUT_DIR"), "/vbd_subgroup.wgsl")),
        naga::BuiltIn::SubgroupInvocationId,
    );
}

#[cfg(feature = "desktop")]
fn check_vbd_lane_builtin(source: &str, builtin: naga::BuiltIn) {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    assert_eq!(module.entry_points.len(), 3);
    assert!(
        module
            .entry_points
            .iter()
            .any(|entry| entry.name == "predict")
    );
    for name in ["iterate", "probe"] {
        let entry = module
            .entry_points
            .iter()
            .find(|entry| entry.name == name)
            .unwrap();
        assert_eq!(entry.workgroup_size, [32, 1, 1]);
        assert_eq!(
            entry.function.arguments[1].binding,
            Some(naga::Binding::BuiltIn(builtin))
        );
    }
}
