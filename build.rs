fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    #[cfg(feature = "desktop")]
    shaders::build();
}

#[cfg(feature = "desktop")]
mod shaders {
    use naga::valid::{
        Capabilities, ShaderStages, SubgroupOperationSet, ValidationFlags, Validator,
    };
    use wesl::{Compiler, Feature, ManglerKind};

    pub fn build() {
        let mut avbd = Compiler::default();
        // Keep the entry points defined across the AVBD modules and preserve
        // the names used by its compute pipeline descriptors.
        avbd.options.strip = false;
        avbd.options.mangler = ManglerKind::None;
        compile(&avbd, "src/gpu_avbd/main.wesl", "avbd", false);
        compile(
            &Compiler::default(),
            "src/gpu_avbd/filter_probe.wesl",
            "avbd_filter_probe",
            false,
        );

        let mut vbd = Compiler::default();
        vbd.options.features.default = Feature::Error;
        for (artifact, subgroups) in [("vbd_portable", false), ("vbd_subgroup", true)] {
            vbd.options.features.set("subgroups", subgroups);
            compile(&vbd, "src/gpu_vbd/vbd.wesl", artifact, subgroups);
        }
    }

    fn compile(compiler: &Compiler, path: &str, artifact: &str, subgroups: bool) {
        let shader = compiler
            .compile(path)
            .unwrap_or_else(|error| panic!("could not compile {artifact}:\n{error}"));
        shader.emit_rerun_if_changed();

        let wgsl = shader.to_string();
        let module = naga::front::wgsl::parse_str(&wgsl).unwrap_or_else(|error| {
            panic!("invalid {artifact} WGSL:\n{}", error.emit_to_string(&wgsl))
        });
        let mut capabilities = Capabilities::empty();
        capabilities.set(Capabilities::SUBGROUP, subgroups);
        let mut validator = Validator::new(ValidationFlags::all(), capabilities);
        if subgroups {
            validator
                .subgroup_stages(ShaderStages::COMPUTE)
                .subgroup_operations(SubgroupOperationSet::all());
        }
        validator.validate(&module).unwrap_or_else(|error| {
            panic!("invalid {artifact} WGSL:\n{}", error.emit_to_string(&wgsl))
        });
        shader.write_artifact(artifact);
    }
}
