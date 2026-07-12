//! Offscreen render path (ADR-016 §1): the same pixels a windowed [`crate::App`] presents,
//! without a window. `OffscreenRenderer` wraps a headless-constructed [`VulkanRenderer`] —
//! headless instance, no surface, one `R8G8B8A8_UNORM` color image at the authored surface
//! extent instead of a swapchain — and reuses the exact same per-frame writes and command
//! recording as presented frames, so there is no second renderer to drift from the product.
//! First consumer: `trustsc-ui-verify`'s rendered-truth checks and manual-generation captures.

use trustsc::realtime::{FrameInputs, ScreenBindings};
use trustsc::screen_text::ScreenTextLayout;

use crate::renderer::{BoxError, InteractionSnapshot, VulkanRenderer, WallClock};

/// One captured offscreen frame: tightly packed, row-major, top-to-bottom RGBA8 bytes at
/// `width`x`height` — the authored surface extent, so every measured pixel coordinate equals the
/// compiled and golden coordinates 1:1.
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Renders a compiled screen offscreen for automated verification and manual-capture tooling
/// (ADR-016). Every draw goes through the same `record_command_buffer` and resource builders a
/// windowed `VulkanRenderer` uses; only the target — one fixed offscreen image instead of a
/// swapchain image — differs.
pub struct OffscreenRenderer {
    renderer: VulkanRenderer,
}

impl OffscreenRenderer {
    /// Builds a headless renderer targeting one `width`x`height` `R8G8B8A8_UNORM` image (the
    /// authored surface extent — no DPI, no scaling).
    pub fn new(
        app_name: &str,
        text_layout: ScreenTextLayout,
        bindings: ScreenBindings,
        width: u32,
        height: u32,
    ) -> Result<Self, BoxError> {
        let renderer = VulkanRenderer::new_offscreen(app_name, text_layout, bindings, width, height)?;
        Ok(Self { renderer })
    }

    /// Renders one frame into the offscreen target: writes this frame's realtime/interaction
    /// state through the same mapped-buffer writes a windowed frame uses, records the command
    /// buffer, then submits and waits synchronously (there is no swapchain present to pace on).
    pub fn draw_frame(
        &mut self,
        inputs: &FrameInputs,
        clock: WallClock,
        interaction: InteractionSnapshot,
    ) -> Result<(), BoxError> {
        self.renderer.draw_frame_offscreen(inputs, clock, interaction)
    }

    /// Reads the last rendered frame back as tightly packed RGBA8 via a one-shot
    /// image-to-buffer copy.
    pub fn read_pixels(&mut self) -> Result<CapturedFrame, BoxError> {
        let (width, height) = self.renderer.current_extent();
        let rgba = self.renderer.read_offscreen_pixels()?;
        Ok(CapturedFrame {
            width,
            height,
            rgba,
        })
    }

    /// The Vulkan device name this offscreen renderer picked (e.g. `llvmpipe (LLVM ...)` for
    /// lavapipe, or the MoltenVK-reported GPU name on macOS).
    pub fn device_name(&self) -> &str {
        self.renderer.device_name()
    }
}
