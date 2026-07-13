//! Bridges an authoring-side [`CompiledScreenSpec`] to the pixel-exact
//! `adapters/trustsc-vulkan-winit` offscreen renderer and encodes the captured frame as PNG
//! (ADR-022 wave S7). This is the riskiest piece of the whole studio: it reuses the exact same
//! render path `--verify-ui` (ADR-016) already exercises in CI — there is no parallel renderer
//! that could drift from what a device actually draws.

use std::error::Error;

use png::{BitDepth, ColorType, Encoder};
use trustsc::realtime::{FrameInputs, ScreenBindings};
use trustsc::screen_text::ScreenTextLayout;
use trustsc::{
    ButtonSpec, ClockSpec, CompiledNode, CompiledNodeKind, CompiledScreenPackage,
    CriticalButtonSpec, GoldenReferenceEntry, ImagePackage, ImageSpec, LabelSpec, LayoutSpec,
    NumericDisplaySpec, PanelSpec, Rect, SignalTraceSpec, StatusIndicatorSpec, TextInputSpec,
    TextPackage, ViewportReservation,
};
use trustsc_ui_dsl_authoring::{CompiledScreenSpec, NodeKind};
use trustsc_vulkan_winit::{CapturedFrame, InteractionSnapshot, OffscreenRenderer, WallClock};

pub type BridgeError = Box<dyn Error>;

/// The pinned clock every studio render uses, so identical inputs always render identical bytes
/// (the same determinism seam `--verify-ui` pins for its base capture).
pub const STUDIO_CLOCK: WallClock = WallClock {
    year: 2026,
    month: 1,
    day: 1,
    hours: 12,
    minutes: 0,
    seconds: 0,
};

fn leak_str(value: String) -> &'static str {
    value.leak()
}

fn leak_opt_str(value: Option<String>) -> Option<&'static str> {
    value.map(leak_str)
}

fn leak_str_slice(values: Vec<String>) -> &'static [&'static str] {
    let leaked: Vec<&'static str> = values.into_iter().map(leak_str).collect();
    Box::leak(leaked.into_boxed_slice())
}

fn leak_slice<T>(values: Vec<T>) -> &'static [T] {
    Box::leak(values.into_boxed_slice())
}

fn leak_rect(rect: trustsc_ui_dsl_authoring::RectSpec) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn leak_node_kind(kind: NodeKind) -> CompiledNodeKind {
    match kind {
        NodeKind::CriticalButton {
            requirement_id,
            label_text_key,
            color_token,
            on_press,
        } => CompiledNodeKind::CriticalButton(CriticalButtonSpec {
            requirement_id: leak_str(requirement_id),
            text_key: leak_str(label_text_key),
            color_token: leak_str(color_token),
            on_press,
        }),
        NodeKind::VulkanViewport { stream_source } => {
            CompiledNodeKind::VulkanViewport(ViewportReservation {
                stream_source: leak_str(stream_source),
            })
        }
        NodeKind::SignalTrace {
            stream_source,
            color_token,
        } => CompiledNodeKind::SignalTrace(SignalTraceSpec {
            stream_source: leak_str(stream_source),
            color_token: leak_str(color_token),
        }),
        NodeKind::Label {
            text_key,
            color_token,
        } => CompiledNodeKind::Label(LabelSpec {
            text_key: leak_str(text_key),
            color_token: leak_str(color_token),
        }),
        NodeKind::Clock { format } => CompiledNodeKind::Clock(ClockSpec { format }),
        NodeKind::NumericDisplay {
            requirement_id,
            template_id,
            source,
            color_token,
        } => CompiledNodeKind::NumericDisplay(NumericDisplaySpec {
            requirement_id: leak_str(requirement_id),
            template_id: leak_str(template_id),
            source: leak_str(source),
            color_token: leak_str(color_token),
        }),
        NodeKind::StatusIndicator {
            requirement_id,
            source,
            state_text_keys,
            color_tokens,
        } => CompiledNodeKind::StatusIndicator(StatusIndicatorSpec {
            requirement_id: leak_str(requirement_id),
            source: leak_str(source),
            state_text_keys: leak_str_slice(state_text_keys),
            color_tokens: leak_str_slice(color_tokens),
        }),
        // Compiler-synthesized only (a Row's `background:`), but a fully compiled
        // CompiledScreenSpec legitimately contains these, unlike a parsed ScreenDefinition AST.
        NodeKind::Panel { color_token } => CompiledNodeKind::Panel(PanelSpec {
            color_token: leak_str(color_token),
        }),
        NodeKind::Image { image_id } => CompiledNodeKind::Image(ImageSpec {
            image_id: leak_str(image_id),
        }),
        NodeKind::Button {
            label_text_key,
            color_token,
            source,
            requirement_id,
        } => CompiledNodeKind::Button(ButtonSpec {
            text_key: leak_str(label_text_key),
            color_token: leak_str(color_token),
            source: leak_str(source),
            requirement_id: leak_opt_str(requirement_id),
        }),
        NodeKind::TextInput {
            source,
            max_length,
            glyph_set_id,
            color_token,
            requirement_id,
        } => CompiledNodeKind::TextInput(TextInputSpec {
            source: leak_str(source),
            max_length,
            glyph_set_id: leak_str(glyph_set_id),
            color_token: leak_str(color_token),
            requirement_id: leak_opt_str(requirement_id),
        }),
    }
}

/// Mechanically maps an authoring-side [`CompiledScreenSpec`] (owned `String`s) onto the runtime
/// [`CompiledScreenPackage`] shape (`&'static str`, ADR-009) via `Box`/`String::leak`, so it can
/// be handed to the exact same rendering path a device build uses. Each call leaks a few KB —
/// acceptable for a host tool session (one leak per render request), never done on a device.
pub fn leak_package(spec: &CompiledScreenSpec) -> &'static CompiledScreenPackage {
    let spec = spec.clone();

    let nodes = spec
        .nodes
        .into_iter()
        .map(|node| CompiledNode {
            id: leak_str(node.id),
            bounds: leak_rect(node.bounds),
            kind: leak_node_kind(node.kind),
        })
        .collect::<Vec<_>>();

    let golden_references = spec
        .golden_references
        .into_iter()
        .map(|golden| GoldenReferenceEntry {
            node_id: leak_str(golden.node_id),
            bounds: leak_rect(golden.bounds),
            text_key: leak_opt_str(golden.text_key),
            color_token: leak_opt_str(golden.color_token),
            cv_checks: leak_slice(golden.cv_checks),
        })
        .collect::<Vec<_>>();

    let package = CompiledScreenPackage {
        screen_id: leak_str(spec.id),
        layout: LayoutSpec {
            kind: spec.layout.kind,
            spacing: spec.layout.spacing,
            padding: spec.layout.padding,
        },
        nodes: leak_slice(nodes),
        golden_references: leak_slice(golden_references),
    };
    Box::leak(Box::new(package))
}

/// Fills every dynamic realtime binding with a placeholder value so a rendered preview never
/// looks blank: mid-range numerics (sized to each template's digit capacity so no template can
/// ever reject it), state index 0 (already `FrameInputs::from_bindings`'s default — nothing to
/// do), and a synthetic sine wave for `SignalTrace`/`VulkanViewport` rings. `TextInput` echoes
/// stay empty (also already the default): an empty controlled-component field is a legitimate,
/// unremarkable preview state, unlike an empty waveform.
fn fill_placeholder_inputs(
    bindings: &ScreenBindings,
    inputs: &mut FrameInputs,
) -> Result<(), BridgeError> {
    for binding in &bindings.numbers {
        let capacity = binding.capacity.min(18);
        let magnitude = if capacity == 0 {
            0
        } else {
            (10i64.pow(capacity as u32) - 1) / 2
        };
        inputs.set_number(binding.source, magnitude)?;
    }

    for binding in &bindings.streams {
        for row_index in 0..binding.rows {
            let row: Vec<f32> = (0..binding.bins)
                .map(|bin_index| {
                    let phase = row_index as f32 * 0.35 + bin_index as f32 * 0.12;
                    0.5 + 0.5 * phase.sin()
                })
                .collect();
            inputs.push_row(binding.source, &row)?;
        }
    }

    for binding in &bindings.traces {
        for sample_index in 0..binding.capacity {
            inputs.push_sample(binding.source, (sample_index as f32 * 0.2).sin())?;
        }
    }

    Ok(())
}

/// Renders a compiled screen offscreen and returns the captured frame, with every dynamic
/// binding filled from [`fill_placeholder_inputs`]. `app_name` only affects diagnostics (it is
/// never rendered).
pub fn render_screen(
    app_name: &str,
    package: &'static CompiledScreenPackage,
    standard: TextPackage,
    displays: Vec<TextPackage>,
    image_packages: &[ImagePackage],
    locale: &str,
    width: u32,
    height: u32,
) -> Result<CapturedFrame, BridgeError> {
    let layout = ScreenTextLayout::from_screen(package, standard.clone(), locale)?;
    let bindings = ScreenBindings::from_screen(package, standard, displays, image_packages, locale)?;
    let mut inputs = FrameInputs::from_bindings(&bindings)?;
    fill_placeholder_inputs(&bindings, &mut inputs)?;

    let mut renderer = OffscreenRenderer::new(app_name, layout, bindings, width, height)?;
    renderer.draw_frame(&inputs, STUDIO_CLOCK, InteractionSnapshot::default())?;
    Ok(renderer.read_pixels()?)
}

/// Encodes a captured frame as a PNG byte buffer (8-bit RGBA).
pub fn encode_png(frame: &CapturedFrame) -> Result<Vec<u8>, BridgeError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut bytes, frame.width, frame.height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&frame.rgba)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::Decoder;
    use trustsc_ui_dsl_authoring::{compile_medui_source, CompileOptions, ImagePackages, TextPackages};

    const HELLO_WORLD_MEDUI: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello_world/hello_world.medui"));

    fn hello_world_package() -> &'static CompiledScreenPackage {
        let standard = trustsc::default_standard_text_package().expect("standard package");
        let displays = trustsc::default_display_text_packages().expect("display packages");
        let display_refs = displays.iter().collect::<Vec<_>>();
        let images = trustsc::default_image_packages().expect("image packages");
        let spec = compile_medui_source(
            HELLO_WORLD_MEDUI,
            &CompileOptions::new(800, 480),
            TextPackages::with_displays(&standard, &display_refs),
            ImagePackages::new(&images),
        )
        .expect("hello_world should compile");
        leak_package(&spec)
    }

    /// The `include_medui_screen!`-generated static for `examples/hello_world` is a
    /// `pub(in that binary)` `include!`d module, not reachable from another crate — so this
    /// pins the exact values a fresh `cargo build -p hello_world` generates instead (verified by
    /// reading `target/debug/build/hello_world-*/out/trustsc_medui_screen.rs`), per the issue's
    /// documented fallback ("or hard-coded expected nodes").
    #[test]
    fn leak_package_matches_the_generated_static_for_hello_world() {
        let bridged = hello_world_package();

        assert_eq!(bridged.screen_id, "HelloWorld");
        assert_eq!(
            bridged.layout,
            LayoutSpec {
                kind: trustsc::LayoutKind::Vertical,
                spacing: 16,
                padding: 24,
            }
        );

        assert_eq!(bridged.nodes.len(), 2);

        let label = &bridged.nodes[0];
        assert_eq!(label.id, "hello-world-label");
        assert_eq!(label.bounds, Rect { x: 24, y: 24, width: 752, height: 48 });
        assert_eq!(
            label.kind,
            CompiledNodeKind::CriticalButton(CriticalButtonSpec {
                requirement_id: "REQ-HELLO-001",
                text_key: "STR-HELLO-WORLD",
                color_token: "Theme.Colors.PrimaryAction",
                on_press: trustsc::SystemEvent::NoOp,
            })
        );

        let viewport = &bridged.nodes[1];
        assert_eq!(viewport.id, "hello-world-viewport");
        assert_eq!(viewport.bounds, Rect { x: 24, y: 88, width: 752, height: 280 });
        assert_eq!(
            viewport.kind,
            CompiledNodeKind::VulkanViewport(ViewportReservation {
                stream_source: "HELLO_WORLD_SIM",
            })
        );

        assert_eq!(bridged.golden_references.len(), 1);
        let golden = &bridged.golden_references[0];
        assert_eq!(golden.node_id, "hello-world-label");
        assert_eq!(golden.bounds, Rect { x: 24, y: 24, width: 752, height: 48 });
        assert_eq!(golden.text_key, Some("STR-HELLO-WORLD"));
        assert_eq!(golden.color_token, Some("Theme.Colors.PrimaryAction"));
        assert_eq!(
            golden.cv_checks,
            &[trustsc::CvCheckKind::Bounds, trustsc::CvCheckKind::ColorHash]
        );
    }

    /// The one test in this crate that actually opens a Vulkan device. `OffscreenRenderer::new`
    /// failing is overwhelmingly "no Vulkan ICD on this host" for a host tooling test (never a
    /// concern for governed/adapter code, which requires Vulkan unconditionally) — skip loudly
    /// instead of failing so `cargo test` stays green on a bare machine, per the S7 acceptance
    /// criteria.
    #[test]
    fn render_screen_produces_a_non_blank_frame_at_the_authored_surface_extent() {
        let standard = trustsc::default_standard_text_package().expect("standard package");
        let displays = trustsc::default_display_text_packages().expect("display packages");
        let images = trustsc::default_image_packages().expect("image packages");
        let package = hello_world_package();

        let result = render_screen(
            "trustsc-medui-studio test",
            package,
            standard,
            displays,
            &images,
            "en-US",
            800,
            480,
        );

        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                eprintln!(
                    "SKIPPED render_screen_produces_a_non_blank_frame_at_the_authored_surface_extent: \
                     no Vulkan ICD available (or offscreen renderer init failed): {error}"
                );
                return;
            }
        };

        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 480);
        assert!(
            !frame.rgba.chunks_exact(4).all(|pixel| pixel == &frame.rgba[0..4]),
            "captured frame is a single uniform color — nothing appears to have rendered"
        );
    }

    #[test]
    fn encode_png_roundtrips_dimensions_and_pixels() {
        let width = 4;
        let height = 2;
        let rgba = vec![7u8; (width * height * 4) as usize];
        let frame = CapturedFrame {
            width,
            height,
            rgba: rgba.clone(),
        };

        let bytes = encode_png(&frame).expect("encoding should succeed");

        let mut reader = Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .expect("encoded bytes should be a valid PNG");
        assert_eq!(reader.info().width, width);
        assert_eq!(reader.info().height, height);
        let mut buf = vec![0u8; reader.output_buffer_size().expect("known-good fixed-size RGBA8 image")];
        let frame_info = reader.next_frame(&mut buf).expect("frame should decode");
        assert_eq!(&buf[..frame_info.buffer_size()], rgba.as_slice());
    }
}
