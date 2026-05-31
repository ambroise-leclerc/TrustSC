use std::{
    env, fs,
    path::{Path, PathBuf},
};

use mdux::{default_standard_text_package, DEFAULT_STANDARD_FONT};
use mdux_ui_dsl_authoring::{CompileOptions, compile_medui_file_to_rust_module};

type DynError = Box<dyn std::error::Error>;

struct ShaderSpec {
    source: &'static str,
    output: &'static str,
    env_var: &'static str,
    kind: shaderc::ShaderKind,
}

const SHADERS: &[ShaderSpec] = &[
    ShaderSpec {
        source: "hello_text.vert",
        output: "hello_text.vert.spv",
        env_var: "HELLO_WORLD_TEXT_VERT_SPV",
        kind: shaderc::ShaderKind::Vertex,
    },
    ShaderSpec {
        source: "hello_text.frag",
        output: "hello_text.frag.spv",
        env_var: "HELLO_WORLD_TEXT_FRAG_SPV",
        kind: shaderc::ShaderKind::Fragment,
    },
];

fn main() -> Result<(), DynError> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or("failed to determine workspace root from hello_world manifest dir")?;
    let shader_dir = manifest_dir.join("shaders");
    let medui_path = manifest_dir.join("hello_world.medui");
    let standard_text_package_path = workspace_root.join(DEFAULT_STANDARD_FONT.package_json_path);
    let out_dir = PathBuf::from(env::var("OUT_DIR")?).join("shaders");
    let medui_out = PathBuf::from(env::var("OUT_DIR")?).join("hello_world_medui.rs");

    println!("cargo:rerun-if-changed={}", shader_dir.display());
    println!("cargo:rerun-if-changed={}", medui_path.display());
    println!(
        "cargo:rerun-if-changed={}",
        standard_text_package_path.display()
    );

    fs::create_dir_all(&out_dir)?;
    let text_package = default_standard_text_package()?;
    compile_medui_file_to_rust_module(
        &medui_path,
        &medui_out,
        CompileOptions::new(800, 480),
        &text_package,
    )?;

    let compiler = shaderc::Compiler::new()?;
    let mut options = shaderc::CompileOptions::new()?;
    options.set_source_language(shaderc::SourceLanguage::GLSL);
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_0 as u32,
    );
    options.set_target_spirv(shaderc::SpirvVersion::V1_0);
    options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    options.set_warnings_as_errors();

    let shader_dir_for_include = shader_dir.clone();
    options.set_include_callback(move |requested, _, source, _| {
        let include_path = shader_dir_for_include.join(requested);
        let content = fs::read_to_string(&include_path).map_err(|error| {
            format!(
                "failed to resolve shader include '{requested}' from '{}': {error}",
                source
            )
        })?;

        Ok(shaderc::ResolvedInclude {
            resolved_name: include_path.to_string_lossy().into_owned(),
            content,
        })
    });

    for shader in SHADERS {
        compile_shader(&compiler, &options, &shader_dir, &out_dir, shader)?;
    }

    println!(
        "cargo:rustc-env=HELLO_WORLD_SHADER_DIR={}",
        out_dir.display()
    );
    Ok(())
}

fn compile_shader(
    compiler: &shaderc::Compiler,
    options: &shaderc::CompileOptions<'_>,
    shader_dir: &Path,
    out_dir: &Path,
    shader: &ShaderSpec,
) -> Result<(), DynError> {
    let source_path = shader_dir.join(shader.source);
    let source_text = fs::read_to_string(&source_path)?;
    let artifact = compiler.compile_into_spirv(
        &source_text,
        shader.kind,
        source_path.to_string_lossy().as_ref(),
        "main",
        Some(options),
    )?;

    let output_path = out_dir.join(shader.output);
    fs::write(&output_path, artifact.as_binary_u8())?;

    println!("cargo:rerun-if-changed={}", source_path.display());
    println!(
        "cargo:rustc-env={}={}",
        shader.env_var,
        output_path.display()
    );
    Ok(())
}
