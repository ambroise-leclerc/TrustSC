//! Raw Vulkan 1.0 renderer for the `trustsc-vulkan-winit` presentation adapter (ADR-005/ADR-012 edge
//! adapter: `unsafe` and native Vulkan handles are confined to this module, never crossing into a
//! governed crate's public API). Renders one swapchain-filling clear color plus a single alpha-atlas
//! text overlay built from a [`trustsc::screen_text::ScreenTextLayout`].

use std::{
    error::Error,
    ffi::CString,
    io::Cursor,
    mem::{size_of, size_of_val},
    ptr,
};

use ash::{khr, util::read_spv, vk, Entry, Instance};
use trustsc::realtime::{FrameInputs, ScreenBindings};
use trustsc::screen_text::ScreenTextLayout;
use trustsc::TextRuntime;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

pub type BoxError = Box<dyn Error>;

const TEXT_VERT_SPV: &[u8] = include_bytes!("../shaders/generated/hello_text.vert.spv");
const TEXT_FRAG_SPV: &[u8] = include_bytes!("../shaders/generated/hello_text.frag.spv");
const HEIGHTFIELD_VERT_SPV: &[u8] = include_bytes!("../shaders/generated/heightfield.vert.spv");
const HEIGHTFIELD_FRAG_SPV: &[u8] = include_bytes!("../shaders/generated/heightfield.frag.spv");
const FLAT_VERT_SPV: &[u8] = include_bytes!("../shaders/generated/flat.vert.spv");
const FLAT_FRAG_SPV: &[u8] = include_bytes!("../shaders/generated/flat.frag.spv");
const IMAGE_VERT_SPV: &[u8] = include_bytes!("../shaders/generated/image.vert.spv");
const IMAGE_FRAG_SPV: &[u8] = include_bytes!("../shaders/generated/image.frag.spv");

/// Vertical exaggeration applied to the waterfall's normalized 0..1 samples.
const WATERFALL_HEIGHT_SCALE: f32 = 0.45;

/// Offscreen render target format (ADR-016 §1): straight RGBA, no shader color conversion, so a
/// solid fill's expected byte is exactly `round(255 * token_float)`.
const OFFSCREEN_COLOR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const BYTES_PER_PIXEL: u32 = 4;
/// Name recorded in verification evidence (ADR-016 §5) for [`OFFSCREEN_COLOR_FORMAT`].
pub(crate) const OFFSCREEN_PIXEL_FORMAT_NAME: &str = "R8G8B8A8_UNORM";

/// The render pass's color attachment clear value: every pixel a node's chrome does not cover
/// renders exactly this color, so `trustsc-ui-verify`'s checks (ADR-016 §2) can tell "no ink here"
/// apart from "unreadable frame" by comparing against it.
const CLEAR_COLOR_RGBA_F32: [f32; 4] = [0.12, 0.18, 0.35, 1.0];

/// [`CLEAR_COLOR_RGBA_F32`] rounded to the exact bytes the offscreen target renders — the
/// verification path's expected background color.
pub(crate) fn clear_color_bytes() -> [u8; 4] {
    [
        (CLEAR_COLOR_RGBA_F32[0] * 255.0).round() as u8,
        (CLEAR_COLOR_RGBA_F32[1] * 255.0).round() as u8,
        (CLEAR_COLOR_RGBA_F32[2] * 255.0).round() as u8,
        (CLEAR_COLOR_RGBA_F32[3] * 255.0).round() as u8,
    ]
}

#[derive(Clone, Copy)]
struct QueueFamilies {
    graphics: u32,
    present: u32,
}

struct SwapchainSupport {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}

#[derive(Default)]
struct TextAtlasResources {
    image: vk::Image,
    memory: vk::DeviceMemory,
    image_view: vk::ImageView,
    sampler: vk::Sampler,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct TextVertex {
    position: [f32; 2],
    tex_coord: [f32; 2],
}

impl TextVertex {
    const fn new(position: [f32; 2], tex_coord: [f32; 2]) -> Self {
        Self {
            position,
            tex_coord,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TextPushConstants {
    transform: [[f32; 4]; 4],
    text_color: [f32; 4],
}

impl TextPushConstants {
    const fn overlay() -> Self {
        Self {
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            text_color: [0.97, 0.98, 1.0, 1.0],
        }
    }
}

/// Must match the push-constant block of `heightfield.vert` (80 bytes: column-major MVP then
/// four floats).
#[repr(C)]
#[derive(Clone, Copy)]
struct HeightfieldPushConstants {
    mvp: [[f32; 4]; 4],
    rows: f32,
    cols: f32,
    row_offset: f32,
    height_scale: f32,
}

/// GPU-side state of one streaming viewport's waterfall: a static index grid over a
/// persistently mapped height array (the ring buffer's mirror), plus the fixed camera.
struct WaterfallResources {
    source: &'static str,
    bounds: trustsc::Rect,
    rows: u32,
    bins: u32,
    height_buffer: vk::Buffer,
    height_buffer_memory: vk::DeviceMemory,
    height_ptr: Option<std::ptr::NonNull<f32>>,
    index_buffer: vk::Buffer,
    index_buffer_memory: vk::DeviceMemory,
    index_count: u32,
    mvp: [[f32; 4]; 4],
    /// Ring cursor of the last written frame, handed to the shader as `row_offset`.
    row_offset: u32,
}

/// Vertex of the flat solid-color pipeline (Panel underlays): NDC position + straight RGBA.
#[repr(C)]
#[derive(Clone, Copy)]
struct FlatVertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// GPU-side state of one `SignalTrace` node (ADR-018): a persistently mapped `FlatVertex` ring
/// mirror drawn as a `LINE_STRIP` — the scrolling-amplitude counterpart of `WaterfallResources`.
/// Reuses the flat solid-color shaders (no new GLSL): the vertex format and shading are
/// identical to a panel quad, only the pipeline's topology differs.
struct TraceResources {
    source: &'static str,
    bounds: trustsc::Rect,
    rgba: [f32; 4],
    capacity: usize,
    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,
    vertex_ptr: Option<std::ptr::NonNull<FlatVertex>>,
}

/// One governed image's GPU resources: its RGBA texture + descriptor set (uploaded once at
/// construction) and its quad vertex buffer (kept in the swapchain lifecycle for consistent
/// teardown/rebuild ordering).
struct ImageResources {
    bounds: trustsc::Rect,
    texture: TextAtlasResources,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,
}

/// One approved display package's GPU text resources and its fixed vertex range in the
/// persistently mapped dynamic buffer (ADR-013/ADR-014).
struct DisplayTextResources {
    atlas: TextAtlasResources,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    /// First vertex of this package's range in the dynamic buffer.
    vertex_offset: usize,
    /// Fixed capacity of the range, in vertices.
    capacity_vertices: usize,
    /// Vertices actually written this frame (reset and rewritten by write_dynamic_vertices,
    /// read by record_command_buffer — no per-frame allocation).
    written_vertices: u32,
}

/// One `StatusIndicator` node's fixed vertex range in the persistently mapped dynamic buffer
/// (mirrors `DisplayTextResources`, minus the atlas/descriptor set: statuses render from the
/// standard package's atlas, same as clocks/text-inputs). `active_rgba` is resolved from
/// `StatusBinding::color_tokens[state_index]` each frame in `write_dynamic_vertices` — giving
/// each status its own dedicated range and push-constant color is what makes the per-state
/// theme color (e.g. green NORMAL / red ARRHYTHMIA) actually render, instead of every dynamic
/// glyph sharing one fixed overlay color.
struct StatusTextResources {
    vertex_offset: usize,
    capacity_vertices: usize,
    written_vertices: u32,
    active_rgba: [f32; 4],
}

impl DisplayTextResources {
    fn empty(vertex_offset: usize, capacity_vertices: usize) -> Self {
        Self {
            atlas: TextAtlasResources::default(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_set: vk::DescriptorSet::null(),
            vertex_offset,
            capacity_vertices,
            written_vertices: 0,
        }
    }
}

/// Sums each `NumberBinding`'s quad capacity into its display package's slot.
/// `ScreenBindings::from_screen` only ever produces `display_index` values in range, but
/// `VulkanRenderer::new` accepts a `ScreenBindings` directly — a hand-constructed or
/// otherwise-modified instance could carry an out-of-range index. This fails with a typed error
/// instead of an index-out-of-bounds panic.
fn accumulate_display_quads(
    numbers: &[trustsc::realtime::NumberBinding],
    display_count: usize,
) -> Result<Vec<usize>, BoxError> {
    let mut per_display_quads = vec![0usize; display_count];
    for binding in numbers {
        let quads = per_display_quads.get_mut(binding.display_index).ok_or_else(|| {
            box_error(format!(
                "numeric display {} has display_index {} but only {display_count} display packages are bound",
                binding.node_id, binding.display_index
            ))
        })?;
        *quads += binding.capacity;
    }
    Ok(per_display_quads)
}

/// Computes the fixed `[standard | s0 | s1 | ... | d0 | d1 | ...]` split of the dynamic vertex
/// buffer from the per-range quad counts (6 vertices per glyph quad): the standard range
/// (clocks/text-inputs, one shared overlay color), then one dedicated range per
/// `StatusIndicator` (ADR-018's per-state color fix needs its own push-constant color per
/// status, hence its own range), then one per display package. Returns the standard range's
/// capacity, each status's `(offset, capacity)`, each display package's `(offset, capacity)`,
/// and the total vertex count.
fn dynamic_buffer_layout(
    standard_quads: usize,
    per_status_quads: &[usize],
    per_display_quads: &[usize],
) -> (usize, Vec<(usize, usize)>, Vec<(usize, usize)>, usize) {
    let standard_capacity = standard_quads * 6;
    let mut offset = standard_capacity;
    let mut status_ranges = Vec::with_capacity(per_status_quads.len());
    for &quads in per_status_quads {
        let capacity = quads * 6;
        status_ranges.push((offset, capacity));
        offset += capacity;
    }
    let mut display_ranges = Vec::with_capacity(per_display_quads.len());
    for &quads in per_display_quads {
        let capacity = quads * 6;
        display_ranges.push((offset, capacity));
        offset += capacity;
    }
    (standard_capacity, status_ranges, display_ranges, offset)
}

pub struct VulkanRenderer {
    // Owns the dynamically loaded libvulkan (ash's `loaded` feature dlopens it; dropping the
    // `Entry` dlcloses it). Most device-level calls resolve to ICD entry points and keep working
    // after an early dlclose, but `vkDestroyDevice`, `vkDestroyInstance`, and the surface calls
    // route through loader trampolines inside libvulkan itself — calling them after the library
    // is unmapped segfaults (issue #28). The `Entry` must therefore outlive every other field.
    _entry: Entry,
    instance: Instance,
    surface_loader: khr::surface::Instance,
    // `None` for an offscreen-constructed renderer (ADR-016 §1): there is no window, no
    // presentable surface, and every present-support filter and swapchain call is skipped.
    surface: Option<vk::SurfaceKHR>,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    queue_families: QueueFamilies,
    swapchain_loader: khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_image_views: Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    in_flight_fence: vk::Fence,
    device_name: String,
    text_layout: ScreenTextLayout,
    text_atlas: TextAtlasResources,
    text_descriptor_set_layout: vk::DescriptorSetLayout,
    text_descriptor_pool: vk::DescriptorPool,
    text_descriptor_set: vk::DescriptorSet,
    text_pipeline_layout: vk::PipelineLayout,
    text_pipeline: vk::Pipeline,
    text_vertex_buffer: vk::Buffer,
    text_vertex_buffer_memory: vk::DeviceMemory,
    text_vertex_count: u32,
    // Realtime bindings (ADR-013): capacities fixed at construction, per-frame work limited to
    // rewriting the persistently mapped dynamic buffer and re-recording command buffers.
    bindings: ScreenBindings,
    // Layout: [standard quads | display-0 quads | display-1 quads | ...], fixed split computed
    // once by `dynamic_buffer_layout`. Null/None when the screen has no dynamic text.
    dynamic_vertex_buffer: vk::Buffer,
    dynamic_vertex_buffer_memory: vk::DeviceMemory,
    dynamic_vertex_ptr: Option<std::ptr::NonNull<TextVertex>>,
    dynamic_standard_capacity_vertices: usize,
    // One entry per `StatusIndicator` node (index-aligned with `bindings.statuses`), each its
    // own fixed range of the dynamic buffer so its active state's theme color can be pushed
    // independently (ADR-018 per-state color fix).
    status_resources: Vec<StatusTextResources>,
    // One entry per approved display package (index-aligned with `bindings.displays`); each
    // carries its own glyph atlas + descriptor set (the pipeline layout is shared — identically
    // defined set layouts are compatible) and its fixed range of the dynamic buffer.
    display_resources: Vec<DisplayTextResources>,
    // Depth attachment (recreated with the swapchain), required by the 3D waterfall pipeline.
    depth_image: vk::Image,
    depth_image_memory: vk::DeviceMemory,
    depth_image_view: vk::ImageView,
    depth_format: vk::Format,
    current_extent: vk::Extent2D,
    // 3D DSA waterfalls, one per streaming viewport; pipeline recreated with the swapchain.
    waterfalls: Vec<WaterfallResources>,
    waterfall_pipeline_layout: vk::PipelineLayout,
    waterfall_pipeline: vk::Pipeline,
    // Panel underlays (ADR-014): one static vertex buffer for ALL panels, one draw, drawn
    // first. Pipeline layout is empty (no descriptors, no push constants).
    flat_pipeline_layout: vk::PipelineLayout,
    flat_pipeline: vk::Pipeline,
    flat_vertex_buffer: vk::Buffer,
    flat_vertex_buffer_memory: vk::DeviceMemory,
    flat_vertex_count: u32,
    // Governed images (ADR-014): textures uploaded once; quads drawn after the waterfalls,
    // before the text overlay.
    images: Vec<ImageResources>,
    image_pipeline_layout: vk::PipelineLayout,
    image_pipeline: vk::Pipeline,
    // Signal traces (ADR-018): one persistently mapped LINE_STRIP vertex buffer per node,
    // rewritten from the realtime sample ring each frame; the pipeline reuses the flat
    // solid-color shaders with a different topology (no new GLSL).
    traces: Vec<TraceResources>,
    trace_pipeline_layout: vk::PipelineLayout,
    trace_pipeline: vk::Pipeline,
    authored_surface_extent: vk::Extent2D,
    // Interactive widget chrome (ADR-015): button faces, text-input fields and the caret — a
    // small persistently mapped flat-rect region rewritten each frame through the same flat
    // pipeline as the panel underlays, drawn between the images and the text overlay.
    interactive_rect_buffer: vk::Buffer,
    interactive_rect_memory: vk::DeviceMemory,
    interactive_rect_ptr: Option<std::ptr::NonNull<FlatVertex>>,
    interactive_rect_capacity_vertices: usize,
    interactive_rect_written_vertices: u32,
    interactive_rect_staging: Vec<FlatVertex>,
    // Per-text-input chrome, derived once from the theme table (ADR-015 §6): no color is
    // computed per frame.
    text_input_chrome: Vec<TextInputChrome>,
    // Offscreen render target (ADR-016 §1): the single color image an offscreen-constructed
    // renderer renders into instead of a swapchain image. Null for a windowed renderer.
    offscreen_image: vk::Image,
    offscreen_image_memory: vk::DeviceMemory,
    offscreen_image_view: vk::ImageView,
}

/// Which widget renders pressed/focused this frame, and where the caret sits — the event
/// loop's presentation snapshot handed to [`VulkanRenderer::draw_frame`] (ADR-015 §6).
#[derive(Clone, Copy, Debug, Default)]
pub struct InteractionSnapshot {
    /// Index into the screen's button bindings whose face renders with the pressed tint.
    pub pressed_button: Option<usize>,
    /// Index into the screen's text-input bindings that renders focused (field highlight +
    /// caret).
    pub focused_input: Option<usize>,
    /// Caret position (character index) inside the focused input's echoed content.
    pub caret: u16,
}

/// A text input's render chrome, resolved once at construction from the governed theme table.
struct TextInputChrome {
    field_rgba: [f32; 4],
    focused_field_rgba: [f32; 4],
    caret_rgba: [f32; 4],
    caret_height: u32,
}

/// Every already-created Vulkan/runtime object [`VulkanRenderer::new`] and
/// [`VulkanRenderer::new_offscreen`] hand to [`VulkanRenderer::assemble`] — the single place that
/// fills in the struct's remaining null/empty sentinels, shared by both constructors so the two
/// paths cannot drift on a field neither of them explicitly sets.
struct AssembledDevice {
    entry: Entry,
    instance: Instance,
    surface_loader: khr::surface::Instance,
    surface: Option<vk::SurfaceKHR>,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    queue_families: QueueFamilies,
    swapchain_loader: khr::swapchain::Device,
    command_pool: vk::CommandPool,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    in_flight_fence: vk::Fence,
    device_name: String,
    text_layout: ScreenTextLayout,
    bindings: ScreenBindings,
    depth_format: vk::Format,
    authored_surface_width: u32,
    authored_surface_height: u32,
}

impl VulkanRenderer {
    pub fn new(
        window: &Window,
        app_name: &str,
        text_layout: ScreenTextLayout,
        bindings: ScreenBindings,
        authored_surface_width: u32,
        authored_surface_height: u32,
    ) -> Result<Self, BoxError> {
        let entry = unsafe { Entry::load()? };
        let instance = create_instance(&entry, Some(window), app_name)?;
        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )?
        };
        let surface_loader = khr::surface::Instance::new(&entry, &instance);
        let (physical_device, queue_families, device_name) =
            pick_physical_device(&instance, &surface_loader, Some(surface))?;
        let (device, graphics_queue, present_queue) =
            create_logical_device(&instance, physical_device, queue_families, true)?;
        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);
        let command_pool = create_command_pool(&device, queue_families.graphics)?;
        let (image_available_semaphore, render_finished_semaphore, in_flight_fence) =
            create_sync_objects(&device)?;

        // Validate both realtime packages once, here: the frame loop then uses
        // `TextRuntime::from_validated_package`, which must not re-run (allocating) validation.
        TextRuntime::<1>::new(&bindings.standard)?;
        for display in &bindings.displays {
            TextRuntime::<1>::new(display)?;
        }
        let depth_format = find_depth_format(&instance, physical_device)?;

        let mut renderer = Self::assemble(AssembledDevice {
            entry,
            instance,
            surface_loader,
            surface: Some(surface),
            physical_device,
            device,
            graphics_queue,
            present_queue,
            queue_families,
            swapchain_loader,
            command_pool,
            image_available_semaphore,
            render_finished_semaphore,
            in_flight_fence,
            device_name,
            text_layout,
            bindings,
            depth_format,
            authored_surface_width,
            authored_surface_height,
        });

        renderer.create_text_static_resources()?;
        renderer.create_dynamic_text_resources()?;
        renderer.create_waterfall_resources()?;
        renderer.create_panel_and_image_static_resources()?;
        renderer.create_interactive_rect_resources()?;
        renderer.create_trace_resources()?;
        renderer.recreate_swapchain(window)?;
        Ok(renderer)
    }

    /// Builds an [`OffscreenRenderer`]-backing renderer (ADR-016 §1): a headless instance (no
    /// `ash_window` WSI extensions, no surface), a device picked for a graphics queue only (no
    /// present-support filter), and one `R8G8B8A8_UNORM` color image at `width`x`height` — the
    /// authored surface extent — instead of a swapchain. Every resource builder and
    /// `record_command_buffer` are the exact same code path presented frames use; only the
    /// target (swapchain image vs. this single offscreen image) differs.
    pub fn new_offscreen(
        app_name: &str,
        text_layout: ScreenTextLayout,
        bindings: ScreenBindings,
        authored_surface_width: u32,
        authored_surface_height: u32,
    ) -> Result<Self, BoxError> {
        let entry = unsafe { Entry::load()? };
        let instance = create_instance(&entry, None, app_name)?;
        let surface_loader = khr::surface::Instance::new(&entry, &instance);
        let (physical_device, queue_families, device_name) =
            pick_physical_device(&instance, &surface_loader, None)?;
        let (device, graphics_queue, present_queue) =
            create_logical_device(&instance, physical_device, queue_families, false)?;
        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);
        let command_pool = create_command_pool(&device, queue_families.graphics)?;
        let (image_available_semaphore, render_finished_semaphore, in_flight_fence) =
            create_sync_objects(&device)?;

        TextRuntime::<1>::new(&bindings.standard)?;
        for display in &bindings.displays {
            TextRuntime::<1>::new(display)?;
        }
        let depth_format = find_depth_format(&instance, physical_device)?;

        let mut renderer = Self::assemble(AssembledDevice {
            entry,
            instance,
            surface_loader,
            surface: None,
            physical_device,
            device,
            graphics_queue,
            present_queue,
            queue_families,
            swapchain_loader,
            command_pool,
            image_available_semaphore,
            render_finished_semaphore,
            in_flight_fence,
            device_name,
            text_layout,
            bindings,
            depth_format,
            authored_surface_width,
            authored_surface_height,
        });

        renderer.create_text_static_resources()?;
        renderer.create_dynamic_text_resources()?;
        renderer.create_waterfall_resources()?;
        renderer.create_panel_and_image_static_resources()?;
        renderer.create_interactive_rect_resources()?;
        renderer.create_trace_resources()?;
        renderer.create_offscreen_target()?;
        Ok(renderer)
    }

    /// Assembles the struct literal shared by the windowed and offscreen constructors: every
    /// field not yet meaningful before the first swapchain/offscreen-target build gets its null
    /// or empty sentinel here, exactly once.
    fn assemble(built: AssembledDevice) -> Self {
        Self {
            _entry: built.entry,
            instance: built.instance,
            surface_loader: built.surface_loader,
            surface: built.surface,
            physical_device: built.physical_device,
            device: built.device,
            graphics_queue: built.graphics_queue,
            present_queue: built.present_queue,
            queue_families: built.queue_families,
            swapchain_loader: built.swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_image_views: Vec::new(),
            render_pass: vk::RenderPass::null(),
            framebuffers: Vec::new(),
            command_pool: built.command_pool,
            command_buffers: Vec::new(),
            image_available_semaphore: built.image_available_semaphore,
            render_finished_semaphore: built.render_finished_semaphore,
            in_flight_fence: built.in_flight_fence,
            device_name: built.device_name,
            text_layout: built.text_layout,
            text_atlas: TextAtlasResources::default(),
            text_descriptor_set_layout: vk::DescriptorSetLayout::null(),
            text_descriptor_pool: vk::DescriptorPool::null(),
            text_descriptor_set: vk::DescriptorSet::null(),
            text_pipeline_layout: vk::PipelineLayout::null(),
            text_pipeline: vk::Pipeline::null(),
            text_vertex_buffer: vk::Buffer::null(),
            text_vertex_buffer_memory: vk::DeviceMemory::null(),
            text_vertex_count: 0,
            bindings: built.bindings,
            dynamic_vertex_buffer: vk::Buffer::null(),
            dynamic_vertex_buffer_memory: vk::DeviceMemory::null(),
            dynamic_vertex_ptr: None,
            dynamic_standard_capacity_vertices: 0,
            status_resources: Vec::new(),
            display_resources: Vec::new(),
            depth_image: vk::Image::null(),
            depth_image_memory: vk::DeviceMemory::null(),
            depth_image_view: vk::ImageView::null(),
            depth_format: built.depth_format,
            current_extent: vk::Extent2D {
                width: 0,
                height: 0,
            },
            waterfalls: Vec::new(),
            waterfall_pipeline_layout: vk::PipelineLayout::null(),
            waterfall_pipeline: vk::Pipeline::null(),
            flat_pipeline_layout: vk::PipelineLayout::null(),
            flat_pipeline: vk::Pipeline::null(),
            flat_vertex_buffer: vk::Buffer::null(),
            flat_vertex_buffer_memory: vk::DeviceMemory::null(),
            flat_vertex_count: 0,
            images: Vec::new(),
            image_pipeline_layout: vk::PipelineLayout::null(),
            image_pipeline: vk::Pipeline::null(),
            traces: Vec::new(),
            trace_pipeline_layout: vk::PipelineLayout::null(),
            trace_pipeline: vk::Pipeline::null(),
            authored_surface_extent: vk::Extent2D {
                width: built.authored_surface_width.max(1),
                height: built.authored_surface_height.max(1),
            },
            interactive_rect_buffer: vk::Buffer::null(),
            interactive_rect_memory: vk::DeviceMemory::null(),
            interactive_rect_ptr: None,
            interactive_rect_capacity_vertices: 0,
            interactive_rect_written_vertices: 0,
            interactive_rect_staging: Vec::new(),
            text_input_chrome: Vec::new(),
            offscreen_image: vk::Image::null(),
            offscreen_image_memory: vk::DeviceMemory::null(),
            offscreen_image_view: vk::ImageView::null(),
        }
    }

    fn authored_surface_size(&self) -> (f32, f32) {
        (
            self.authored_surface_extent.width as f32,
            self.authored_surface_extent.height as f32,
        )
    }

    fn scale_bounds_to_current_extent(&self, bounds: trustsc::Rect) -> trustsc::Rect {
        scale_rect_to_extent(bounds, self.authored_surface_extent, self.current_extent)
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// The renderer's current target extent (window swapchain extent, or the offscreen image's
    /// fixed authored extent).
    pub(crate) fn current_extent(&self) -> (u32, u32) {
        (self.current_extent.width, self.current_extent.height)
    }

    pub fn draw_frame(
        &mut self,
        window: &Window,
        inputs: &FrameInputs,
        clock: WallClock,
        interaction: InteractionSnapshot,
    ) -> Result<(), BoxError> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }

        unsafe {
            self.device
                .wait_for_fences(&[self.in_flight_fence], true, u64::MAX)?;
        }

        let acquire_result = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available_semaphore,
                vk::Fence::null(),
            )
        };

        let (image_index, suboptimal) = match acquire_result {
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain(window)?;
                return Ok(());
            }
            Err(error) => {
                return Err(box_error(format!(
                    "failed to acquire swapchain image: {error}"
                )));
            }
        };

        // The in-flight fence has been waited on: the previous submission using this command
        // buffer and the mapped buffers has fully completed, so all are safe to rewrite.
        self.write_waterfall_heights(inputs)?;
        self.write_traces(inputs)?;
        let dynamic_standard_vertices = self.write_dynamic_vertices(inputs, clock)?;
        self.write_interactive_rects(inputs, interaction)?;
        self.record_command_buffer(image_index as usize, dynamic_standard_vertices)?;

        let wait_semaphores = [self.image_available_semaphore];
        let signal_semaphores = [self.render_finished_semaphore];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [self.command_buffers[image_index as usize]];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);

        unsafe {
            self.device.reset_fences(&[self.in_flight_fence])?;
            self.device
                .queue_submit(self.graphics_queue, &[submit_info], self.in_flight_fence)?;
        }

        let swapchains = [self.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_result = unsafe {
            self.swapchain_loader
                .queue_present(self.present_queue, &present_info)
        };

        match present_result {
            Ok(is_suboptimal) => {
                if suboptimal || is_suboptimal {
                    self.recreate_swapchain(window)?;
                }
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.recreate_swapchain(window)?,
            Err(error) => return Err(box_error(format!("failed to present frame: {error}"))),
        }

        Ok(())
    }

    fn recreate_swapchain(&mut self, window: &Window) -> Result<(), BoxError> {
        unsafe {
            self.device.device_wait_idle()?;
        }

        self.destroy_swapchain_objects();

        let surface = self
            .surface
            .expect("recreate_swapchain is only called for a windowed renderer");
        let support = query_swapchain_support(self.physical_device, &self.surface_loader, surface)?;
        let surface_format = choose_surface_format(&support.formats)?;
        let present_mode = choose_present_mode(&support.present_modes);
        let extent = choose_extent(&support.capabilities, window);

        let mut image_count = support.capabilities.min_image_count + 1;
        if support.capabilities.max_image_count > 0 {
            image_count = image_count.min(support.capabilities.max_image_count);
        }

        let family_indices = [self.queue_families.graphics, self.queue_families.present];
        let (sharing_mode, queue_family_indices) =
            if self.queue_families.graphics != self.queue_families.present {
                (vk::SharingMode::CONCURRENT, &family_indices[..])
            } else {
                (vk::SharingMode::EXCLUSIVE, &family_indices[0..1])
            };

        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(image_count)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(sharing_mode)
            .queue_family_indices(queue_family_indices)
            .pre_transform(support.capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);

        self.swapchain = unsafe { self.swapchain_loader.create_swapchain(&create_info, None)? };
        let images = unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain)? };

        self.swapchain_image_views = images
            .iter()
            .map(|&image| {
                create_image_view(
                    &self.device,
                    image,
                    surface_format.format,
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.render_pass = create_render_pass(
            &self.device,
            surface_format.format,
            self.depth_format,
            vk::ImageLayout::PRESENT_SRC_KHR,
        )?;
        self.create_depth_resources(extent)?;
        self.framebuffers = self
            .swapchain_image_views
            .iter()
            .map(|&image_view| {
                create_framebuffer(
                    &self.device,
                    self.render_pass,
                    image_view,
                    self.depth_image_view,
                    extent,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.create_text_swapchain_resources(extent)?;
        self.create_panel_and_image_swapchain_resources()?;
        self.command_buffers = allocate_command_buffers(
            &self.device,
            self.command_pool,
            self.framebuffers.len() as u32,
        )?;
        self.current_extent = extent;
        // Command buffers are recorded per frame in draw_frame (ADR-013 realtime contract), not
        // here: the first draw after a swapchain (re)creation records before submitting.

        Ok(())
    }

    /// Builds the offscreen counterpart of `recreate_swapchain`: one `R8G8B8A8_UNORM` color
    /// image (`COLOR_ATTACHMENT | TRANSFER_SRC`) at the authored surface extent instead of a
    /// swapchain, whose final layout is `TRANSFER_SRC_OPTIMAL` so `read_offscreen_pixels` needs
    /// no extra transition before its copy. Runs once — there is no resize to react to.
    fn create_offscreen_target(&mut self) -> Result<(), BoxError> {
        let extent = self.authored_surface_extent;

        let (image, memory) = create_image(
            &self.instance,
            &self.device,
            self.physical_device,
            extent.width,
            extent.height,
            OFFSCREEN_COLOR_FORMAT,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let image_view = create_image_view(
            &self.device,
            image,
            OFFSCREEN_COLOR_FORMAT,
            vk::ImageAspectFlags::COLOR,
        )?;
        self.offscreen_image = image;
        self.offscreen_image_memory = memory;
        self.offscreen_image_view = image_view;

        self.render_pass = create_render_pass(
            &self.device,
            OFFSCREEN_COLOR_FORMAT,
            self.depth_format,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        )?;
        self.create_depth_resources(extent)?;
        self.framebuffers = vec![create_framebuffer(
            &self.device,
            self.render_pass,
            self.offscreen_image_view,
            self.depth_image_view,
            extent,
        )?];

        self.create_text_swapchain_resources(extent)?;
        self.create_panel_and_image_swapchain_resources()?;
        self.command_buffers = allocate_command_buffers(&self.device, self.command_pool, 1)?;
        self.current_extent = extent;

        Ok(())
    }

    /// Renders one frame into the offscreen target (ADR-016 §1): the same per-frame writes and
    /// `record_command_buffer` presented frames use, submitted without semaphores (there is no
    /// swapchain image to synchronize against) and waited synchronously — the next
    /// `read_offscreen_pixels` call can assume the GPU is idle.
    pub(crate) fn draw_frame_offscreen(
        &mut self,
        inputs: &FrameInputs,
        clock: WallClock,
        interaction: InteractionSnapshot,
    ) -> Result<(), BoxError> {
        unsafe {
            self.device
                .wait_for_fences(&[self.in_flight_fence], true, u64::MAX)?;
        }

        self.write_waterfall_heights(inputs)?;
        self.write_traces(inputs)?;
        let dynamic_standard_vertices = self.write_dynamic_vertices(inputs, clock)?;
        self.write_interactive_rects(inputs, interaction)?;
        self.record_command_buffer(0, dynamic_standard_vertices)?;

        let command_buffers = [self.command_buffers[0]];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

        unsafe {
            self.device.reset_fences(&[self.in_flight_fence])?;
            self.device
                .queue_submit(self.graphics_queue, &[submit_info], self.in_flight_fence)?;
            self.device
                .wait_for_fences(&[self.in_flight_fence], true, u64::MAX)?;
        }

        Ok(())
    }

    /// One-shot image-to-buffer copy of the offscreen color image into a tightly packed RGBA
    /// byte vector (ADR-016 §1): a fresh `TRANSFER_DST | HOST_VISIBLE | HOST_COHERENT` staging
    /// buffer is created, filled through `copy_image_to_buffer`, mapped, copied out and freed —
    /// there is no per-frame readback buffer to keep alive between captures.
    pub(crate) fn read_offscreen_pixels(&mut self) -> Result<Vec<u8>, BoxError> {
        let extent = self.current_extent;
        let byte_count = vk::DeviceSize::from(extent.width)
            * vk::DeviceSize::from(extent.height)
            * vk::DeviceSize::from(BYTES_PER_PIXEL);

        let (staging_buffer, staging_memory) = create_buffer(
            &self.instance,
            &self.device,
            self.physical_device,
            byte_count,
            vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        // Both the copy and the map can fail; every exit path from here must still free the
        // staging buffer/memory, so the fallible work happens in one expression and cleanup runs
        // unconditionally afterward rather than via early `?` returns that would leak them.
        let result = copy_image_to_buffer(
            &self.device,
            self.command_pool,
            self.graphics_queue,
            self.offscreen_image,
            staging_buffer,
            extent.width,
            extent.height,
        )
        .and_then(|()| unsafe {
            let mapped = self.device.map_memory(
                staging_memory,
                0,
                byte_count,
                vk::MemoryMapFlags::empty(),
            )?;
            let mut rgba = vec![0u8; byte_count as usize];
            ptr::copy_nonoverlapping(mapped.cast::<u8>(), rgba.as_mut_ptr(), byte_count as usize);
            self.device.unmap_memory(staging_memory);
            Ok(rgba)
        });

        unsafe {
            self.device.destroy_buffer(staging_buffer, None);
            self.device.free_memory(staging_memory, None);
        }

        result
    }

    fn create_depth_resources(&mut self, extent: vk::Extent2D) -> Result<(), BoxError> {
        let (image, memory) = create_image(
            &self.instance,
            &self.device,
            self.physical_device,
            extent.width,
            extent.height,
            self.depth_format,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;
        let view = create_image_view(
            &self.device,
            image,
            self.depth_format,
            depth_aspect_mask(self.depth_format),
        )?;
        self.depth_image = image;
        self.depth_image_memory = memory;
        self.depth_image_view = view;
        Ok(())
    }

    fn create_text_static_resources(&mut self) -> Result<(), BoxError> {
        self.text_atlas = create_text_atlas_resources(
            &self.instance,
            &self.device,
            self.physical_device,
            self.command_pool,
            self.graphics_queue,
            &self.text_layout.package,
        )?;
        let (descriptor_set_layout, descriptor_pool, descriptor_set) =
            create_text_descriptor_resources(&self.device, &self.text_atlas)?;
        self.text_descriptor_set_layout = descriptor_set_layout;
        self.text_descriptor_pool = descriptor_pool;
        self.text_descriptor_set = descriptor_set;
        self.text_pipeline_layout =
            create_text_pipeline_layout(&self.device, self.text_descriptor_set_layout)?;
        Ok(())
    }

    /// Allocates the fixed-capacity dynamic text machinery once: the persistently mapped
    /// vertex buffer sized from the screen's bindings
    /// ([standard quads | per-status quads | display quads]) and, when numeric displays exist,
    /// the display package's atlas + descriptor set. Nothing here runs per frame (ADR-013).
    fn create_dynamic_text_resources(&mut self) -> Result<(), BoxError> {
        let standard_quads: usize = self
            .bindings
            .clocks
            .iter()
            .map(|binding| binding.capacity)
            .chain(
                self.bindings
                    .text_inputs
                    .iter()
                    .map(|binding| binding.capacity),
            )
            .sum();
        let per_status_quads: Vec<usize> = self
            .bindings
            .statuses
            .iter()
            .map(|binding| binding.capacity)
            .collect();
        let per_display_quads =
            accumulate_display_quads(&self.bindings.numbers, self.bindings.displays.len())?;

        // The per-frame renderer uses fixed TextRuntime::<DYNAMIC_RUN_CAPACITY> buffers; every
        // individual binding must fit one render call.
        for (node_id, capacity) in self
            .bindings
            .clocks
            .iter()
            .map(|binding| (binding.node_id, binding.capacity))
            .chain(
                self.bindings
                    .statuses
                    .iter()
                    .map(|binding| (binding.node_id, binding.capacity)),
            )
            .chain(
                self.bindings
                    .numbers
                    .iter()
                    .map(|binding| (binding.node_id, binding.capacity)),
            )
            .chain(
                self.bindings
                    .text_inputs
                    .iter()
                    .map(|binding| (binding.node_id, binding.capacity)),
            )
        {
            if capacity > DYNAMIC_RUN_CAPACITY {
                return Err(box_error(format!(
                    "realtime binding {node_id} needs {capacity} glyphs, above the adapter's per-run capacity of {DYNAMIC_RUN_CAPACITY}"
                )));
            }
        }

        let (standard_capacity, status_ranges, display_ranges, total_vertices) =
            dynamic_buffer_layout(standard_quads, &per_status_quads, &per_display_quads);
        self.dynamic_standard_capacity_vertices = standard_capacity;
        if total_vertices == 0 {
            return Ok(());
        }

        let buffer_size = vk::DeviceSize::try_from(total_vertices * size_of::<TextVertex>())?;
        let (buffer, memory) = create_buffer(
            &self.instance,
            &self.device,
            self.physical_device,
            buffer_size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let mapped = unsafe {
            self.device
                .map_memory(memory, 0, buffer_size, vk::MemoryMapFlags::empty())?
        };
        self.dynamic_vertex_buffer = buffer;
        self.dynamic_vertex_buffer_memory = memory;
        self.dynamic_vertex_ptr = std::ptr::NonNull::new(mapped.cast::<TextVertex>());

        for &(vertex_offset, capacity_vertices) in &status_ranges {
            self.status_resources.push(StatusTextResources {
                vertex_offset,
                capacity_vertices,
                written_vertices: 0,
                active_rgba: [1.0, 1.0, 1.0, 1.0],
            });
        }

        for (display_index, &(vertex_offset, capacity_vertices)) in
            display_ranges.iter().enumerate()
        {
            let mut resources = DisplayTextResources::empty(vertex_offset, capacity_vertices);
            // Packages no numeric display uses get no GPU resources — offset bookkeeping only.
            if capacity_vertices > 0 {
                resources.atlas = create_text_atlas_resources(
                    &self.instance,
                    &self.device,
                    self.physical_device,
                    self.command_pool,
                    self.graphics_queue,
                    &self.bindings.displays[display_index],
                )?;
                let (descriptor_set_layout, descriptor_pool, descriptor_set) =
                    create_text_descriptor_resources(&self.device, &resources.atlas)?;
                resources.descriptor_set_layout = descriptor_set_layout;
                resources.descriptor_pool = descriptor_pool;
                resources.descriptor_set = descriptor_set;
            }
            self.display_resources.push(resources);
        }

        Ok(())
    }

    /// Rewrites the persistently mapped dynamic vertex buffer from the drained frame inputs:
    /// clocks and status states from the standard package (first range), numeric displays each
    /// into their own package's range. Returns the standard range's vertex count; per-display
    /// counts land in `DisplayTextResources::written_vertices` (read by record_command_buffer —
    /// no per-frame allocation). Bounded ArrayVec renders + in-place quad writes only.
    fn write_dynamic_vertices(
        &mut self,
        inputs: &FrameInputs,
        clock: WallClock,
    ) -> Result<u32, BoxError> {
        let Some(pointer) = self.dynamic_vertex_ptr else {
            return Ok(0);
        };
        let (surface_width, surface_height) = self.authored_surface_size();
        let total_vertices = self.dynamic_standard_capacity_vertices
            + self
                .status_resources
                .iter()
                .map(|resources| resources.capacity_vertices)
                .sum::<usize>()
            + self
                .display_resources
                .iter()
                .map(|resources| resources.capacity_vertices)
                .sum::<usize>();
        // Safety: the buffer was mapped once at creation with exactly this vertex capacity, and
        // the in-flight fence waited in draw_frame guarantees the GPU is done reading it.
        let vertices = unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), total_vertices) };

        let standard_runtime =
            TextRuntime::<DYNAMIC_RUN_CAPACITY>::from_validated_package(&self.bindings.standard);

        let mut standard_cursor = 0usize;
        for binding in &self.bindings.clocks {
            match binding.format {
                trustsc::ClockFormat::TimeSeconds => {
                    let commands = standard_runtime.render_clock(
                        &binding.glyph_set_id,
                        clock.hours,
                        clock.minutes,
                        clock.seconds,
                        binding.origin_x,
                        binding.origin_y,
                    )?;
                    write_glyph_quads(
                        vertices,
                        &mut standard_cursor,
                        self.dynamic_standard_capacity_vertices,
                        &commands,
                        &self.bindings.standard,
                        surface_width,
                        surface_height,
                    )?;
                }
                trustsc::ClockFormat::DateTimeSeconds => {
                    let date_commands = standard_runtime.render_date(
                        &binding.glyph_set_id,
                        clock.year,
                        clock.month,
                        clock.day,
                        binding.origin_x,
                        binding.origin_y,
                    )?;
                    write_glyph_quads(
                        vertices,
                        &mut standard_cursor,
                        self.dynamic_standard_capacity_vertices,
                        &date_commands,
                        &self.bindings.standard,
                        surface_width,
                        surface_height,
                    )?;
                    // Time starts after the rendered date plus one space advance.
                    let date_advance = glyph_sequence_advance(
                        &self.bindings.standard,
                        &binding.glyph_set_id,
                        clock,
                    )?;
                    let time_commands = standard_runtime.render_clock(
                        &binding.glyph_set_id,
                        clock.hours,
                        clock.minutes,
                        clock.seconds,
                        binding.origin_x + date_advance,
                        binding.origin_y,
                    )?;
                    write_glyph_quads(
                        vertices,
                        &mut standard_cursor,
                        self.dynamic_standard_capacity_vertices,
                        &time_commands,
                        &self.bindings.standard,
                        surface_width,
                        surface_height,
                    )?;
                }
            }
        }

        // Each status gets its own dedicated vertex range (ADR-018 per-state color fix) so its
        // active state's theme color can be pushed independently at draw time, instead of every
        // status sharing the fixed white overlay color the clocks/text-inputs use.
        for index in 0..self.bindings.statuses.len() {
            let binding = &self.bindings.statuses[index];
            let state_index = usize::from(inputs.status_index(binding.source).unwrap_or(0));
            let run_id = binding.state_run_ids.get(state_index).ok_or_else(|| {
                box_error(format!(
                    "status {} has no run for state {state_index}",
                    binding.node_id
                ))
            })?;
            let color_token = binding.color_tokens.get(state_index).copied().ok_or_else(|| {
                box_error(format!(
                    "status {} has no color for state {state_index}",
                    binding.node_id
                ))
            })?;
            let rgba = trustsc::resolve_color_token(color_token).ok_or_else(|| {
                box_error(format!(
                    "status {} references unknown theme color token {color_token}",
                    binding.node_id
                ))
            })?;
            let (origin_x, origin_y) = binding.state_origins[state_index];
            let commands = standard_runtime.render_run(run_id, origin_x, origin_y)?;

            let (range_offset, range_end) = {
                let resources = &self.status_resources[index];
                (
                    resources.vertex_offset,
                    resources.vertex_offset + resources.capacity_vertices,
                )
            };
            let mut cursor = range_offset;
            write_glyph_quads(
                vertices,
                &mut cursor,
                range_end,
                &commands,
                &self.bindings.standard,
                surface_width,
                surface_height,
            )?;
            self.status_resources[index].written_vertices = (cursor - range_offset) as u32;
            self.status_resources[index].active_rgba = rgba;
        }

        // Text inputs echo application-owned content (ADR-015 controlled component): the
        // renderer draws exactly what the frame handed it and stores nothing.
        for binding in &self.bindings.text_inputs {
            let text = inputs.text(binding.source).unwrap_or("");
            let commands = standard_runtime.render_glyph_set_text(
                &binding.glyph_set_id,
                text,
                binding.origin_x,
                binding.origin_y,
            )?;
            write_glyph_quads(
                vertices,
                &mut standard_cursor,
                self.dynamic_standard_capacity_vertices,
                &commands,
                &self.bindings.standard,
                surface_width,
                surface_height,
            )?;
        }

        for resources in &mut self.display_resources {
            resources.written_vertices = 0;
        }
        for binding in &self.bindings.numbers {
            let display = &self.bindings.displays[binding.display_index];
            let display_runtime =
                TextRuntime::<DYNAMIC_RUN_CAPACITY>::from_validated_package(display);
            let (range_offset, range_end, written_so_far) = {
                let resources = &self.display_resources[binding.display_index];
                (
                    resources.vertex_offset,
                    resources.vertex_offset + resources.capacity_vertices,
                    resources.written_vertices as usize,
                )
            };
            let mut cursor = range_offset + written_so_far;

            let value = inputs.number(binding.source).unwrap_or(0);
            let commands = display_runtime.render_numeric_template(
                &binding.template_id,
                value,
                binding.origin_x,
                binding.origin_y,
            )?;
            write_glyph_quads(
                vertices,
                &mut cursor,
                range_end,
                &commands,
                display,
                surface_width,
                surface_height,
            )?;
            self.display_resources[binding.display_index].written_vertices =
                (cursor - range_offset) as u32;
        }

        Ok(standard_cursor as u32)
    }

    /// Uploads every governed image's RGBA texture and builds the panel/image pipeline layouts
    /// once.
    fn create_panel_and_image_static_resources(&mut self) -> Result<(), BoxError> {
        if !self.bindings.panels.is_empty()
            || !self.bindings.buttons.is_empty()
            || !self.bindings.critical_button_chrome.is_empty()
            || !self.bindings.text_inputs.is_empty()
        {
            let layout_info = vk::PipelineLayoutCreateInfo::default();
            self.flat_pipeline_layout =
                unsafe { self.device.create_pipeline_layout(&layout_info, None)? };
        }

        if self.bindings.images.is_empty() {
            return Ok(());
        }
        for binding in &self.bindings.images {
            let texture = create_sampled_texture(
                &self.instance,
                &self.device,
                self.physical_device,
                self.command_pool,
                self.graphics_queue,
                binding.image.width,
                binding.image.height,
                vk::Format::R8G8B8A8_UNORM,
                &binding.image.pixels,
            )?;
            let (descriptor_set_layout, descriptor_pool, descriptor_set) =
                create_text_descriptor_resources(&self.device, &texture)?;
            self.images.push(ImageResources {
                bounds: binding.bounds,
                texture,
                descriptor_set_layout,
                descriptor_pool,
                descriptor_set,
                vertex_buffer: vk::Buffer::null(),
                vertex_buffer_memory: vk::DeviceMemory::null(),
            });
        }
        // All image descriptor set layouts are identically defined; the shared pipeline layout
        // is created from the first (identically-defined layouts are compatible).
        let set_layouts = [self.images[0].descriptor_set_layout];
        let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        self.image_pipeline_layout =
            unsafe { self.device.create_pipeline_layout(&layout_info, None)? };

        Ok(())
    }

    /// (Re)builds panel/image geometry and pipelines during swapchain lifecycle events.
    fn create_panel_and_image_swapchain_resources(&mut self) -> Result<(), BoxError> {
        let (surface_width, surface_height) = self.authored_surface_size();

        if !self.bindings.panels.is_empty() {
            let mut vertices = Vec::with_capacity(self.bindings.panels.len() * 6);
            for panel in &self.bindings.panels {
                push_flat_quad(
                    &mut vertices,
                    panel.bounds,
                    panel.rgba,
                    surface_width,
                    surface_height,
                );
            }
            let buffer_size = vk::DeviceSize::try_from(std::mem::size_of_val(vertices.as_slice()))?;
            let (buffer, memory) = create_buffer(
                &self.instance,
                &self.device,
                self.physical_device,
                buffer_size,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            write_buffer(&self.device, memory, bytes_of_slice(&vertices))?;
            self.flat_vertex_buffer = buffer;
            self.flat_vertex_buffer_memory = memory;
            self.flat_vertex_count = vertices.len() as u32;
        }

        // The flat pipeline serves both the static panel underlays and the per-frame
        // interactive chrome (ADR-015), so it exists whenever any of those does.
        if !self.bindings.panels.is_empty()
            || !self.bindings.buttons.is_empty()
            || !self.bindings.critical_button_chrome.is_empty()
            || !self.bindings.text_inputs.is_empty()
        {
            self.flat_pipeline =
                create_flat_pipeline(&self.device, self.render_pass, self.flat_pipeline_layout)?;
        }

        if !self.images.is_empty() {
            for index in 0..self.images.len() {
                let bounds = self.images[index].bounds;
                let vertices = image_quad_vertices(bounds, surface_width, surface_height);
                let buffer_size = vk::DeviceSize::try_from(std::mem::size_of_val(&vertices))?;
                let (buffer, memory) = create_buffer(
                    &self.instance,
                    &self.device,
                    self.physical_device,
                    buffer_size,
                    vk::BufferUsageFlags::VERTEX_BUFFER,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )?;
                write_buffer(&self.device, memory, bytes_of_slice(&vertices))?;
                self.images[index].vertex_buffer = buffer;
                self.images[index].vertex_buffer_memory = memory;
            }
            self.image_pipeline =
                create_image_pipeline(&self.device, self.render_pass, self.image_pipeline_layout)?;
        }

        if !self.traces.is_empty() {
            self.trace_pipeline =
                create_trace_pipeline(&self.device, self.render_pass, self.trace_pipeline_layout)?;
        }

        Ok(())
    }

    /// Allocates the interactive-chrome machinery once (ADR-015): a persistently mapped
    /// flat-rect region sized for every button face, every text-input field and one caret, plus
    /// each input's chrome colors derived from the governed theme table. Nothing here runs per
    /// frame.
    fn create_interactive_rect_resources(&mut self) -> Result<(), BoxError> {
        let quad_count = self.bindings.buttons.len()
            + self.bindings.critical_button_chrome.len()
            + self.bindings.text_inputs.len();
        if quad_count == 0 {
            return Ok(());
        }
        // +1 quad for the caret of the focused input.
        let capacity_vertices = (quad_count + 1) * 6;

        for binding in &self.bindings.text_inputs {
            let neutral =
                trustsc::resolve_color_token("Theme.Colors.Neutral").unwrap_or([0.5, 0.5, 0.5, 1.0]);
            let caret_rgba = trustsc::resolve_color_token(binding.color_token)
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let caret_height = self
                .bindings
                .standard
                .find_numeric_glyph_set(&binding.glyph_set_id)
                .map(|glyph_set| {
                    glyph_set
                        .entries
                        .iter()
                        .filter_map(|entry| {
                            self.bindings
                                .standard
                                .find_glyph(entry.atlas_index, entry.glyph_id)
                                .map(|glyph| u32::from(glyph.height))
                        })
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0)
                .max(4);
            self.text_input_chrome.push(TextInputChrome {
                field_rgba: scale_rgb(neutral, 0.35),
                focused_field_rgba: scale_rgb(neutral, 0.55),
                caret_rgba,
                caret_height,
            });
        }

        let buffer_size = vk::DeviceSize::try_from(capacity_vertices * size_of::<FlatVertex>())?;
        let (buffer, memory) = create_buffer(
            &self.instance,
            &self.device,
            self.physical_device,
            buffer_size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        let mapped = unsafe {
            self.device
                .map_memory(memory, 0, buffer_size, vk::MemoryMapFlags::empty())?
        };
        self.interactive_rect_buffer = buffer;
        self.interactive_rect_memory = memory;
        self.interactive_rect_ptr = std::ptr::NonNull::new(mapped.cast::<FlatVertex>());
        self.interactive_rect_capacity_vertices = capacity_vertices;
        self.interactive_rect_staging = Vec::with_capacity(capacity_vertices);
        Ok(())
    }

    /// Rewrites the interactive-chrome region from this frame's presentation snapshot: button
    /// faces (pressed tint when armed), text-input fields (focus highlight) and the focused
    /// input's caret at its character position. Staging capacity is reserved once; no per-frame
    /// allocation.
    fn write_interactive_rects(
        &mut self,
        inputs: &FrameInputs,
        interaction: InteractionSnapshot,
    ) -> Result<(), BoxError> {
        let Some(pointer) = self.interactive_rect_ptr else {
            self.interactive_rect_written_vertices = 0;
            return Ok(());
        };
        let (surface_width, surface_height) = self.authored_surface_size();
        let mut staging = std::mem::take(&mut self.interactive_rect_staging);
        staging.clear();

        for (index, binding) in self.bindings.buttons.iter().enumerate() {
            let rgba = if interaction.pressed_button == Some(index) {
                binding.pressed_rgba
            } else {
                binding.rgba
            };
            push_flat_quad(&mut staging, binding.bounds, rgba, surface_width, surface_height);
        }

        // CriticalButtons have no rendering-visible pressed state here: their press is
        // dispatched through the adapter's separate framework-governed path (ADR-015 §4), which
        // does not feed this presentation snapshot. Only the static face renders.
        for binding in &self.bindings.critical_button_chrome {
            push_flat_quad(&mut staging, binding.bounds, binding.rgba, surface_width, surface_height);
        }

        for (index, binding) in self.bindings.text_inputs.iter().enumerate() {
            let chrome = &self.text_input_chrome[index];
            let rgba = if interaction.focused_input == Some(index) {
                chrome.focused_field_rgba
            } else {
                chrome.field_rgba
            };
            push_flat_quad(&mut staging, binding.bounds, rgba, surface_width, surface_height);
        }

        if let Some(index) = interaction.focused_input {
            if let (Some(binding), Some(chrome)) = (
                self.bindings.text_inputs.get(index),
                self.text_input_chrome.get(index),
            ) {
                let text = inputs.text(binding.source).unwrap_or("");
                let caret_x = binding.origin_x
                    + glyph_set_text_advance(
                        &self.bindings.standard,
                        &binding.glyph_set_id,
                        text,
                        interaction.caret,
                    );
                let caret_bounds = trustsc::Rect {
                    x: caret_x,
                    y: binding.origin_y,
                    width: 2,
                    height: chrome.caret_height,
                };
                push_flat_quad(
                    &mut staging,
                    caret_bounds,
                    chrome.caret_rgba,
                    surface_width,
                    surface_height,
                );
            }
        }

        if staging.len() > self.interactive_rect_capacity_vertices {
            let written = staging.len();
            self.interactive_rect_staging = staging;
            return Err(box_error(format!(
                "interactive chrome wrote {written} vertices, above the fixed capacity of {}",
                self.interactive_rect_capacity_vertices
            )));
        }

        // Safety: mapped once at creation with exactly this capacity; the in-flight fence waited
        // in draw_frame guarantees the GPU is done reading the previous frame's contents.
        unsafe {
            std::ptr::copy_nonoverlapping(staging.as_ptr(), pointer.as_ptr(), staging.len());
        }
        self.interactive_rect_written_vertices = staging.len() as u32;
        self.interactive_rect_staging = staging;
        Ok(())
    }

    /// Allocates the per-stream waterfall machinery once: the persistently mapped height array
    /// (rows × bins floats, the ring buffer's GPU mirror), the static triangle-grid index
    /// buffer, the fixed perspective camera, and the shared pipeline layout.
    fn create_waterfall_resources(&mut self) -> Result<(), BoxError> {
        if self.bindings.streams.is_empty() {
            return Ok(());
        }

        let push_constant_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<HeightfieldPushConstants>() as u32);
        let push_constant_ranges = [push_constant_range];
        let layout_info =
            vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&push_constant_ranges);
        self.waterfall_pipeline_layout =
            unsafe { self.device.create_pipeline_layout(&layout_info, None)? };

        let streams = self.bindings.streams.clone();
        for stream in &streams {
            let rows = stream.rows as u32;
            let bins = stream.bins as u32;
            let height_count = (rows * bins) as usize;
            let height_size = vk::DeviceSize::try_from(height_count * size_of::<f32>())?;
            let (height_buffer, height_buffer_memory) = create_buffer(
                &self.instance,
                &self.device,
                self.physical_device,
                height_size,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = unsafe {
                self.device.map_memory(
                    height_buffer_memory,
                    0,
                    height_size,
                    vk::MemoryMapFlags::empty(),
                )?
            };
            // Start flat: all heights zero.
            unsafe {
                std::ptr::write_bytes(mapped.cast::<u8>(), 0, height_count * size_of::<f32>());
            }

            let indices = heightfield_grid_indices(rows, bins);
            let index_size = vk::DeviceSize::try_from(indices.len() * size_of::<u32>())?;
            let (index_buffer, index_buffer_memory) = create_buffer(
                &self.instance,
                &self.device,
                self.physical_device,
                index_size,
                vk::BufferUsageFlags::INDEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            write_buffer(&self.device, index_buffer_memory, bytes_of_slice(&indices))?;

            let aspect = stream.bounds.width.max(1) as f32 / stream.bounds.height.max(1) as f32;
            let mvp = waterfall_camera_mvp(aspect);

            self.waterfalls.push(WaterfallResources {
                source: stream.source,
                bounds: stream.bounds,
                rows,
                bins,
                height_buffer,
                height_buffer_memory,
                height_ptr: std::ptr::NonNull::new(mapped.cast::<f32>()),
                index_buffer,
                index_buffer_memory,
                index_count: indices.len() as u32,
                mvp,
                row_offset: 0,
            });
        }

        Ok(())
    }

    /// Mirrors each stream's ring buffer into its mapped height array and records the ring
    /// cursor for the shader's scroll remap. Plain memcpy — no allocation, no object creation.
    fn write_waterfall_heights(&mut self, inputs: &FrameInputs) -> Result<(), BoxError> {
        for waterfall in &mut self.waterfalls {
            let (data, cursor) = inputs.stream(waterfall.source).ok_or_else(|| {
                box_error(format!(
                    "screen declares stream {} but FrameInputs does not know it",
                    waterfall.source
                ))
            })?;
            let Some(pointer) = waterfall.height_ptr else {
                continue;
            };
            let expected = (waterfall.rows * waterfall.bins) as usize;
            if data.len() != expected {
                return Err(box_error(format!(
                    "stream {} ring size {} does not match the waterfall grid {expected}",
                    waterfall.source,
                    data.len()
                )));
            }
            // Safety: mapped once at creation with exactly `expected` floats; the in-flight
            // fence waited in draw_frame guarantees the GPU finished reading the buffer.
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.as_ptr(), expected);
            }
            waterfall.row_offset = cursor as u32;
        }
        Ok(())
    }

    /// Allocates each `SignalTrace` node's persistently mapped `LINE_STRIP` vertex buffer
    /// (`capacity` `FlatVertex` points, ADR-018) and the shared empty pipeline layout. Nothing
    /// here runs per frame; [`write_traces`](Self::write_traces) rewrites the mapped buffer from
    /// the realtime ring each frame.
    fn create_trace_resources(&mut self) -> Result<(), BoxError> {
        if self.bindings.traces.is_empty() {
            return Ok(());
        }

        let layout_info = vk::PipelineLayoutCreateInfo::default();
        self.trace_pipeline_layout =
            unsafe { self.device.create_pipeline_layout(&layout_info, None)? };

        let traces = self.bindings.traces.clone();
        for trace in &traces {
            let buffer_size = vk::DeviceSize::try_from(trace.capacity * size_of::<FlatVertex>())?;
            let (buffer, memory) = create_buffer(
                &self.instance,
                &self.device,
                self.physical_device,
                buffer_size,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            let mapped = unsafe {
                self.device
                    .map_memory(memory, 0, buffer_size, vk::MemoryMapFlags::empty())?
            };
            unsafe {
                std::ptr::write_bytes(
                    mapped.cast::<u8>(),
                    0,
                    trace.capacity * size_of::<FlatVertex>(),
                );
            }

            self.traces.push(TraceResources {
                source: trace.source,
                bounds: trace.bounds,
                rgba: trace.rgba,
                capacity: trace.capacity,
                vertex_buffer: buffer,
                vertex_buffer_memory: memory,
                vertex_ptr: std::ptr::NonNull::new(mapped.cast::<FlatVertex>()),
            });
        }

        Ok(())
    }

    /// Rewrites each trace's mapped vertex buffer from its ring buffer's current samples
    /// (ADR-018): one `FlatVertex` per ring slot, oldest first (leftmost) so the line always
    /// scrolls left-to-right, `y` derived from the sample clamped to `[-1, 1]` so the polyline
    /// geometry can never leave the node's bounds — no per-node scissor is needed, unlike the
    /// waterfall's separate local clip space.
    fn write_traces(&mut self, inputs: &FrameInputs) -> Result<(), BoxError> {
        let (surface_width, surface_height) = self.authored_surface_size();
        for trace in &mut self.traces {
            let (data, cursor) = inputs.trace(trace.source).ok_or_else(|| {
                box_error(format!(
                    "screen declares signal trace {} but FrameInputs does not know it",
                    trace.source
                ))
            })?;
            if data.len() != trace.capacity {
                return Err(box_error(format!(
                    "signal trace {} ring size {} does not match the declared capacity {}",
                    trace.source,
                    data.len(),
                    trace.capacity
                )));
            }
            let Some(pointer) = trace.vertex_ptr else {
                continue;
            };

            let left = (2.0 * trace.bounds.x as f32 / surface_width) - 1.0;
            let right =
                (2.0 * (trace.bounds.x + trace.bounds.width as i32) as f32 / surface_width) - 1.0;
            let top = -1.0 + (2.0 * trace.bounds.y as f32 / surface_height);
            let bottom = -1.0
                + (2.0 * (trace.bounds.y + trace.bounds.height as i32) as f32 / surface_height);
            let mid_y = (top + bottom) / 2.0;
            let half_height = (bottom - top) / 2.0;
            let last_slot = (trace.capacity - 1).max(1) as f32;

            // Safety: mapped once at creation with exactly `capacity` vertices; the in-flight
            // fence waited in draw_frame guarantees the GPU finished reading the previous
            // frame's contents.
            unsafe {
                for slot in 0..trace.capacity {
                    let ring_index = (cursor + slot) % trace.capacity;
                    let sample = data[ring_index].clamp(-1.0, 1.0);
                    let x = left + (right - left) * (slot as f32 / last_slot);
                    let y = mid_y - sample * half_height;
                    pointer.as_ptr().add(slot).write(FlatVertex {
                        position: [x, y],
                        color: trace.rgba,
                    });
                }
            }
        }
        Ok(())
    }

    fn create_text_swapchain_resources(&mut self, extent: vk::Extent2D) -> Result<(), BoxError> {
        let (vertex_buffer, vertex_buffer_memory, vertex_count) = create_text_vertex_buffer(
            &self.instance,
            &self.device,
            self.physical_device,
            &self.text_layout,
            self.authored_surface_extent,
        )?;
        self.text_vertex_buffer = vertex_buffer;
        self.text_vertex_buffer_memory = vertex_buffer_memory;
        self.text_vertex_count = vertex_count;
        self.text_pipeline = create_text_pipeline(
            &self.device,
            self.render_pass,
            self.text_pipeline_layout,
            extent,
        )?;
        if !self.waterfalls.is_empty() {
            self.waterfall_pipeline = create_heightfield_pipeline(
                &self.device,
                self.render_pass,
                self.waterfall_pipeline_layout,
            )?;
        }
        Ok(())
    }

    /// Records one frame's command buffer: static text overlay, then the two dynamic text
    /// ranges (standard-package quads: clock/status; display-package quads: numeric displays).
    /// Re-recorded every frame from a pre-allocated buffer (RESET_COMMAND_BUFFER pool) — no
    /// Vulkan object is created here (ADR-013).
    fn record_command_buffer(
        &self,
        image_index: usize,
        dynamic_standard_vertices: u32,
    ) -> Result<(), BoxError> {
        let command_buffer = self.command_buffers[image_index];
        let extent = self.current_extent;
        let begin_info = vk::CommandBufferBeginInfo::default();
        unsafe {
            self.device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.device
                .begin_command_buffer(command_buffer, &begin_info)?;
        }

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: CLEAR_COLOR_RGBA_F32,
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        };
        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index])
            .render_area(render_area)
            .clear_values(&clear_values);

        unsafe {
            self.device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_info,
                vk::SubpassContents::INLINE,
            );

            // Draw order (ADR-014, extended by ADR-018): panel underlays -> 3D waterfalls
            // (depth-tested) -> governed images -> signal traces -> interactive chrome -> text
            // overlay.
            if self.flat_pipeline != vk::Pipeline::null() && self.flat_vertex_count > 0 {
                let viewport = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                let scissor = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                };
                let vertex_buffers = [self.flat_vertex_buffer];
                let offsets = [0];
                self.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.flat_pipeline,
                );
                self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
                self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                self.device
                    .cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &offsets);
                self.device
                    .cmd_draw(command_buffer, self.flat_vertex_count, 1, 0, 0);
            }

            if self.waterfall_pipeline != vk::Pipeline::null() {
                self.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.waterfall_pipeline,
                );
                for waterfall in &self.waterfalls {
                    let scaled_bounds = self.scale_bounds_to_current_extent(waterfall.bounds);
                    // Render strictly inside the node's reserved bounds (ADR-011). The bounds
                    // come from the fixed authoring surface, but the window is resizable, so the
                    // current swapchain extent can be smaller — the scissor rect must stay
                    // within the framebuffer or it's an invalid Vulkan command.
                    let viewport = vk::Viewport {
                        x: scaled_bounds.x as f32,
                        y: scaled_bounds.y as f32,
                        width: scaled_bounds.width as f32,
                        height: scaled_bounds.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    };
                    let scissor = clamp_scissor_to_extent(scaled_bounds, extent);
                    let push_constants = HeightfieldPushConstants {
                        mvp: waterfall.mvp,
                        rows: waterfall.rows as f32,
                        cols: waterfall.bins as f32,
                        row_offset: waterfall.row_offset as f32,
                        height_scale: WATERFALL_HEIGHT_SCALE,
                    };
                    let vertex_buffers = [waterfall.height_buffer];
                    let offsets = [0];

                    self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
                    self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                    self.device.cmd_push_constants(
                        command_buffer,
                        self.waterfall_pipeline_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes_of(&push_constants),
                    );
                    self.device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &vertex_buffers,
                        &offsets,
                    );
                    self.device.cmd_bind_index_buffer(
                        command_buffer,
                        waterfall.index_buffer,
                        0,
                        vk::IndexType::UINT32,
                    );
                    self.device
                        .cmd_draw_indexed(command_buffer, waterfall.index_count, 1, 0, 0, 0);
                }
            }

            if self.image_pipeline != vk::Pipeline::null() && !self.images.is_empty() {
                let viewport = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                let scissor = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                };
                self.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.image_pipeline,
                );
                self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
                self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                for image in &self.images {
                    let descriptor_sets = [image.descriptor_set];
                    let vertex_buffers = [image.vertex_buffer];
                    let offsets = [0];
                    self.device.cmd_bind_descriptor_sets(
                        command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        0,
                        &descriptor_sets,
                        &[],
                    );
                    self.device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &vertex_buffers,
                        &offsets,
                    );
                    self.device.cmd_draw(command_buffer, 6, 1, 0, 0);
                }
            }

            // Signal traces (ADR-018): scrolling amplitude polylines, above waterfalls/images.
            // The line-strip geometry is clamped to the node's bounds at write time (no per-node
            // scissor needed), so the full-extent viewport/scissor here matches the flat pipeline
            // panels/interactive chrome use.
            if self.trace_pipeline != vk::Pipeline::null() && !self.traces.is_empty() {
                let viewport = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                let scissor = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                };
                self.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.trace_pipeline,
                );
                self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
                self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                for trace in &self.traces {
                    let vertex_buffers = [trace.vertex_buffer];
                    let offsets = [0];
                    self.device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &vertex_buffers,
                        &offsets,
                    );
                    self.device
                        .cmd_draw(command_buffer, trace.capacity as u32, 1, 0, 0);
                }
            }

            // Interactive chrome (ADR-015): button faces, text-input fields and the caret —
            // above panels/waterfalls/images, beneath the text overlay so labels and echoed
            // content render on top of their faces.
            if self.flat_pipeline != vk::Pipeline::null()
                && self.interactive_rect_written_vertices > 0
            {
                let viewport = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                let scissor = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                };
                let vertex_buffers = [self.interactive_rect_buffer];
                let offsets = [0];
                self.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.flat_pipeline,
                );
                self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
                self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                self.device
                    .cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &offsets);
                self.device
                    .cmd_draw(command_buffer, self.interactive_rect_written_vertices, 1, 0, 0);
            }

            let has_static =
                self.text_pipeline != vk::Pipeline::null() && self.text_vertex_count > 0;
            let has_dynamic = self.text_pipeline != vk::Pipeline::null()
                && (dynamic_standard_vertices > 0
                    || self
                        .status_resources
                        .iter()
                        .any(|resources| resources.written_vertices > 0)
                    || self
                        .display_resources
                        .iter()
                        .any(|resources| resources.written_vertices > 0));

            if has_static || has_dynamic {
                let viewport = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                let scissor = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                };
                let push_constants = TextPushConstants::overlay();

                self.device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.text_pipeline,
                );
                self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
                self.device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                self.device.cmd_push_constants(
                    command_buffer,
                    self.text_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    bytes_of(&push_constants),
                );

                if has_static {
                    let vertex_buffers = [self.text_vertex_buffer];
                    let offsets = [0];
                    let descriptor_sets = [self.text_descriptor_set];
                    self.device.cmd_bind_descriptor_sets(
                        command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.text_pipeline_layout,
                        0,
                        &descriptor_sets,
                        &[],
                    );
                    self.device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &vertex_buffers,
                        &offsets,
                    );
                    self.device
                        .cmd_draw(command_buffer, self.text_vertex_count, 1, 0, 0);
                }

                if has_dynamic {
                    let vertex_buffers = [self.dynamic_vertex_buffer];
                    let offsets = [0];
                    self.device.cmd_bind_vertex_buffers(
                        command_buffer,
                        0,
                        &vertex_buffers,
                        &offsets,
                    );

                    if dynamic_standard_vertices > 0 {
                        let descriptor_sets = [self.text_descriptor_set];
                        self.device.cmd_bind_descriptor_sets(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.text_pipeline_layout,
                            0,
                            &descriptor_sets,
                            &[],
                        );
                        self.device
                            .cmd_draw(command_buffer, dynamic_standard_vertices, 1, 0, 0);
                    }

                    for resources in &self.display_resources {
                        if resources.written_vertices == 0 {
                            continue;
                        }
                        let descriptor_sets = [resources.descriptor_set];
                        self.device.cmd_bind_descriptor_sets(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.text_pipeline_layout,
                            0,
                            &descriptor_sets,
                            &[],
                        );
                        self.device.cmd_draw(
                            command_buffer,
                            resources.written_vertices,
                            1,
                            resources.vertex_offset as u32,
                            0,
                        );
                    }

                    // Each status draws in its own dedicated range with its own push-constant
                    // color (ADR-018 per-state color fix) — recorded last in this block so no
                    // later draw call inherits its color instead of the default white overlay.
                    for resources in &self.status_resources {
                        if resources.written_vertices == 0 {
                            continue;
                        }
                        let descriptor_sets = [self.text_descriptor_set];
                        self.device.cmd_bind_descriptor_sets(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.text_pipeline_layout,
                            0,
                            &descriptor_sets,
                            &[],
                        );
                        let status_push_constants = TextPushConstants {
                            text_color: resources.active_rgba,
                            ..TextPushConstants::overlay()
                        };
                        self.device.cmd_push_constants(
                            command_buffer,
                            self.text_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                            0,
                            bytes_of(&status_push_constants),
                        );
                        self.device.cmd_draw(
                            command_buffer,
                            resources.written_vertices,
                            1,
                            resources.vertex_offset as u32,
                            0,
                        );
                    }
                }
            }

            self.device.cmd_end_render_pass(command_buffer);
            self.device.end_command_buffer(command_buffer)?;
        }

        Ok(())
    }

    fn destroy_swapchain_objects(&mut self) {
        unsafe {
            if !self.command_buffers.is_empty() {
                self.device
                    .free_command_buffers(self.command_pool, &self.command_buffers);
                self.command_buffers.clear();
            }

            if self.depth_image_view != vk::ImageView::null() {
                self.device.destroy_image_view(self.depth_image_view, None);
                self.depth_image_view = vk::ImageView::null();
            }
            if self.depth_image != vk::Image::null() {
                self.device.destroy_image(self.depth_image, None);
                self.depth_image = vk::Image::null();
            }
            if self.depth_image_memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.depth_image_memory, None);
                self.depth_image_memory = vk::DeviceMemory::null();
            }

            if self.text_vertex_buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.text_vertex_buffer, None);
                self.text_vertex_buffer = vk::Buffer::null();
            }
            if self.text_vertex_buffer_memory != vk::DeviceMemory::null() {
                self.device
                    .free_memory(self.text_vertex_buffer_memory, None);
                self.text_vertex_buffer_memory = vk::DeviceMemory::null();
            }
            self.text_vertex_count = 0;

            if self.text_pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(self.text_pipeline, None);
                self.text_pipeline = vk::Pipeline::null();
            }
            if self.waterfall_pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(self.waterfall_pipeline, None);
                self.waterfall_pipeline = vk::Pipeline::null();
            }
            if self.flat_pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(self.flat_pipeline, None);
                self.flat_pipeline = vk::Pipeline::null();
            }
            if self.flat_vertex_buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.flat_vertex_buffer, None);
                self.flat_vertex_buffer = vk::Buffer::null();
            }
            if self.flat_vertex_buffer_memory != vk::DeviceMemory::null() {
                self.device
                    .free_memory(self.flat_vertex_buffer_memory, None);
                self.flat_vertex_buffer_memory = vk::DeviceMemory::null();
            }
            self.flat_vertex_count = 0;
            if self.image_pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(self.image_pipeline, None);
                self.image_pipeline = vk::Pipeline::null();
            }
            if self.trace_pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(self.trace_pipeline, None);
                self.trace_pipeline = vk::Pipeline::null();
            }
            for image in &mut self.images {
                if image.vertex_buffer != vk::Buffer::null() {
                    self.device.destroy_buffer(image.vertex_buffer, None);
                    image.vertex_buffer = vk::Buffer::null();
                }
                if image.vertex_buffer_memory != vk::DeviceMemory::null() {
                    self.device.free_memory(image.vertex_buffer_memory, None);
                    image.vertex_buffer_memory = vk::DeviceMemory::null();
                }
            }

            for framebuffer in self.framebuffers.drain(..) {
                self.device.destroy_framebuffer(framebuffer, None);
            }

            if self.render_pass != vk::RenderPass::null() {
                self.device.destroy_render_pass(self.render_pass, None);
                self.render_pass = vk::RenderPass::null();
            }

            for image_view in self.swapchain_image_views.drain(..) {
                self.device.destroy_image_view(image_view, None);
            }

            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }

            if self.offscreen_image_view != vk::ImageView::null() {
                self.device.destroy_image_view(self.offscreen_image_view, None);
                self.offscreen_image_view = vk::ImageView::null();
            }
            if self.offscreen_image != vk::Image::null() {
                self.device.destroy_image(self.offscreen_image, None);
                self.offscreen_image = vk::Image::null();
            }
            if self.offscreen_image_memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.offscreen_image_memory, None);
                self.offscreen_image_memory = vk::DeviceMemory::null();
            }
        }
    }

    fn destroy_text_static_objects(&mut self) {
        unsafe {
            if self.interactive_rect_ptr.take().is_some() {
                self.device.unmap_memory(self.interactive_rect_memory);
            }
            if self.interactive_rect_buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.interactive_rect_buffer, None);
                self.interactive_rect_buffer = vk::Buffer::null();
            }
            if self.interactive_rect_memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.interactive_rect_memory, None);
                self.interactive_rect_memory = vk::DeviceMemory::null();
            }
            if self.dynamic_vertex_ptr.take().is_some() {
                self.device.unmap_memory(self.dynamic_vertex_buffer_memory);
            }
            if self.dynamic_vertex_buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.dynamic_vertex_buffer, None);
                self.dynamic_vertex_buffer = vk::Buffer::null();
            }
            if self.dynamic_vertex_buffer_memory != vk::DeviceMemory::null() {
                self.device
                    .free_memory(self.dynamic_vertex_buffer_memory, None);
                self.dynamic_vertex_buffer_memory = vk::DeviceMemory::null();
            }
            for resources in self.display_resources.drain(..) {
                if resources.descriptor_pool != vk::DescriptorPool::null() {
                    self.device
                        .destroy_descriptor_pool(resources.descriptor_pool, None);
                }
                if resources.descriptor_set_layout != vk::DescriptorSetLayout::null() {
                    self.device
                        .destroy_descriptor_set_layout(resources.descriptor_set_layout, None);
                }
                if resources.atlas.sampler != vk::Sampler::null() {
                    self.device.destroy_sampler(resources.atlas.sampler, None);
                }
                if resources.atlas.image_view != vk::ImageView::null() {
                    self.device
                        .destroy_image_view(resources.atlas.image_view, None);
                }
                if resources.atlas.image != vk::Image::null() {
                    self.device.destroy_image(resources.atlas.image, None);
                }
                if resources.atlas.memory != vk::DeviceMemory::null() {
                    self.device.free_memory(resources.atlas.memory, None);
                }
            }
            for waterfall in &mut self.waterfalls {
                if waterfall.height_ptr.take().is_some() {
                    self.device.unmap_memory(waterfall.height_buffer_memory);
                }
                if waterfall.height_buffer != vk::Buffer::null() {
                    self.device.destroy_buffer(waterfall.height_buffer, None);
                    waterfall.height_buffer = vk::Buffer::null();
                }
                if waterfall.height_buffer_memory != vk::DeviceMemory::null() {
                    self.device
                        .free_memory(waterfall.height_buffer_memory, None);
                    waterfall.height_buffer_memory = vk::DeviceMemory::null();
                }
                if waterfall.index_buffer != vk::Buffer::null() {
                    self.device.destroy_buffer(waterfall.index_buffer, None);
                    waterfall.index_buffer = vk::Buffer::null();
                }
                if waterfall.index_buffer_memory != vk::DeviceMemory::null() {
                    self.device.free_memory(waterfall.index_buffer_memory, None);
                    waterfall.index_buffer_memory = vk::DeviceMemory::null();
                }
            }
            if self.waterfall_pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.waterfall_pipeline_layout, None);
                self.waterfall_pipeline_layout = vk::PipelineLayout::null();
            }
            if self.flat_pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.flat_pipeline_layout, None);
                self.flat_pipeline_layout = vk::PipelineLayout::null();
            }
            if self.image_pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.image_pipeline_layout, None);
                self.image_pipeline_layout = vk::PipelineLayout::null();
            }
            for trace in &mut self.traces {
                if trace.vertex_ptr.take().is_some() {
                    self.device.unmap_memory(trace.vertex_buffer_memory);
                }
                if trace.vertex_buffer != vk::Buffer::null() {
                    self.device.destroy_buffer(trace.vertex_buffer, None);
                    trace.vertex_buffer = vk::Buffer::null();
                }
                if trace.vertex_buffer_memory != vk::DeviceMemory::null() {
                    self.device.free_memory(trace.vertex_buffer_memory, None);
                    trace.vertex_buffer_memory = vk::DeviceMemory::null();
                }
            }
            if self.trace_pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.trace_pipeline_layout, None);
                self.trace_pipeline_layout = vk::PipelineLayout::null();
            }
            for image in self.images.drain(..) {
                if image.descriptor_pool != vk::DescriptorPool::null() {
                    self.device
                        .destroy_descriptor_pool(image.descriptor_pool, None);
                }
                if image.descriptor_set_layout != vk::DescriptorSetLayout::null() {
                    self.device
                        .destroy_descriptor_set_layout(image.descriptor_set_layout, None);
                }
                if image.texture.sampler != vk::Sampler::null() {
                    self.device.destroy_sampler(image.texture.sampler, None);
                }
                if image.texture.image_view != vk::ImageView::null() {
                    self.device
                        .destroy_image_view(image.texture.image_view, None);
                }
                if image.texture.image != vk::Image::null() {
                    self.device.destroy_image(image.texture.image, None);
                }
                if image.texture.memory != vk::DeviceMemory::null() {
                    self.device.free_memory(image.texture.memory, None);
                }
            }
            if self.text_pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.text_pipeline_layout, None);
                self.text_pipeline_layout = vk::PipelineLayout::null();
            }
            if self.text_descriptor_pool != vk::DescriptorPool::null() {
                self.device
                    .destroy_descriptor_pool(self.text_descriptor_pool, None);
                self.text_descriptor_pool = vk::DescriptorPool::null();
            }
            self.text_descriptor_set = vk::DescriptorSet::null();
            if self.text_descriptor_set_layout != vk::DescriptorSetLayout::null() {
                self.device
                    .destroy_descriptor_set_layout(self.text_descriptor_set_layout, None);
                self.text_descriptor_set_layout = vk::DescriptorSetLayout::null();
            }
            if self.text_atlas.sampler != vk::Sampler::null() {
                self.device.destroy_sampler(self.text_atlas.sampler, None);
                self.text_atlas.sampler = vk::Sampler::null();
            }
            if self.text_atlas.image_view != vk::ImageView::null() {
                self.device
                    .destroy_image_view(self.text_atlas.image_view, None);
                self.text_atlas.image_view = vk::ImageView::null();
            }
            if self.text_atlas.image != vk::Image::null() {
                self.device.destroy_image(self.text_atlas.image, None);
                self.text_atlas.image = vk::Image::null();
            }
            if self.text_atlas.memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.text_atlas.memory, None);
                self.text_atlas.memory = vk::DeviceMemory::null();
            }
        }
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.destroy_swapchain_objects();
            self.destroy_text_static_objects();
            self.device.destroy_fence(self.in_flight_fence, None);
            self.device
                .destroy_semaphore(self.render_finished_semaphore, None);
            self.device
                .destroy_semaphore(self.image_available_semaphore, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            if let Some(surface) = self.surface {
                self.surface_loader.destroy_surface(surface, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// Builds the Vulkan instance. `window` is `Some` for a windowed renderer — its WSI extensions
/// (`ash_window::enumerate_required_extensions`) are added and, on macOS, portability
/// enumeration is enabled alongside them when the loader offers it — and `None` for a headless
/// offscreen renderer, which skips WSI extensions entirely but keeps the same macOS portability
/// handling (ADR-016 §1): MoltenVK still requires it regardless of whether a surface exists.
fn create_instance(
    entry: &Entry,
    window: Option<&Window>,
    app_name: &str,
) -> Result<Instance, BoxError> {
    let app_name = CString::new(app_name)?;
    let engine_name = CString::new("trustsc-vulkan-winit")?;
    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_0);

    // Headless (offscreen): no WSI extensions at all — there is no surface to present to.
    let platform_extensions: Vec<*const std::os::raw::c_char> = match window {
        Some(window) => {
            ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?.to_vec()
        }
        None => Vec::new(),
    };

    #[cfg(target_os = "macos")]
    let (required_extensions, instance_flags) = {
        let mut required_extensions = platform_extensions;
        let mut instance_flags = vk::InstanceCreateFlags::empty();
        let available_extensions = unsafe { entry.enumerate_instance_extension_properties(None)? };
        if extension_names_contain(&available_extensions, khr::portability_enumeration::NAME)
            && extension_names_contain(
                &available_extensions,
                khr::get_physical_device_properties2::NAME,
            )
        {
            required_extensions.push(khr::get_physical_device_properties2::NAME.as_ptr());
            required_extensions.push(khr::portability_enumeration::NAME.as_ptr());
            instance_flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        }
        (required_extensions, instance_flags)
    };

    #[cfg(not(target_os = "macos"))]
    let (required_extensions, instance_flags) =
        (platform_extensions, vk::InstanceCreateFlags::empty());

    let instance_info = vk::InstanceCreateInfo::default()
        .flags(instance_flags)
        .application_info(&app_info)
        .enabled_extension_names(&required_extensions);

    let instance = unsafe { entry.create_instance(&instance_info, None)? };
    Ok(instance)
}

/// Picks a physical device. `surface` is `Some` for a windowed renderer — a queue family must
/// support both graphics and presentation to that surface — and `None` for a headless offscreen
/// renderer (ADR-016 §1), which drops the present-support filter and requires only a graphics
/// queue.
fn pick_physical_device(
    instance: &Instance,
    surface_loader: &khr::surface::Instance,
    surface: Option<vk::SurfaceKHR>,
) -> Result<(vk::PhysicalDevice, QueueFamilies, String), BoxError> {
    let devices = unsafe { instance.enumerate_physical_devices()? };

    for device in devices {
        if let Some(queue_families) =
            find_queue_families(instance, device, surface_loader, surface)?
        {
            let properties = unsafe { instance.get_physical_device_properties(device) };
            let device_name = unsafe { std::ffi::CStr::from_ptr(properties.device_name.as_ptr()) }
                .to_string_lossy()
                .into_owned();

            return Ok((device, queue_families, device_name));
        }
    }

    Err(box_error(if surface.is_some() {
        "no Vulkan device with graphics and present support was found"
    } else {
        "no Vulkan device with graphics support was found"
    }))
}

fn find_queue_families(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface_loader: &khr::surface::Instance,
    surface: Option<vk::SurfaceKHR>,
) -> Result<Option<QueueFamilies>, BoxError> {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    let mut graphics = None;
    let mut present = None;

    for (index, queue_family) in queue_families.iter().enumerate() {
        if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics = Some(index as u32);
        }

        match surface {
            Some(surface) => {
                let present_support = unsafe {
                    surface_loader.get_physical_device_surface_support(
                        physical_device,
                        index as u32,
                        surface,
                    )?
                };
                if present_support {
                    present = Some(index as u32);
                }

                if let (Some(graphics), Some(present)) = (graphics, present) {
                    return Ok(Some(QueueFamilies { graphics, present }));
                }
            }
            // Headless: no presentation to filter on — the graphics family alone is enough,
            // and it also stands in for `present` so the rest of the renderer (which always
            // has both) needs no offscreen-specific branching.
            None => {
                if let Some(graphics) = graphics {
                    return Ok(Some(QueueFamilies {
                        graphics,
                        present: graphics,
                    }));
                }
            }
        }
    }

    Ok(None)
}

/// Creates the logical device. `enable_swapchain` adds the `VK_KHR_swapchain` device extension
/// (windowed only — the extension requires `VK_KHR_surface` at the instance level, which a
/// headless offscreen instance never enables). The macOS `VK_KHR_portability_subset` check is
/// unconditional: MoltenVK requires it whenever the physical device reports it, regardless of
/// whether a swapchain is in use.
fn create_logical_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_families: QueueFamilies,
    enable_swapchain: bool,
) -> Result<(ash::Device, vk::Queue, vk::Queue), BoxError> {
    let priorities = [1.0_f32];
    let mut queue_infos = vec![vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_families.graphics)
        .queue_priorities(&priorities)];

    if queue_families.graphics != queue_families.present {
        queue_infos.push(
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_families.present)
                .queue_priorities(&priorities),
        );
    }

    let mut extensions = Vec::new();
    if enable_swapchain {
        extensions.push(khr::swapchain::NAME.as_ptr());
    }

    #[cfg(target_os = "macos")]
    {
        let available_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device)? };
        if extension_names_contain(&available_extensions, khr::portability_subset::NAME) {
            extensions.push(khr::portability_subset::NAME.as_ptr());
        }
    }

    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extensions);

    let device = unsafe { instance.create_device(physical_device, &create_info, None)? };
    let graphics_queue = unsafe { device.get_device_queue(queue_families.graphics, 0) };
    let present_queue = unsafe { device.get_device_queue(queue_families.present, 0) };
    Ok((device, graphics_queue, present_queue))
}

fn create_command_pool(
    device: &ash::Device,
    queue_family: u32,
) -> Result<vk::CommandPool, BoxError> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    let command_pool = unsafe { device.create_command_pool(&create_info, None)? };
    Ok(command_pool)
}

fn create_sync_objects(
    device: &ash::Device,
) -> Result<(vk::Semaphore, vk::Semaphore, vk::Fence), BoxError> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let image_available = unsafe { device.create_semaphore(&semaphore_info, None)? };
    let render_finished = unsafe { device.create_semaphore(&semaphore_info, None)? };
    let in_flight_fence = unsafe { device.create_fence(&fence_info, None)? };
    Ok((image_available, render_finished, in_flight_fence))
}

fn query_swapchain_support(
    physical_device: vk::PhysicalDevice,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<SwapchainSupport, BoxError> {
    let capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)?
    };
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface)? };
    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)?
    };

    Ok(SwapchainSupport {
        capabilities,
        formats,
        present_modes,
    })
}

fn choose_surface_format(
    formats: &[vk::SurfaceFormatKHR],
) -> Result<vk::SurfaceFormatKHR, BoxError> {
    formats
        .iter()
        .copied()
        .find(|format| {
            format.format == vk::Format::B8G8R8A8_UNORM
                && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or_else(|| formats.first().copied())
        .ok_or_else(|| box_error("swapchain did not expose any surface format"))
}

fn choose_present_mode(modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    modes
        .iter()
        .copied()
        .find(|mode| *mode == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO)
}

fn choose_extent(capabilities: &vk::SurfaceCapabilitiesKHR, window: &Window) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        return capabilities.current_extent;
    }

    let size = window.inner_size();
    vk::Extent2D {
        width: size.width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: size.height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    }
}

fn create_image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect_mask: vk::ImageAspectFlags,
) -> Result<vk::ImageView, BoxError> {
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(aspect_mask)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(range);
    let image_view = unsafe { device.create_image_view(&create_info, None)? };
    Ok(image_view)
}

/// `final_layout` is the layout the render pass automatically transitions the color attachment
/// to when it ends: `PRESENT_SRC_KHR` for a windowed swapchain image, `TRANSFER_SRC_OPTIMAL` for
/// the offscreen target (ADR-016 §1). The trailing subpass dependency below makes that
/// transition's writes visible to a subsequent transfer read, so `copy_image_to_buffer` needs no
/// separate `vkCmdPipelineBarrier` of its own — the render pass itself is the barrier.
fn create_render_pass(
    device: &ash::Device,
    format: vk::Format,
    depth_format: vk::Format,
    final_layout: vk::ImageLayout,
) -> Result<vk::RenderPass, BoxError> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(final_layout);
    let depth_attachment = vk::AttachmentDescription::default()
        .format(depth_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
    let color_reference = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let depth_reference = vk::AttachmentReference::default()
        .attachment(1)
        .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
    let color_references = [color_reference];
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_references)
        .depth_stencil_attachment(&depth_reference);
    let entry_dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        )
        .dst_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        )
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );
    // Without this, the automatic UNDEFINED -> final_layout transition at render pass end is
    // only ordered by the implicit external dependency's default (no) access mask — sufficient
    // for a swapchain image the presentation engine synchronizes separately, but not for a
    // subsequent vkCmdCopyImageToBuffer transfer read of the offscreen target, which could
    // observe undefined data on a stricter driver. This makes the color attachment write visible
    // to that read explicitly; harmless (an unexercised wait) on the windowed present path.
    let exit_dependency = vk::SubpassDependency::default()
        .src_subpass(0)
        .dst_subpass(vk::SUBPASS_EXTERNAL)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
        .dst_access_mask(vk::AccessFlags::TRANSFER_READ);

    let attachments = [color_attachment, depth_attachment];
    let subpasses = [subpass];
    let dependencies = [entry_dependency, exit_dependency];
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    let render_pass = unsafe { device.create_render_pass(&create_info, None)? };
    Ok(render_pass)
}

/// Picks the first supported depth format, preferring plain 32-bit depth.
fn find_depth_format(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<vk::Format, BoxError> {
    for format in [
        vk::Format::D32_SFLOAT,
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
    ] {
        let properties =
            unsafe { instance.get_physical_device_format_properties(physical_device, format) };
        if properties
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            return Ok(format);
        }
    }
    Err(box_error("no supported depth attachment format found"))
}

/// The image-view aspect mask matching `format`: depth-only formats need `DEPTH`, but
/// depth-stencil formats (the fallback tier of `find_depth_format`) require `DEPTH | STENCIL` —
/// a view that omits the stencil aspect of a depth-stencil image is invalid per the Vulkan spec.
fn depth_aspect_mask(format: vk::Format) -> vk::ImageAspectFlags {
    match format {
        vk::Format::D32_SFLOAT_S8_UINT | vk::Format::D24_UNORM_S8_UINT => {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        }
        _ => vk::ImageAspectFlags::DEPTH,
    }
}

fn create_framebuffer(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    image_view: vk::ImageView,
    depth_image_view: vk::ImageView,
    extent: vk::Extent2D,
) -> Result<vk::Framebuffer, BoxError> {
    let attachments = [image_view, depth_image_view];
    let create_info = vk::FramebufferCreateInfo::default()
        .render_pass(render_pass)
        .attachments(&attachments)
        .width(extent.width)
        .height(extent.height)
        .layers(1);
    let framebuffer = unsafe { device.create_framebuffer(&create_info, None)? };
    Ok(framebuffer)
}

fn allocate_command_buffers(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    count: u32,
) -> Result<Vec<vk::CommandBuffer>, BoxError> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count);
    let command_buffers = unsafe { device.allocate_command_buffers(&allocate_info)? };
    Ok(command_buffers)
}

fn create_text_atlas_resources(
    instance: &Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    command_pool: vk::CommandPool,
    graphics_queue: vk::Queue,
    package: &trustsc::TextPackage,
) -> Result<TextAtlasResources, BoxError> {
    let atlas = package
        .atlases
        .first()
        .ok_or_else(|| box_error("screen text package does not contain an atlas"))?;
    create_sampled_texture(
        instance,
        device,
        physical_device,
        command_pool,
        graphics_queue,
        atlas.width.into(),
        atlas.height.into(),
        vk::Format::R8_UNORM,
        &atlas.pixels,
    )
}

/// Bytes per texel for the sampled-texture formats this adapter uploads. A mismatched pixel
/// buffer would otherwise be silently accepted and produce an invalid `vkCmdCopyBufferToImage`
/// (garbage GPU reads or a validation error far from the actual bug).
fn format_bytes_per_pixel(format: vk::Format) -> Result<usize, BoxError> {
    match format {
        vk::Format::R8_UNORM => Ok(1),
        vk::Format::R8G8B8A8_UNORM => Ok(4),
        other => Err(box_error(format!(
            "no known bytes-per-pixel for texture format {}",
            other.as_raw()
        ))),
    }
}

/// Uploads a CPU pixel buffer as an immutable sampled texture (staging copy + layout
/// transitions, nearest sampling, clamp-to-edge). Shared by the R8 glyph atlases and the RGBA
/// governed images.
#[allow(clippy::too_many_arguments)]
fn create_sampled_texture(
    instance: &Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    command_pool: vk::CommandPool,
    graphics_queue: vk::Queue,
    width: u32,
    height: u32,
    format: vk::Format,
    pixels: &[u8],
) -> Result<TextAtlasResources, BoxError> {
    let bytes_per_pixel = format_bytes_per_pixel(format)?;
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|texels| texels.checked_mul(bytes_per_pixel))
        .ok_or_else(|| box_error("texture dimensions overflow usize"))?;
    if pixels.len() != expected_len {
        return Err(box_error(format!(
            "texture upload buffer is {} bytes but {width}x{height} at {bytes_per_pixel} bytes/pixel needs {expected_len}",
            pixels.len()
        )));
    }
    let image_size = vk::DeviceSize::try_from(pixels.len())?;
    let (staging_buffer, staging_memory) = create_buffer(
        instance,
        device,
        physical_device,
        image_size,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    // From here on, every fallible step must destroy `staging_buffer`/`staging_memory` (and,
    // once created, `image`/`memory`/`image_view`) before propagating an error: nothing owns
    // these handles until this function returns `Ok`, so an early `?` would otherwise leak them.
    let destroy_staging = |device: &ash::Device| unsafe {
        device.destroy_buffer(staging_buffer, None);
        device.free_memory(staging_memory, None);
    };

    if let Err(error) = write_buffer(device, staging_memory, pixels) {
        destroy_staging(device);
        return Err(error);
    }

    let (image, memory) = match create_image(
        instance,
        device,
        physical_device,
        width,
        height,
        format,
        vk::ImageTiling::OPTIMAL,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    ) {
        Ok(created) => created,
        Err(error) => {
            destroy_staging(device);
            return Err(error);
        }
    };
    let destroy_image = |device: &ash::Device| unsafe {
        device.destroy_image(image, None);
        device.free_memory(memory, None);
    };

    let upload_result = transition_image_layout(
        device,
        command_pool,
        graphics_queue,
        image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    )
    .and_then(|()| {
        copy_buffer_to_image(
            device,
            command_pool,
            graphics_queue,
            staging_buffer,
            image,
            width,
            height,
        )
    })
    .and_then(|()| {
        transition_image_layout(
            device,
            command_pool,
            graphics_queue,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )
    });
    if let Err(error) = upload_result {
        destroy_image(device);
        destroy_staging(device);
        return Err(error);
    }

    destroy_staging(device);

    let image_view = match create_image_view(device, image, format, vk::ImageAspectFlags::COLOR) {
        Ok(view) => view,
        Err(error) => {
            destroy_image(device);
            return Err(error);
        }
    };

    let sampler_info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .max_lod(1.0);
    let sampler = match unsafe { device.create_sampler(&sampler_info, None) } {
        Ok(sampler) => sampler,
        Err(error) => {
            unsafe {
                device.destroy_image_view(image_view, None);
            }
            destroy_image(device);
            return Err(error.into());
        }
    };

    Ok(TextAtlasResources {
        image,
        memory,
        image_view,
        sampler,
    })
}

fn create_text_descriptor_resources(
    device: &ash::Device,
    text_atlas: &TextAtlasResources,
) -> Result<
    (
        vk::DescriptorSetLayout,
        vk::DescriptorPool,
        vk::DescriptorSet,
    ),
    BoxError,
> {
    let layout_binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT);
    let bindings = [layout_binding];
    let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    let descriptor_set_layout = unsafe { device.create_descriptor_set_layout(&layout_info, None)? };

    let pool_size = vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1);
    let pool_sizes = [pool_size];
    let pool_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(&pool_sizes)
        .max_sets(1);
    let descriptor_pool = unsafe { device.create_descriptor_pool(&pool_info, None)? };

    let set_layouts = [descriptor_set_layout];
    let allocate_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool)
        .set_layouts(&set_layouts);
    let descriptor_set = unsafe { device.allocate_descriptor_sets(&allocate_info)? }[0];

    let image_info = vk::DescriptorImageInfo::default()
        .sampler(text_atlas.sampler)
        .image_view(text_atlas.image_view)
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
    let image_infos = [image_info];
    let write = vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&image_infos);
    unsafe {
        device.update_descriptor_sets(&[write], &[]);
    }

    Ok((descriptor_set_layout, descriptor_pool, descriptor_set))
}

fn create_text_pipeline_layout(
    device: &ash::Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> Result<vk::PipelineLayout, BoxError> {
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        .offset(0)
        .size(size_of::<TextPushConstants>() as u32);
    let set_layouts = [descriptor_set_layout];
    let push_constant_ranges = [push_constant_range];
    let create_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&create_info, None)? };
    Ok(pipeline_layout)
}

fn create_text_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    extent: vk::Extent2D,
) -> Result<vk::Pipeline, BoxError> {
    let vertex_shader_module = create_shader_module(device, TEXT_VERT_SPV)?;
    let fragment_shader_module = create_shader_module(device, TEXT_FRAG_SPV)?;
    let entry_point = CString::new("main")?;

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .module(vertex_shader_module)
            .name(&entry_point)
            .stage(vk::ShaderStageFlags::VERTEX),
        vk::PipelineShaderStageCreateInfo::default()
            .module(fragment_shader_module)
            .name(&entry_point)
            .stage(vk::ShaderStageFlags::FRAGMENT),
    ];

    let binding_descriptions = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(size_of::<TextVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attribute_descriptions = [
        vk::VertexInputAttributeDescription::default()
            .location(0)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        vk::VertexInputAttributeDescription::default()
            .location(1)
            .binding(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(size_of::<[f32; 2]>() as u32),
    ];
    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);
    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewport = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent,
    };
    let viewports = [viewport];
    let scissors = [scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1)
        .viewports(&viewports)
        .scissors(&scissors);
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&color_blend_attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    // Text is an overlay drawn last: depth test/write disabled, but the state must be supplied
    // explicitly now that the subpass carries a depth attachment.
    let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .depth_stencil_state(&depth_stencil_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline_result = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    };

    unsafe {
        device.destroy_shader_module(vertex_shader_module, None);
        device.destroy_shader_module(fragment_shader_module, None);
    }

    let pipeline = pipeline_result
        .map_err(|(_, error)| box_error(format!("failed to create text pipeline: {error}")))?[0];

    Ok(pipeline)
}

fn create_text_vertex_buffer(
    instance: &Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    text_layout: &ScreenTextLayout,
    extent: vk::Extent2D,
) -> Result<(vk::Buffer, vk::DeviceMemory, u32), BoxError> {
    let vertices = build_text_vertices(text_layout, extent)?;
    if vertices.is_empty() {
        // A screen with no glyph commands (e.g. no text-bearing nodes) has nothing to upload.
        // Vulkan buffer sizes must be > 0, so skip creation entirely rather than requesting a
        // zero-sized VERTEX_BUFFER; draw_frame already gates drawing on text_vertex_count > 0.
        return Ok((vk::Buffer::null(), vk::DeviceMemory::null(), 0));
    }

    let buffer_size = vk::DeviceSize::try_from(size_of_val(vertices.as_slice()))?;
    let (buffer, memory) = create_buffer(
        instance,
        device,
        physical_device,
        buffer_size,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    write_buffer(device, memory, bytes_of_slice(vertices.as_slice()))?;
    Ok((buffer, memory, vertices.len() as u32))
}

fn build_text_vertices(
    text_layout: &ScreenTextLayout,
    authored_surface_extent: vk::Extent2D,
) -> Result<Vec<TextVertex>, BoxError> {
    let atlas = text_layout
        .package
        .atlases
        .first()
        .ok_or_else(|| box_error("screen text package does not contain an atlas"))?;
    let width = authored_surface_extent.width.max(1) as f32;
    let height = authored_surface_extent.height.max(1) as f32;
    let atlas_width = atlas.width as f32;
    let atlas_height = atlas.height as f32;
    let commands = text_layout.runs.iter().flat_map(|run| run.commands.iter());
    let mut vertices = Vec::new();

    for command in commands {
        let glyph = text_layout
            .package
            .find_glyph(command.atlas_index, command.glyph_id)
            .ok_or_else(|| {
                box_error(format!(
                    "missing atlas glyph {} for screen text overlay",
                    command.glyph_id
                ))
            })?;

        vertices.extend_from_slice(&glyph_quad_vertices(
            command,
            glyph,
            atlas_width,
            atlas_height,
            width,
            height,
        ));
    }

    Ok(vertices)
}

/// Appends one panel's 6 solid-color vertices (two CCW triangles) in NDC.
fn push_flat_quad(
    vertices: &mut Vec<FlatVertex>,
    bounds: trustsc::Rect,
    color: [f32; 4],
    surface_width: f32,
    surface_height: f32,
) {
    let left = (2.0 * bounds.x as f32 / surface_width) - 1.0;
    let right = (2.0 * (bounds.x + bounds.width as i32) as f32 / surface_width) - 1.0;
    let top = -1.0 + (2.0 * bounds.y as f32 / surface_height);
    let bottom = -1.0 + (2.0 * (bounds.y + bounds.height as i32) as f32 / surface_height);
    let vertex = |x: f32, y: f32| FlatVertex {
        position: [x, y],
        color,
    };
    vertices.extend_from_slice(&[
        vertex(left, top),
        vertex(right, top),
        vertex(right, bottom),
        vertex(left, top),
        vertex(right, bottom),
        vertex(left, bottom),
    ]);
}

/// RGB scaled once at construction (alpha untouched) — pressed/field tint derivation.
fn scale_rgb(rgba: [f32; 4], factor: f32) -> [f32; 4] {
    [rgba[0] * factor, rgba[1] * factor, rgba[2] * factor, rgba[3]]
}

/// Pixel advance of the first `characters` characters of `text` in a glyph set — the caret's
/// x offset inside a text input. Characters missing from the set advance zero (they cannot
/// occur in echoed content, which the `set_text` boundary already validated).
fn glyph_set_text_advance(
    package: &trustsc::TextPackage,
    glyph_set_id: &str,
    text: &str,
    characters: u16,
) -> i32 {
    let Some(glyph_set) = package.find_numeric_glyph_set(glyph_set_id) else {
        return 0;
    };
    text.chars()
        .take(usize::from(characters))
        .map(|character| {
            glyph_set
                .entries
                .iter()
                .find(|entry| entry.character == character)
                .map(|entry| entry.advance_x.max(0))
                .unwrap_or(0)
        })
        .sum()
}

/// One image quad's 6 textured vertices (full 0..1 UV range) in NDC.
fn image_quad_vertices(
    bounds: trustsc::Rect,
    surface_width: f32,
    surface_height: f32,
) -> [TextVertex; 6] {
    let left = (2.0 * bounds.x as f32 / surface_width) - 1.0;
    let right = (2.0 * (bounds.x + bounds.width as i32) as f32 / surface_width) - 1.0;
    let top = -1.0 + (2.0 * bounds.y as f32 / surface_height);
    let bottom = -1.0 + (2.0 * (bounds.y + bounds.height as i32) as f32 / surface_height);
    [
        TextVertex::new([left, top], [0.0, 0.0]),
        TextVertex::new([right, top], [1.0, 0.0]),
        TextVertex::new([right, bottom], [1.0, 1.0]),
        TextVertex::new([left, top], [0.0, 0.0]),
        TextVertex::new([right, bottom], [1.0, 1.0]),
        TextVertex::new([left, bottom], [0.0, 1.0]),
    ]
}

/// The Panel underlay pipeline: FlatVertex input, opaque, depth off, dynamic viewport/scissor.
fn create_flat_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
) -> Result<vk::Pipeline, BoxError> {
    let binding_description = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(size_of::<FlatVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let attribute_descriptions = [
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(8),
    ];
    create_overlay_pipeline(
        device,
        render_pass,
        pipeline_layout,
        FLAT_VERT_SPV,
        FLAT_FRAG_SPV,
        binding_description,
        &attribute_descriptions,
        false,
        vk::PrimitiveTopology::TRIANGLE_LIST,
    )
}

/// The `SignalTrace` pipeline (ADR-018): reuses the flat solid-color shaders and `FlatVertex`
/// layout verbatim — the only difference from [`create_flat_pipeline`] is `LINE_STRIP`
/// topology, since the shaders themselves are topology-agnostic (they just pass through
/// position and interpolate color).
fn create_trace_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
) -> Result<vk::Pipeline, BoxError> {
    let binding_description = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(size_of::<FlatVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let attribute_descriptions = [
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(8),
    ];
    create_overlay_pipeline(
        device,
        render_pass,
        pipeline_layout,
        FLAT_VERT_SPV,
        FLAT_FRAG_SPV,
        binding_description,
        &attribute_descriptions,
        false,
        vk::PrimitiveTopology::LINE_STRIP,
    )
}

/// The governed-image pipeline: TextVertex input (pos + uv), alpha-blended, depth off.
fn create_image_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
) -> Result<vk::Pipeline, BoxError> {
    let binding_description = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(size_of::<TextVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let attribute_descriptions = [
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(0),
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(8),
    ];
    create_overlay_pipeline(
        device,
        render_pass,
        pipeline_layout,
        IMAGE_VERT_SPV,
        IMAGE_FRAG_SPV,
        binding_description,
        &attribute_descriptions,
        true,
        vk::PrimitiveTopology::TRIANGLE_LIST,
    )
}

/// Shared 2D-overlay pipeline builder (panels, images, signal traces): dynamic viewport/scissor,
/// no depth test/write, optional alpha blending, caller-chosen primitive topology (ADR-018 adds
/// `LINE_STRIP` alongside the original `TRIANGLE_LIST`).
#[allow(clippy::too_many_arguments)]
fn create_overlay_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    vert_spv: &[u8],
    frag_spv: &[u8],
    binding_description: vk::VertexInputBindingDescription,
    attribute_descriptions: &[vk::VertexInputAttributeDescription],
    alpha_blend: bool,
    topology: vk::PrimitiveTopology,
) -> Result<vk::Pipeline, BoxError> {
    let vertex_shader_module = create_shader_module(device, vert_spv)?;
    let fragment_shader_module = create_shader_module(device, frag_spv)?;
    let entry_point = CString::new("main")?;
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader_module)
            .name(&entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader_module)
            .name(&entry_point),
    ];

    let binding_descriptions = [binding_description];
    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(attribute_descriptions);
    let input_assembly_state =
        vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachment = if alpha_blend {
        vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
    } else {
        vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(false)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
    };
    let color_blend_attachments = [color_blend_attachment];
    let color_blend_state =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
    let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .depth_stencil_state(&depth_stencil_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline_result = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    };

    unsafe {
        device.destroy_shader_module(vertex_shader_module, None);
        device.destroy_shader_module(fragment_shader_module, None);
    }

    let pipeline = pipeline_result
        .map_err(|(_, error)| box_error(format!("failed to create overlay pipeline: {error}")))?[0];

    Ok(pipeline)
}

/// Builds the heightfield pipeline: one float vertex attribute (the height sample), triangle
/// list over the static grid indices, depth test + write enabled, opaque, dynamic
/// viewport/scissor (set per waterfall to its node bounds).
fn create_heightfield_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
) -> Result<vk::Pipeline, BoxError> {
    let vertex_shader_module = create_shader_module(device, HEIGHTFIELD_VERT_SPV)?;
    let fragment_shader_module = create_shader_module(device, HEIGHTFIELD_FRAG_SPV)?;
    let entry_point = CString::new("main")?;
    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_shader_module)
            .name(&entry_point),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_shader_module)
            .name(&entry_point),
    ];

    let binding_description = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(size_of::<f32>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let attribute_description = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(0)
        .format(vk::Format::R32_SFLOAT)
        .offset(0);
    let binding_descriptions = [binding_description];
    let attribute_descriptions = [attribute_description];
    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);
    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(false)
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let color_blend_attachments = [color_blend_attachment];
    let color_blend_state =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
    let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .depth_stencil_state(&depth_stencil_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline_result = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    };

    unsafe {
        device.destroy_shader_module(vertex_shader_module, None);
        device.destroy_shader_module(fragment_shader_module, None);
    }

    let pipeline = pipeline_result.map_err(|(_, error)| {
        box_error(format!("failed to create heightfield pipeline: {error}"))
    })?[0];

    Ok(pipeline)
}

/// Triangle-list indices over a rows × bins vertex grid (vertex id = row * bins + col), two CCW
/// triangles per cell.
/// Intersects a node's fixed-authoring-surface bounds with the current swapchain extent. The
/// window is resizable, so `extent` can be smaller than the surface the screen was compiled
/// for; an out-of-framebuffer scissor rect (negative offset or offset+extent beyond the
/// framebuffer) is an invalid Vulkan command, so this must clamp rather than pass bounds through
/// unchecked. Clamping to an empty rect (zero width/height) is legal and simply draws nothing.
fn clamp_scissor_to_extent(bounds: trustsc::Rect, extent: vk::Extent2D) -> vk::Rect2D {
    let x = bounds.x.clamp(0, extent.width as i32);
    let y = bounds.y.clamp(0, extent.height as i32);
    let width = bounds.width.min(extent.width.saturating_sub(x as u32));
    let height = bounds.height.min(extent.height.saturating_sub(y as u32));
    vk::Rect2D {
        offset: vk::Offset2D { x, y },
        extent: vk::Extent2D { width, height },
    }
}

fn scale_rect_to_extent(
    bounds: trustsc::Rect,
    source_extent: vk::Extent2D,
    destination_extent: vk::Extent2D,
) -> trustsc::Rect {
    let source_width = source_extent.width.max(1) as f32;
    let source_height = source_extent.height.max(1) as f32;
    let x_scale = destination_extent.width.max(1) as f32 / source_width;
    let y_scale = destination_extent.height.max(1) as f32 / source_height;

    let left = (bounds.x as f32 * x_scale).floor() as i32;
    let top = (bounds.y as f32 * y_scale).floor() as i32;
    let right = ((bounds.x as f32 + bounds.width as f32) * x_scale).ceil() as i32;
    let bottom = ((bounds.y as f32 + bounds.height as f32) * y_scale).ceil() as i32;

    trustsc::Rect {
        x: left,
        y: top,
        width: right.saturating_sub(left) as u32,
        height: bottom.saturating_sub(top) as u32,
    }
}

fn heightfield_grid_indices(rows: u32, bins: u32) -> Vec<u32> {
    if rows < 2 || bins < 2 {
        // Fewer than 2 rows or columns means zero cells: `rows - 1`/`bins - 1` would otherwise
        // underflow (u32) and turn into a request for an enormous capacity.
        return Vec::new();
    }
    let mut indices = Vec::with_capacity(((rows - 1) * (bins - 1) * 6) as usize);
    for row in 0..rows - 1 {
        for col in 0..bins - 1 {
            let top_left = row * bins + col;
            let top_right = top_left + 1;
            let bottom_left = top_left + bins;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }
    indices
}

/// The waterfall's fixed camera: perspective(45°) × look-at from an elevated front position
/// toward the middle of the scrolling grid. Hand-rolled column-major math (no linear-algebra
/// dependency), Vulkan clip conventions (Y flipped, depth 0..1).
fn waterfall_camera_mvp(aspect: f32) -> [[f32; 4]; 4] {
    let projection = perspective_vk(45f32.to_radians(), aspect, 0.1, 10.0);
    let view = look_at(
        [0.0, 1.05, -1.35], // eye: above and in front of the newest row
        [0.0, 0.10, 0.55],  // target: middle of the receding history
        [0.0, 1.0, 0.0],
    );
    mat4_mul(projection, view)
}

/// Right-handed perspective projection for Vulkan clip space (depth 0..1, Y down).
fn perspective_vk(fov_y: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let focal = 1.0 / (fov_y / 2.0).tan();
    let mut matrix = [[0.0f32; 4]; 4];
    matrix[0][0] = focal / aspect;
    matrix[1][1] = -focal; // Vulkan Y points down in clip space
    matrix[2][2] = far / (near - far);
    matrix[2][3] = -1.0;
    matrix[3][2] = (near * far) / (near - far);
    matrix
}

/// Right-handed look-at view matrix (column-major).
fn look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let forward = normalize(sub(center, eye));
    let side = normalize(cross(forward, up));
    let true_up = cross(side, forward);

    [
        [side[0], true_up[0], -forward[0], 0.0],
        [side[1], true_up[1], -forward[1], 0.0],
        [side[2], true_up[2], -forward[2], 0.0],
        [-dot(side, eye), -dot(true_up, eye), dot(forward, eye), 1.0],
    ]
}

fn mat4_mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0f32; 4]; 4];
    for (column, result_column) in result.iter_mut().enumerate() {
        for (row, cell) in result_column.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][row] * b[column][k]).sum();
        }
    }
    result
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    [v[0] / length, v[1] / length, v[2] / length]
}

/// Per-render-call glyph budget of the dynamic text path; every realtime binding's capacity is
/// checked against it at construction, so the frame loop's ArrayVecs can never overflow.
const DYNAMIC_RUN_CAPACITY: usize = 64;

/// Writes one render call's glyph commands as quads into the mapped dynamic buffer, advancing
/// `cursor` and enforcing the range's fixed capacity.
fn write_glyph_quads(
    vertices: &mut [TextVertex],
    cursor: &mut usize,
    range_end: usize,
    commands: &[trustsc::GlyphDrawCommand],
    package: &trustsc::TextPackage,
    surface_width: f32,
    surface_height: f32,
) -> Result<(), BoxError> {
    let atlas = package
        .atlases
        .first()
        .ok_or_else(|| box_error("text package does not contain an atlas"))?;
    let atlas_width = atlas.width as f32;
    let atlas_height = atlas.height as f32;

    for command in commands {
        let glyph = package
            .find_glyph(command.atlas_index, command.glyph_id)
            .ok_or_else(|| {
                box_error(format!(
                    "missing atlas glyph {} for dynamic text",
                    command.glyph_id
                ))
            })?;
        if *cursor + 6 > range_end {
            return Err(box_error("dynamic text capacity exceeded"));
        }
        vertices[*cursor..*cursor + 6].copy_from_slice(&glyph_quad_vertices(
            command,
            glyph,
            atlas_width,
            atlas_height,
            surface_width,
            surface_height,
        ));
        *cursor += 6;
    }

    Ok(())
}

/// The pixel advance of the rendered `YYYY-MM-DD ` prefix (date digits, separators, and one
/// trailing space) for the given wall-clock date, computed from the glyph set's advances.
fn glyph_sequence_advance(
    package: &trustsc::TextPackage,
    glyph_set_id: &str,
    clock: WallClock,
) -> Result<i32, BoxError> {
    let glyph_set = package
        .find_numeric_glyph_set(glyph_set_id)
        .ok_or_else(|| box_error(format!("unknown numeric glyph set {glyph_set_id}")))?;

    let digit = |value: u16| char::from(b'0' + (value % 10) as u8);
    let characters = [
        digit(clock.year / 1000),
        digit(clock.year / 100),
        digit(clock.year / 10),
        digit(clock.year),
        '-',
        digit(u16::from(clock.month) / 10),
        digit(u16::from(clock.month)),
        '-',
        digit(u16::from(clock.day) / 10),
        digit(u16::from(clock.day)),
        ' ',
    ];

    let mut advance = 0i32;
    for character in characters {
        let entry = glyph_set
            .entries
            .iter()
            .find(|entry| entry.character == character)
            .ok_or_else(|| {
                box_error(format!(
                    "glyph set {glyph_set_id} is missing '{character}' for the datetime clock"
                ))
            })?;
        advance += entry.advance_x;
    }
    Ok(advance)
}

/// The six vertices (two CCW triangles) of one glyph quad in NDC, with atlas UVs. Shared by the
/// static overlay path (heap `Vec` at swapchain creation) and the dynamic realtime path (writes
/// into the persistently mapped buffer each frame).
fn glyph_quad_vertices(
    command: &trustsc::GlyphDrawCommand,
    glyph: &trustsc::AtlasGlyph,
    atlas_width: f32,
    atlas_height: f32,
    surface_width: f32,
    surface_height: f32,
) -> [TextVertex; 6] {
    let left = (2.0 * command.x as f32 / surface_width) - 1.0;
    let right = (2.0 * (command.x + i32::from(command.width)) as f32 / surface_width) - 1.0;
    let top = -1.0 + (2.0 * command.y as f32 / surface_height);
    let bottom = -1.0 + (2.0 * (command.y + i32::from(command.height)) as f32 / surface_height);

    let u0 = glyph.x as f32 / atlas_width;
    let v0 = glyph.y as f32 / atlas_height;
    let u1 = (glyph.x + glyph.width) as f32 / atlas_width;
    let v1 = (glyph.y + glyph.height) as f32 / atlas_height;

    [
        TextVertex::new([left, top], [u0, v0]),
        TextVertex::new([right, top], [u1, v0]),
        TextVertex::new([right, bottom], [u1, v1]),
        TextVertex::new([left, top], [u0, v0]),
        TextVertex::new([right, bottom], [u1, v1]),
        TextVertex::new([left, bottom], [u0, v1]),
    ]
}

/// Wall-clock timestamp handed to the renderer each frame (UTC civil time; a real device would
/// use its configured device clock).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WallClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
}

/// Converts seconds since the Unix epoch to UTC civil date/time (Howard Hinnant's
/// days-from-civil inverse, pure integer math — no chrono/time dependency).
pub fn civil_from_unix(unix_seconds: i64) -> WallClock {
    let days = unix_seconds.div_euclid(86_400);
    let seconds_of_day = unix_seconds.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = if month <= 2 { year + 1 } else { year };

    WallClock {
        year: year.clamp(0, 9999) as u16,
        month,
        day,
        hours: (seconds_of_day / 3600) as u8,
        minutes: (seconds_of_day / 60 % 60) as u8,
        seconds: (seconds_of_day % 60) as u8,
    }
}

fn create_shader_module(device: &ash::Device, bytes: &[u8]) -> Result<vk::ShaderModule, BoxError> {
    let code = read_spv(&mut Cursor::new(bytes))?;
    let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
    let shader_module = unsafe { device.create_shader_module(&create_info, None)? };
    Ok(shader_module)
}

fn create_buffer(
    instance: &Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory), BoxError> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_info, None)? };
    let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type_index = find_memory_type(
        instance,
        physical_device,
        memory_requirements.memory_type_bits,
        properties,
    )?;
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(memory_requirements.size)
        .memory_type_index(memory_type_index);
    let memory = unsafe { device.allocate_memory(&allocate_info, None)? };
    unsafe {
        device.bind_buffer_memory(buffer, memory, 0)?;
    }
    Ok((buffer, memory))
}

fn write_buffer(
    device: &ash::Device,
    memory: vk::DeviceMemory,
    bytes: &[u8],
) -> Result<(), BoxError> {
    let size = vk::DeviceSize::try_from(bytes.len())?;
    unsafe {
        let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
        ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.cast::<u8>(), bytes.len());
        device.unmap_memory(memory);
    }
    Ok(())
}

fn create_image(
    instance: &Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    width: u32,
    height: u32,
    format: vk::Format,
    tiling: vk::ImageTiling,
    usage: vk::ImageUsageFlags,
    properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Image, vk::DeviceMemory), BoxError> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .format(format)
        .tiling(tiling)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .samples(vk::SampleCountFlags::TYPE_1);
    let image = unsafe { device.create_image(&image_info, None)? };
    let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory_type_index = find_memory_type(
        instance,
        physical_device,
        memory_requirements.memory_type_bits,
        properties,
    )?;
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(memory_requirements.size)
        .memory_type_index(memory_type_index);
    let memory = unsafe { device.allocate_memory(&allocate_info, None)? };
    unsafe {
        device.bind_image_memory(image, memory, 0)?;
    }
    Ok((image, memory))
}

fn transition_image_layout(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> Result<(), BoxError> {
    let command_buffer = begin_single_time_commands(device, command_pool)?;

    let (src_access_mask, dst_access_mask, src_stage, dst_stage) = match (old_layout, new_layout) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
        ),
        (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
        ),
        _ => {
            return Err(box_error(format!(
                "unsupported atlas layout transition: {} -> {}",
                old_layout.as_raw(),
                new_layout.as_raw()
            )));
        }
    };

    let subresource_range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1);
    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(subresource_range)
        .src_access_mask(src_access_mask)
        .dst_access_mask(dst_access_mask);

    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }

    end_single_time_commands(device, command_pool, queue, command_buffer)
}

fn copy_buffer_to_image(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    buffer: vk::Buffer,
    image: vk::Image,
    width: u32,
    height: u32,
) -> Result<(), BoxError> {
    let command_buffer = begin_single_time_commands(device, command_pool)?;
    let subresource_layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1);
    let region = vk::BufferImageCopy::default()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(subresource_layers)
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    unsafe {
        device.cmd_copy_buffer_to_image(
            command_buffer,
            buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
    }
    end_single_time_commands(device, command_pool, queue, command_buffer)
}

/// The offscreen readback's one-shot copy (ADR-016 §1): `image` must already be in
/// `TRANSFER_SRC_OPTIMAL` — true by construction, since the offscreen render pass's color
/// attachment `final_layout` puts it there automatically at the end of every render pass, and
/// that render pass's trailing subpass dependency (`create_render_pass`) makes the write visible
/// to this transfer read, so no separate barrier is issued here.
fn copy_image_to_buffer(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    buffer: vk::Buffer,
    width: u32,
    height: u32,
) -> Result<(), BoxError> {
    let command_buffer = begin_single_time_commands(device, command_pool)?;
    let subresource_layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1);
    let region = vk::BufferImageCopy::default()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(subresource_layers)
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    unsafe {
        device.cmd_copy_image_to_buffer(
            command_buffer,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            buffer,
            &[region],
        );
    }
    end_single_time_commands(device, command_pool, queue, command_buffer)
}

fn begin_single_time_commands(
    device: &ash::Device,
    command_pool: vk::CommandPool,
) -> Result<vk::CommandBuffer, BoxError> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer = unsafe { device.allocate_command_buffers(&allocate_info)? }[0];
    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    unsafe {
        device.begin_command_buffer(command_buffer, &begin_info)?;
    }
    Ok(command_buffer)
}

fn end_single_time_commands(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
) -> Result<(), BoxError> {
    unsafe {
        device.end_command_buffer(command_buffer)?;
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        device.queue_submit(queue, &[submit_info], vk::Fence::null())?;
        device.queue_wait_idle(queue)?;
        device.free_command_buffers(command_pool, &command_buffers);
    }
    Ok(())
}

fn find_memory_type(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    type_filter: u32,
    properties: vk::MemoryPropertyFlags,
) -> Result<u32, BoxError> {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };

    for index in 0..memory_properties.memory_type_count {
        let supported = (type_filter & (1 << index)) != 0;
        let has_properties = memory_properties.memory_types[index as usize]
            .property_flags
            .contains(properties);
        if supported && has_properties {
            return Ok(index);
        }
    }

    Err(box_error(format!(
        "failed to find Vulkan memory type for properties 0x{:x}",
        properties.as_raw()
    )))
}

fn bytes_of<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}

fn bytes_of_slice<T>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr().cast::<u8>(), size_of_val(slice)) }
}

fn box_error(message: impl Into<String>) -> BoxError {
    std::io::Error::other(message.into()).into()
}

#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
fn extension_names_contain(
    extension_properties: &[vk::ExtensionProperties],
    extension_name: &std::ffi::CStr,
) -> bool {
    extension_properties
        .iter()
        .any(|property| extension_property_matches(property, extension_name))
}

#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
fn extension_property_matches(
    extension_property: &vk::ExtensionProperties,
    extension_name: &std::ffi::CStr,
) -> bool {
    extension_property
        .extension_name
        .iter()
        .copied()
        .take_while(|character| *character != 0)
        .map(|character| character as u8)
        .eq(extension_name.to_bytes().iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use trustsc::{
        default_standard_text_package, CompiledNode, CompiledNodeKind, CompiledScreenPackage,
        CriticalButtonSpec, LayoutKind, LayoutSpec, Rect, SystemEvent,
        DEFAULT_STANDARD_HELLO_WORLD_STRING_ID,
    };
    use crate::offscreen::OffscreenRenderer;

    const SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "RendererTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 8,
            padding: 16,
        },
        nodes: &[CompiledNode {
            id: "greeting-label",
            bounds: Rect {
                x: 10,
                y: 20,
                width: 300,
                height: 64,
            },
            kind: CompiledNodeKind::CriticalButton(CriticalButtonSpec {
                requirement_id: "REQ-TEST-001",
                text_key: DEFAULT_STANDARD_HELLO_WORLD_STRING_ID,
                color_token: "Theme.Colors.PrimaryAction",
                on_press: SystemEvent::NoOp,
            }),
        }],
        golden_references: &[],
    };

    fn test_layout() -> ScreenTextLayout {
        let package = default_standard_text_package().expect("standard package should load");
        ScreenTextLayout::from_screen(&SCREEN, package, "en-US").expect("layout should build")
    }

    #[test]
    fn builds_textured_quads_for_every_resolved_run() {
        let layout = test_layout();
        let extent = vk::Extent2D {
            width: 320,
            height: 128,
        };
        let vertices =
            build_text_vertices(&layout, extent).expect("vertex generation should succeed");
        let all_commands = layout
            .runs
            .iter()
            .flat_map(|run| run.commands.iter())
            .collect::<Vec<_>>();
        let atlas = layout.package.atlases.first().expect("atlas should exist");
        let first_command = all_commands[0];
        let first_glyph = layout
            .package
            .find_glyph(first_command.atlas_index, first_command.glyph_id)
            .expect("first glyph should exist");
        let last_command = *all_commands.last().expect("last command should exist");
        let last_glyph = layout
            .package
            .find_glyph(last_command.atlas_index, last_command.glyph_id)
            .expect("last glyph should exist");

        assert_eq!(vertices.len(), all_commands.len() * 6);
        assert!(vertices.iter().all(|vertex| {
            (-1.0..=1.0).contains(&vertex.position[0])
                && (-1.0..=1.0).contains(&vertex.position[1])
                && (0.0..=1.0).contains(&vertex.tex_coord[0])
                && (0.0..=1.0).contains(&vertex.tex_coord[1])
        }));

        assert_vertex(
            &vertices[0],
            expected_position(first_command.x, first_command.y, extent),
            [
                first_glyph.x as f32 / atlas.width as f32,
                first_glyph.y as f32 / atlas.height as f32,
            ],
        );

        let last_quad = &vertices[vertices.len() - 6..];
        assert_vertex(
            &last_quad[0],
            expected_position(last_command.x, last_command.y, extent),
            [
                last_glyph.x as f32 / atlas.width as f32,
                last_glyph.y as f32 / atlas.height as f32,
            ],
        );
        assert_vertex(
            &last_quad[5],
            expected_position(
                last_command.x,
                last_command.y + i32::from(last_command.height),
                extent,
            ),
            [
                last_glyph.x as f32 / atlas.width as f32,
                (last_glyph.y + last_glyph.height) as f32 / atlas.height as f32,
            ],
        );
    }

    #[test]
    fn builds_no_vertices_for_a_layout_with_no_text_runs() {
        let mut layout = test_layout();
        layout.runs.clear();

        let vertices = build_text_vertices(
            &layout,
            vk::Extent2D {
                width: 320,
                height: 128,
            },
        )
        .expect("empty layout should still produce a (empty) vertex list");

        // create_text_vertex_buffer must special-case this: Vulkan buffer sizes must be > 0, so
        // it skips create_buffer entirely rather than requesting a zero-sized VERTEX_BUFFER.
        assert!(vertices.is_empty());
    }

    #[test]
    fn rejects_text_vertex_generation_without_an_atlas() {
        let mut layout = test_layout();
        layout.package.atlases.clear();

        let error = build_text_vertices(
            &layout,
            vk::Extent2D {
                width: 128,
                height: 64,
            },
        )
        .expect_err("missing atlas should fail vertex generation");

        assert!(error.to_string().contains("does not contain an atlas"));
    }

    #[test]
    fn generates_two_ccw_triangles_per_grid_cell() {
        let indices = heightfield_grid_indices(3, 4);
        // (3-1) x (4-1) cells x 6 indices.
        assert_eq!(indices.len(), 36);
        // First cell: vertices 0,4,1 then 1,4,5 (row stride = bins = 4).
        assert_eq!(&indices[0..6], &[0, 4, 1, 1, 4, 5]);
        // Every index addresses a real vertex.
        assert!(indices.iter().all(|&index| index < 12));
    }

    #[test]
    fn grid_indices_never_underflow_for_degenerate_grids() {
        assert!(heightfield_grid_indices(0, 4).is_empty());
        assert!(heightfield_grid_indices(1, 4).is_empty());
        assert!(heightfield_grid_indices(4, 1).is_empty());
        assert!(heightfield_grid_indices(0, 0).is_empty());
    }

    #[test]
    fn scissor_clamps_to_a_smaller_swapchain_extent() {
        let bounds = trustsc::Rect {
            x: 16,
            y: 200,
            width: 1248,
            height: 504,
        };

        // Window shrunk below the authored 1280x720 surface: the scissor must not extend past
        // the framebuffer in either dimension.
        let scissor = clamp_scissor_to_extent(
            bounds,
            vk::Extent2D {
                width: 640,
                height: 300,
            },
        );
        assert_eq!(scissor.offset.x, 16);
        assert_eq!(scissor.offset.y, 200);
        assert_eq!(scissor.extent.width, 624);
        assert_eq!(scissor.extent.height, 100);

        // A node bound entirely outside a very small extent clamps to an empty (but valid) rect.
        let offscreen = clamp_scissor_to_extent(
            bounds,
            vk::Extent2D {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(offscreen.extent.width, 0);
        assert_eq!(offscreen.extent.height, 0);

        // Enough room: bounds pass through unchanged.
        let unclamped = clamp_scissor_to_extent(
            bounds,
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
        );
        assert_eq!(unclamped.offset.x, 16);
        assert_eq!(unclamped.offset.y, 200);
        assert_eq!(unclamped.extent.width, 1248);
        assert_eq!(unclamped.extent.height, 504);
    }

    #[test]
    fn scales_authored_bounds_to_hidpi_extent() {
        let authored = vk::Extent2D {
            width: 1280,
            height: 720,
        };
        let framebuffer = vk::Extent2D {
            width: 2560,
            height: 1440,
        };
        let bounds = trustsc::Rect {
            x: 16,
            y: 200,
            width: 1248,
            height: 504,
        };

        let scaled = scale_rect_to_extent(bounds, authored, framebuffer);
        assert_eq!(
            scaled,
            trustsc::Rect {
                x: 32,
                y: 400,
                width: 2496,
                height: 1008
            }
        );
    }

    #[test]
    fn camera_math_behaves_like_a_view_projection() {
        // look_at maps the eye to the origin.
        let view = look_at([1.0, 2.0, 3.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let eye_in_view = transform(view, [1.0, 2.0, 3.0, 1.0]);
        for component in &eye_in_view[0..3] {
            assert!(
                component.abs() < 1e-5,
                "eye should map to origin: {eye_in_view:?}"
            );
        }

        // A point straight ahead of the camera projects to clip center with positive w.
        let mvp = waterfall_camera_mvp(16.0 / 9.0);
        let center = transform(mvp, [0.0, 0.10, 0.55, 1.0]);
        assert!(center[3] > 0.0, "target should be in front of the camera");
        assert!(
            (center[0] / center[3]).abs() < 0.05,
            "target should be near clip x center"
        );
    }

    fn transform(matrix: [[f32; 4]; 4], point: [f32; 4]) -> [f32; 4] {
        let mut result = [0.0f32; 4];
        for (row, cell) in result.iter_mut().enumerate() {
            *cell = (0..4)
                .map(|column| matrix[column][row] * point[column])
                .sum();
        }
        result
    }

    #[test]
    fn dynamic_buffer_layout_packs_ranges_contiguously() {
        // [standard | s0 | s1 | d0 | d1 | d2] with quad counts 5 / (1, 2) / 2 / 0 / 3.
        let (standard, status_ranges, display_ranges, total) =
            dynamic_buffer_layout(5, &[1, 2], &[2, 0, 3]);
        assert_eq!(standard, 30);
        assert_eq!(status_ranges, vec![(30, 6), (36, 12)]);
        assert_eq!(display_ranges, vec![(48, 12), (60, 0), (60, 18)]);
        assert_eq!(total, 78);

        // No dynamic text at all.
        let (standard, status_ranges, display_ranges, total) = dynamic_buffer_layout(0, &[], &[]);
        assert_eq!((standard, total), (0, 0));
        assert!(status_ranges.is_empty());
        assert!(display_ranges.is_empty());

        // Standard-only screens (hello_world) get no status or display ranges.
        let (standard, status_ranges, display_ranges, total) =
            dynamic_buffer_layout(4, &[], &[0, 0]);
        assert_eq!(standard, 24);
        assert!(status_ranges.is_empty());
        assert_eq!(display_ranges, vec![(24, 0), (24, 0)]);
        assert_eq!(total, 24);
    }

    fn stub_number_binding(
        node_id: &'static str,
        display_index: usize,
    ) -> trustsc::realtime::NumberBinding {
        trustsc::realtime::NumberBinding {
            node_id,
            bounds: trustsc::Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            origin_x: 0,
            origin_y: 0,
            source: "SRC",
            template_id: "TPL".to_string(),
            display_index,
            color_token: "Theme.Colors.ScoreDigits",
            capacity: 3,
        }
    }

    #[test]
    fn accumulates_display_quads_by_index_and_rejects_out_of_range() {
        let numbers = [
            stub_number_binding("a", 0),
            stub_number_binding("b", 1),
            stub_number_binding("c", 0),
        ];
        let per_display = accumulate_display_quads(&numbers, 2).expect("in-range indices");
        assert_eq!(per_display, vec![6, 3]);

        // A hand-constructed ScreenBindings (bypassing from_screen's own resolution) could
        // carry a display_index beyond the actual number of bound display packages.
        let out_of_range = [stub_number_binding("d", 5)];
        let error = accumulate_display_quads(&out_of_range, 2)
            .expect_err("out-of-range display_index should be rejected, not panic");
        assert!(
            error
                .to_string()
                .contains("display_index 5 but only 2 display packages are bound"),
            "{error}"
        );
    }

    #[test]
    fn format_bytes_per_pixel_matches_known_upload_formats() {
        assert_eq!(format_bytes_per_pixel(vk::Format::R8_UNORM).unwrap(), 1);
        assert_eq!(
            format_bytes_per_pixel(vk::Format::R8G8B8A8_UNORM).unwrap(),
            4
        );
        assert!(format_bytes_per_pixel(vk::Format::R32G32B32A32_SFLOAT).is_err());
    }

    #[test]
    fn clear_color_bytes_rounds_the_float_clear_value() {
        assert_eq!(clear_color_bytes(), [31, 46, 89, 255]);
    }

    #[test]
    fn depth_stencil_formats_get_the_stencil_aspect() {
        assert!(depth_aspect_mask(vk::Format::D32_SFLOAT) == vk::ImageAspectFlags::DEPTH);
        assert!(
            depth_aspect_mask(vk::Format::D32_SFLOAT_S8_UINT)
                == vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        );
        assert!(
            depth_aspect_mask(vk::Format::D24_UNORM_S8_UINT)
                == vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        );
    }

    #[test]
    fn converts_unix_seconds_to_utc_civil_time() {
        // 1970-01-01 00:00:00
        assert_eq!(
            civil_from_unix(0),
            WallClock {
                year: 1970,
                month: 1,
                day: 1,
                hours: 0,
                minutes: 0,
                seconds: 0
            }
        );
        // 2000-02-29 12:34:56 (leap day) = 951827696
        assert_eq!(
            civil_from_unix(951_827_696),
            WallClock {
                year: 2000,
                month: 2,
                day: 29,
                hours: 12,
                minutes: 34,
                seconds: 56
            }
        );
        // 2026-07-03 00:00:00 = 1783036800
        assert_eq!(
            civil_from_unix(1_783_036_800),
            WallClock {
                year: 2026,
                month: 7,
                day: 3,
                hours: 0,
                minutes: 0,
                seconds: 0
            }
        );
        // 2023-12-31 23:59:59 = 1704067199
        assert_eq!(
            civil_from_unix(1_704_067_199),
            WallClock {
                year: 2023,
                month: 12,
                day: 31,
                hours: 23,
                minutes: 59,
                seconds: 59
            }
        );
    }

    #[test]
    fn detects_supported_extension_names() {
        let properties = [
            extension_property(b"VK_KHR_get_physical_device_properties2"),
            extension_property(b"VK_KHR_swapchain"),
            extension_property(b"VK_KHR_portability_subset"),
        ];

        assert!(extension_names_contain(
            &properties,
            khr::get_physical_device_properties2::NAME
        ));
        assert!(extension_names_contain(&properties, khr::swapchain::NAME));
        assert!(extension_names_contain(
            &properties,
            khr::portability_subset::NAME
        ));
        assert!(!extension_names_contain(
            &properties,
            khr::portability_enumeration::NAME
        ));
    }

    fn assert_vertex(
        vertex: &TextVertex,
        expected_position: [f32; 2],
        expected_tex_coord: [f32; 2],
    ) {
        for (actual, expected) in vertex.position.iter().zip(expected_position) {
            assert!(
                (actual - expected).abs() < 0.000_1,
                "expected position {expected:?}, got {:?}",
                vertex.position
            );
        }
        for (actual, expected) in vertex.tex_coord.iter().zip(expected_tex_coord) {
            assert!(
                (actual - expected).abs() < 0.000_1,
                "expected tex coords {expected:?}, got {:?}",
                vertex.tex_coord
            );
        }
    }

    fn expected_position(x: i32, y: i32, extent: vk::Extent2D) -> [f32; 2] {
        [
            (2.0 * x as f32 / extent.width as f32) - 1.0,
            -1.0 + (2.0 * y as f32 / extent.height as f32),
        ]
    }

    fn extension_property(name: &[u8]) -> vk::ExtensionProperties {
        let mut extension_property = vk::ExtensionProperties::default();
        for (slot, byte) in extension_property
            .extension_name
            .iter_mut()
            .zip(name.iter().copied())
        {
            *slot = byte as std::ffi::c_char;
        }
        extension_property
    }

    /// A 64x64 fixture with one `Panel` covering the left half in
    /// `Theme.Colors.PrimaryAction` — enough to prove the offscreen path (ADR-016 §1) renders
    /// the exact theme bytes inside the panel and the exact clear color everywhere else, at
    /// pixel coordinates equal to the authored surface (no swapchain, no scaling).
    const OFFSCREEN_TEST_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "OffscreenPixelTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 0,
            padding: 0,
        },
        nodes: &[CompiledNode {
            id: "left-panel",
            bounds: Rect { x: 0, y: 0, width: 32, height: 64 },
            kind: CompiledNodeKind::Panel(trustsc::PanelSpec {
                color_token: "Theme.Colors.PrimaryAction",
            }),
        }],
        golden_references: &[],
    };

    /// Renders [`OFFSCREEN_TEST_SCREEN`] offscreen and asserts, byte-exactly, that a pixel
    /// inside the panel matches `Theme.Colors.PrimaryAction` and a pixel outside it matches the
    /// render pass's clear color — proving the whole offscreen path end to end (headless
    /// instance, no-present device pick, offscreen color target, command recording, one-shot
    /// readback) without ever presenting a window. Skips (rather than fails) when no Vulkan
    /// device is available, so contributors without a loader/driver are not broken; it runs for
    /// real on a MoltenVK-capable Mac and on lavapipe in CI.
    #[test]
    fn offscreen_render_produces_exact_theme_and_clear_color_bytes() {
        let standard = match trustsc::default_standard_text_package() {
            Ok(package) => package,
            Err(error) => {
                eprintln!("skipping offscreen render test: {error}");
                return;
            }
        };
        let displays = trustsc::default_display_text_packages()
            .expect("display packages should load alongside the standard package");
        let text_layout =
            ScreenTextLayout::from_screen(&OFFSCREEN_TEST_SCREEN, standard.clone(), "en-US")
                .expect("layout should build");
        let bindings =
            ScreenBindings::from_screen(&OFFSCREEN_TEST_SCREEN, standard, displays, &[], "en-US")
                .expect("bindings should resolve");

        let mut renderer =
            match OffscreenRenderer::new("offscreen-pixel-test", text_layout, bindings, 64, 64) {
                Ok(renderer) => renderer,
                Err(error) => {
                    eprintln!("skipping offscreen render test: no Vulkan device available: {error}");
                    return;
                }
            };

        let inputs = FrameInputs::from_bindings(
            &ScreenBindings::from_screen(
                &OFFSCREEN_TEST_SCREEN,
                trustsc::default_standard_text_package().expect("standard package"),
                trustsc::default_display_text_packages().expect("display packages"),
                &[],
                "en-US",
            )
            .expect("bindings should resolve"),
        )
        .expect("frame inputs should build from bindings");
        let clock = WallClock {
            year: 2026,
            month: 1,
            day: 1,
            hours: 12,
            minutes: 0,
            seconds: 0,
        };

        renderer
            .draw_frame(&inputs, clock, InteractionSnapshot::default())
            .expect("offscreen frame should render");
        let frame = renderer.read_pixels().expect("pixels should read back");

        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(frame.rgba.len(), 64 * 64 * 4);

        let byte_at = |x: u32, y: u32| -> [u8; 4] {
            let offset = ((y * frame.width + x) * 4) as usize;
            [
                frame.rgba[offset],
                frame.rgba[offset + 1],
                frame.rgba[offset + 2],
                frame.rgba[offset + 3],
            ]
        };
        let expected_bytes = |rgba: [f32; 4]| -> [u8; 4] {
            [
                (rgba[0] * 255.0).round() as u8,
                (rgba[1] * 255.0).round() as u8,
                (rgba[2] * 255.0).round() as u8,
                (rgba[3] * 255.0).round() as u8,
            ]
        };

        let panel_rgba = trustsc::resolve_color_token("Theme.Colors.PrimaryAction")
            .expect("PrimaryAction is a governed theme token");
        assert_eq!(
            byte_at(8, 32),
            expected_bytes(panel_rgba),
            "pixel inside the panel should match Theme.Colors.PrimaryAction exactly"
        );

        let clear_rgba = CLEAR_COLOR_RGBA_F32;
        assert_eq!(
            byte_at(48, 32),
            expected_bytes(clear_rgba),
            "pixel outside the panel should match the render pass clear color exactly"
        );

        assert!(!renderer.device_name().is_empty());
    }

    /// A 64x64 fixture with one `SignalTrace` covering only the bottom half
    /// (`y: 32..64`) — enough to prove (ADR-018) that a flat-amplitude trace paints its own
    /// color exactly at its mid-line and paints nothing (clear color only) both above its
    /// bounds and away from the line within its bounds, i.e. the polyline stays contained.
    const OFFSCREEN_TRACE_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "OffscreenTraceTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 0,
            padding: 0,
        },
        nodes: &[CompiledNode {
            id: "ecg-trace",
            bounds: Rect { x: 0, y: 32, width: 64, height: 32 },
            kind: CompiledNodeKind::SignalTrace(trustsc::SignalTraceSpec {
                stream_source: "TEST_TRACE",
                color_token: "Theme.Colors.Nominal",
            }),
        }],
        golden_references: &[],
    };

    #[test]
    fn offscreen_render_draws_a_signal_trace_contained_within_its_bounds() {
        let standard = match trustsc::default_standard_text_package() {
            Ok(package) => package,
            Err(error) => {
                eprintln!("skipping signal trace offscreen test: {error}");
                return;
            }
        };
        let displays = trustsc::default_display_text_packages()
            .expect("display packages should load alongside the standard package");
        let text_layout =
            ScreenTextLayout::from_screen(&OFFSCREEN_TRACE_SCREEN, standard.clone(), "en-US")
                .expect("layout should build");
        let bindings = ScreenBindings::from_screen(
            &OFFSCREEN_TRACE_SCREEN,
            standard,
            displays,
            &[],
            "en-US",
        )
        .expect("bindings should resolve");

        let mut renderer =
            match OffscreenRenderer::new("offscreen-trace-test", text_layout, bindings, 64, 64) {
                Ok(renderer) => renderer,
                Err(error) => {
                    eprintln!(
                        "skipping signal trace offscreen test: no Vulkan device available: {error}"
                    );
                    return;
                }
            };

        let mut inputs = FrameInputs::from_bindings(
            &ScreenBindings::from_screen(
                &OFFSCREEN_TRACE_SCREEN,
                trustsc::default_standard_text_package().expect("standard package"),
                trustsc::default_display_text_packages().expect("display packages"),
                &[],
                "en-US",
            )
            .expect("bindings should resolve"),
        )
        .expect("frame inputs should build from bindings");

        // Flat-amplitude signal (all zero, the ring's normalized midpoint): the LINE_STRIP
        // renders as a straight horizontal line at the bounds' vertical center.
        for _ in 0..trustsc::realtime::DEFAULT_TRACE_SAMPLES {
            inputs
                .push_sample("TEST_TRACE", 0.0)
                .expect("known trace source");
        }

        let clock = WallClock {
            year: 2026,
            month: 1,
            day: 1,
            hours: 12,
            minutes: 0,
            seconds: 0,
        };

        renderer
            .draw_frame(&inputs, clock, InteractionSnapshot::default())
            .expect("offscreen frame should render");
        let frame = renderer.read_pixels().expect("pixels should read back");

        let byte_at = |x: u32, y: u32| -> [u8; 4] {
            let offset = ((y * frame.width + x) * 4) as usize;
            [
                frame.rgba[offset],
                frame.rgba[offset + 1],
                frame.rgba[offset + 2],
                frame.rgba[offset + 3],
            ]
        };
        let expected_bytes = |rgba: [f32; 4]| -> [u8; 4] {
            [
                (rgba[0] * 255.0).round() as u8,
                (rgba[1] * 255.0).round() as u8,
                (rgba[2] * 255.0).round() as u8,
                (rgba[3] * 255.0).round() as u8,
            ]
        };

        let trace_rgba = trustsc::resolve_color_token("Theme.Colors.Nominal")
            .expect("Nominal is a governed theme token");
        let clear_bytes = expected_bytes(CLEAR_COLOR_RGBA_F32);

        // The bounds span y: 32..64, so the flat line at zero amplitude sits at the vertical
        // center, y=48. Scan a small band for the line rather than asserting one exact row: the
        // NDC-to-pixel rasterization of a 1px line can land on either neighboring row.
        let line_row = (44..=52).find(|&y| byte_at(32, y) == expected_bytes(trace_rgba));
        assert!(
            line_row.is_some(),
            "expected to find the trace color near y=48 within the trace bounds; got {:?}",
            (44..=52).map(|y| byte_at(32, y)).collect::<Vec<_>>()
        );

        // Above the trace's bounds entirely (y=8 < 32): must be pure background, proving the
        // trace does not paint outside its reserved region.
        assert_eq!(
            byte_at(32, 8),
            clear_bytes,
            "pixel above the signal trace's bounds must be untouched background"
        );

        // Inside the bounds but far from the flat line (near the top of the bottom half,
        // y=34): still background, since the line is only 1px tall.
        assert_eq!(
            byte_at(32, 34),
            clear_bytes,
            "pixel inside the trace bounds but away from the flat line must be background"
        );
    }

    /// A `StatusIndicator` with two states tinted from different theme tokens
    /// (`Theme.Colors.Nominal` green / `Theme.Colors.Fault` red) — proves the ADR-018 per-state
    /// color fix: selecting each state must actually paint its own resolved color, not a shared
    /// fixed overlay color both states used to render in regardless of which was active.
    const OFFSCREEN_STATUS_SCREEN: CompiledScreenPackage = CompiledScreenPackage {
        screen_id: "OffscreenStatusTest",
        layout: LayoutSpec {
            kind: LayoutKind::Vertical,
            spacing: 0,
            padding: 0,
        },
        nodes: &[CompiledNode {
            id: "system-status",
            bounds: Rect { x: 16, y: 8, width: 200, height: 48 },
            kind: CompiledNodeKind::StatusIndicator(trustsc::StatusIndicatorSpec {
                requirement_id: "REQ-TEST-STATUS",
                source: "TEST_STATUS",
                state_text_keys: &["STR-NS-NOMINAL", "STR-NS-FAULT"],
                color_tokens: &["Theme.Colors.Nominal", "Theme.Colors.Fault"],
            }),
        }],
        golden_references: &[],
    };

    #[test]
    fn offscreen_render_paints_each_status_state_in_its_own_theme_color() {
        let standard = match trustsc::default_standard_text_package() {
            Ok(package) => package,
            Err(error) => {
                eprintln!("skipping status color offscreen test: {error}");
                return;
            }
        };
        let displays = trustsc::default_display_text_packages()
            .expect("display packages should load alongside the standard package");
        let text_layout =
            ScreenTextLayout::from_screen(&OFFSCREEN_STATUS_SCREEN, standard.clone(), "en-US")
                .expect("layout should build");
        let bindings = ScreenBindings::from_screen(
            &OFFSCREEN_STATUS_SCREEN,
            standard,
            displays,
            &[],
            "en-US",
        )
        .expect("bindings should resolve");

        let mut renderer =
            match OffscreenRenderer::new("offscreen-status-test", text_layout, bindings, 256, 64) {
                Ok(renderer) => renderer,
                Err(error) => {
                    eprintln!(
                        "skipping status color offscreen test: no Vulkan device available: {error}"
                    );
                    return;
                }
            };

        let clock = WallClock {
            year: 2026,
            month: 1,
            day: 1,
            hours: 12,
            minutes: 0,
            seconds: 0,
        };
        let byte_at = |rgba: &[u8], width: u32, x: u32, y: u32| -> [u8; 4] {
            let offset = ((y * width + x) * 4) as usize;
            [rgba[offset], rgba[offset + 1], rgba[offset + 2], rgba[offset + 3]]
        };
        let expected_bytes = |rgba: [f32; 4]| -> [u8; 4] {
            [
                (rgba[0] * 255.0).round() as u8,
                (rgba[1] * 255.0).round() as u8,
                (rgba[2] * 255.0).round() as u8,
                (rgba[3] * 255.0).round() as u8,
            ]
        };
        let contains_color = |rgba: &[u8], width: u32, color: [u8; 4]| -> bool {
            (16..216).any(|x| (8..56).any(|y| byte_at(rgba, width, x, y) == color))
        };

        let nominal_bytes = expected_bytes(
            trustsc::resolve_color_token("Theme.Colors.Nominal").expect("Nominal is a theme token"),
        );
        let fault_bytes = expected_bytes(
            trustsc::resolve_color_token("Theme.Colors.Fault").expect("Fault is a theme token"),
        );

        let bindings_for_inputs = ScreenBindings::from_screen(
            &OFFSCREEN_STATUS_SCREEN,
            trustsc::default_standard_text_package().expect("standard package"),
            trustsc::default_display_text_packages().expect("display packages"),
            &[],
            "en-US",
        )
        .expect("bindings should resolve");

        let mut nominal_inputs = FrameInputs::from_bindings(&bindings_for_inputs)
            .expect("frame inputs should build from bindings");
        nominal_inputs
            .set_status("TEST_STATUS", 0)
            .expect("state 0 exists");
        renderer
            .draw_frame(&nominal_inputs, clock, InteractionSnapshot::default())
            .expect("offscreen frame should render");
        let nominal_frame = renderer.read_pixels().expect("pixels should read back");
        assert!(
            contains_color(&nominal_frame.rgba, nominal_frame.width, nominal_bytes),
            "state 0 (NOMINAL) should paint Theme.Colors.Nominal somewhere in its bounds"
        );
        assert!(
            !contains_color(&nominal_frame.rgba, nominal_frame.width, fault_bytes),
            "state 0 (NOMINAL) must not paint Theme.Colors.Fault anywhere"
        );

        let mut fault_inputs = FrameInputs::from_bindings(&bindings_for_inputs)
            .expect("frame inputs should build from bindings");
        fault_inputs
            .set_status("TEST_STATUS", 1)
            .expect("state 1 exists");
        renderer
            .draw_frame(&fault_inputs, clock, InteractionSnapshot::default())
            .expect("offscreen frame should render");
        let fault_frame = renderer.read_pixels().expect("pixels should read back");
        assert!(
            contains_color(&fault_frame.rgba, fault_frame.width, fault_bytes),
            "state 1 (FAULT) should paint Theme.Colors.Fault somewhere in its bounds"
        );
        assert!(
            !contains_color(&fault_frame.rgba, fault_frame.width, nominal_bytes),
            "state 1 (FAULT) must not paint Theme.Colors.Nominal anywhere"
        );
    }
}
