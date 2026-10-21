//! Compiles the WGSL shaders in `shaders/` to SPIR-V with naga, so no Vulkan
//! SDK (glslc) is required to build, and adds the Windows version resource
//! (version from `version.txt`, bumped by tools\build.bat).

use std::path::PathBuf;

#[path = "../build-support/version_resource.rs"]
mod version_resource;

fn compile(name: &str, entries: &[(&str, naga::ShaderStage)]) {
    let src_path = PathBuf::from("shaders").join(format!("{name}.wgsl"));
    println!("cargo:rerun-if-changed={}", src_path.display());
    let src = std::fs::read_to_string(&src_path).expect("read shader");
    let module = match naga::front::wgsl::parse_str(&src) {
        Ok(m) => m,
        Err(e) => panic!("{}: {}", src_path.display(), e.emit_to_string(&src)),
    };
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}: validation failed: {e:?}", src_path.display()));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for (entry, stage) in entries {
        let options = naga::back::spv::Options {
            lang_version: (1, 3),
            flags: naga::back::spv::WriterFlags::empty(),
            ..Default::default()
        };
        let pipeline = naga::back::spv::PipelineOptions { shader_stage: *stage, entry_point: entry.to_string() };
        let words = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline))
            .unwrap_or_else(|e| panic!("{name}:{entry}: {e:?}"));
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        std::fs::write(out_dir.join(format!("{name}_{entry}.spv")), bytes).unwrap();
    }
}

fn main() {
    version_resource::embed(&version_resource::VersionInfo {
        file_description: "Hitman Codename 47 Rust library",
        original_filename: "ddraw.dll",
        internal_name: "hm1vk",
        dll: true,
    });
    compile("ff", &[("vs_main", naga::ShaderStage::Vertex), ("fs_main", naga::ShaderStage::Fragment)]);
    compile("present", &[("vs_main", naga::ShaderStage::Vertex), ("fs_main", naga::ShaderStage::Fragment)]);
}
