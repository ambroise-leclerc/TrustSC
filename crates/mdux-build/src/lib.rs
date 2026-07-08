#![forbid(unsafe_code)]

//! Build-script helper that wraps the MedUI DSL compiler (`mdux-ui-dsl-authoring`) behind a
//! small, explicit builder, so an application's `build.rs` does not need to hand-roll
//! `OUT_DIR`/`rerun-if-changed` plumbing or resolve the standard text package itself.
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     mdux_build::MeduiScreen::new("hello_world.medui")
//!         .surface(800, 480)
//!         .compile()
//! }
//! ```
//!
//! Pair this with `mdux::include_medui_screen!()` in the crate's `src/` to bring the generated
//! `medui_screen` module into scope with no manual imports.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use mdux_ui_dsl_authoring::{CompileOptions, ImagePackages, TextPackages};

mod scenario;
pub use scenario::ScenarioSet;

type DynError = Box<dyn std::error::Error>;

/// Builder for compiling one `.medui` file into the generated Rust module consumed by
/// `mdux::include_medui_screen!()`.
pub struct MeduiScreen {
    file: PathBuf,
    surface_width: Option<u32>,
    surface_height: Option<u32>,
}

impl MeduiScreen {
    /// `file` is resolved relative to `CARGO_MANIFEST_DIR` of the calling build script.
    pub fn new(file: impl AsRef<Path>) -> Self {
        Self {
            file: file.as_ref().to_path_buf(),
            surface_width: None,
            surface_height: None,
        }
    }

    /// The layout viewport the DSL compiler resolves `Fill` dimensions against. Required before
    /// calling [`compile`](Self::compile) — there is no implicit default, since the surface size
    /// is a layout-affecting input that should be an explicit, reviewable choice.
    pub fn surface(mut self, width: u32, height: u32) -> Self {
        self.surface_width = Some(width);
        self.surface_height = Some(height);
        self
    }

    /// Parses, validates, and compiles the `.medui` file into
    /// `$OUT_DIR/mdux_medui_screen.rs`, and emits `cargo:rerun-if-changed` for the source file.
    pub fn compile(self) -> Result<(), DynError> {
        let width = self
            .surface_width
            .ok_or("MeduiScreen: call .surface(width, height) before .compile()")?;
        let height = self
            .surface_height
            .ok_or("MeduiScreen: call .surface(width, height) before .compile()")?;

        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        let medui_path = manifest_dir.join(&self.file);
        let out_dir = PathBuf::from(env::var("OUT_DIR")?);
        let generated_path = out_dir.join("mdux_medui_screen.rs");

        println!("cargo:rerun-if-changed={}", medui_path.display());
        fs::create_dir_all(&out_dir)?;

        // Both approved packages are always available from the facade; NumericDisplay budgets
        // resolve in the display package, everything else in the standard one (ADR-013).
        let standard_package = mdux::default_standard_text_package()?;
        let display_packages = mdux::default_display_text_packages()?;
        let display_refs = display_packages.iter().collect::<Vec<_>>();
        let image_packages = mdux::default_image_packages()?;
        mdux_ui_dsl_authoring::compile_medui_file_to_rust_module(
            &medui_path,
            &generated_path,
            CompileOptions::new(width, height),
            TextPackages::with_displays(&standard_package, &display_refs),
            ImagePackages::new(&image_packages),
        )?;

        Ok(())
    }
}
