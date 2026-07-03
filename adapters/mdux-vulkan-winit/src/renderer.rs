//! Raw Vulkan 1.0 renderer for the `mdux-vulkan-winit` presentation adapter (ADR-005/ADR-012 edge
//! adapter: `unsafe` and native Vulkan handles are confined to this module, never crossing into a
//! governed crate's public API). Renders one swapchain-filling clear color plus a single alpha-atlas
//! text overlay built from a [`mdux::screen_text::ScreenTextLayout`].

use std::{
    error::Error,
    ffi::CString,
    io::Cursor,
    mem::{size_of, size_of_val},
    ptr,
};

use ash::{Entry, Instance, khr, util::read_spv, vk};
use mdux::TextRuntime;
use mdux::realtime::{FrameInputs, ScreenBindings};
use mdux::screen_text::ScreenTextLayout;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

pub type BoxError = Box<dyn Error>;

const TEXT_VERT_SPV: &[u8] = include_bytes!("../shaders/generated/hello_text.vert.spv");
const TEXT_FRAG_SPV: &[u8] = include_bytes!("../shaders/generated/hello_text.frag.spv");

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

pub struct VulkanRenderer {
    // Owns the dynamically loaded libvulkan (ash's `loaded` feature dlopens it; dropping the
    // `Entry` dlcloses it). Most device-level calls resolve to ICD entry points and keep working
    // after an early dlclose, but `vkDestroyDevice`, `vkDestroyInstance`, and the surface calls
    // route through loader trampolines inside libvulkan itself — calling them after the library
    // is unmapped segfaults (issue #28). The `Entry` must therefore outlive every other field.
    _entry: Entry,
    instance: Instance,
    surface_loader: khr::surface::Instance,
    surface: vk::SurfaceKHR,
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
    // Layout: [standard-package quads | display-package quads], fixed split at
    // `dynamic_standard_capacity_vertices`. Null/None when the screen has no dynamic text.
    dynamic_vertex_buffer: vk::Buffer,
    dynamic_vertex_buffer_memory: vk::DeviceMemory,
    dynamic_vertex_ptr: Option<std::ptr::NonNull<TextVertex>>,
    dynamic_standard_capacity_vertices: usize,
    dynamic_display_capacity_vertices: usize,
    // Second glyph atlas (display package, 48px digits) with its own descriptor set; the
    // pipeline layout is shared (identically defined set layouts are compatible).
    display_atlas: TextAtlasResources,
    display_descriptor_set_layout: vk::DescriptorSetLayout,
    display_descriptor_pool: vk::DescriptorPool,
    display_descriptor_set: vk::DescriptorSet,
    // Depth attachment (recreated with the swapchain), required by the 3D waterfall pipeline.
    depth_image: vk::Image,
    depth_image_memory: vk::DeviceMemory,
    depth_image_view: vk::ImageView,
    depth_format: vk::Format,
    current_extent: vk::Extent2D,
}

impl VulkanRenderer {
    pub fn new(
        window: &Window,
        app_name: &str,
        text_layout: ScreenTextLayout,
        bindings: ScreenBindings,
    ) -> Result<Self, BoxError> {
        let entry = unsafe { Entry::load()? };
        let instance = create_instance(&entry, window, app_name)?;
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
            pick_physical_device(&instance, &surface_loader, surface)?;
        let (device, graphics_queue, present_queue) =
            create_logical_device(&instance, physical_device, queue_families)?;
        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);
        let command_pool = create_command_pool(&device, queue_families.graphics)?;
        let (image_available_semaphore, render_finished_semaphore, in_flight_fence) =
            create_sync_objects(&device)?;

        // Validate both realtime packages once, here: the frame loop then uses
        // `TextRuntime::from_validated_package`, which must not re-run (allocating) validation.
        TextRuntime::<1>::new(&bindings.standard)?;
        TextRuntime::<1>::new(&bindings.display)?;
        let depth_format = find_depth_format(&instance, physical_device)?;

        let mut renderer = Self {
            _entry: entry,
            instance,
            surface_loader,
            surface,
            physical_device,
            device,
            graphics_queue,
            present_queue,
            queue_families,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_image_views: Vec::new(),
            render_pass: vk::RenderPass::null(),
            framebuffers: Vec::new(),
            command_pool,
            command_buffers: Vec::new(),
            image_available_semaphore,
            render_finished_semaphore,
            in_flight_fence,
            device_name,
            text_layout,
            text_atlas: TextAtlasResources::default(),
            text_descriptor_set_layout: vk::DescriptorSetLayout::null(),
            text_descriptor_pool: vk::DescriptorPool::null(),
            text_descriptor_set: vk::DescriptorSet::null(),
            text_pipeline_layout: vk::PipelineLayout::null(),
            text_pipeline: vk::Pipeline::null(),
            text_vertex_buffer: vk::Buffer::null(),
            text_vertex_buffer_memory: vk::DeviceMemory::null(),
            text_vertex_count: 0,
            bindings,
            dynamic_vertex_buffer: vk::Buffer::null(),
            dynamic_vertex_buffer_memory: vk::DeviceMemory::null(),
            dynamic_vertex_ptr: None,
            dynamic_standard_capacity_vertices: 0,
            dynamic_display_capacity_vertices: 0,
            display_atlas: TextAtlasResources::default(),
            display_descriptor_set_layout: vk::DescriptorSetLayout::null(),
            display_descriptor_pool: vk::DescriptorPool::null(),
            display_descriptor_set: vk::DescriptorSet::null(),
            depth_image: vk::Image::null(),
            depth_image_memory: vk::DeviceMemory::null(),
            depth_image_view: vk::ImageView::null(),
            depth_format,
            current_extent: vk::Extent2D { width: 0, height: 0 },
        };

        renderer.create_text_static_resources()?;
        renderer.create_dynamic_text_resources()?;
        renderer.recreate_swapchain(window)?;
        Ok(renderer)
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn draw_frame(
        &mut self,
        window: &Window,
        inputs: &FrameInputs,
        clock: WallClock,
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
        // buffer and the dynamic vertex buffer has fully completed, so both are safe to rewrite.
        let (dynamic_standard_vertices, dynamic_display_vertices) =
            self.write_dynamic_vertices(inputs, clock)?;
        self.record_command_buffer(
            image_index as usize,
            dynamic_standard_vertices,
            dynamic_display_vertices,
        )?;

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

        let support =
            query_swapchain_support(self.physical_device, &self.surface_loader, self.surface)?;
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
            .surface(self.surface)
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

        self.render_pass = create_render_pass(&self.device, surface_format.format, self.depth_format)?;
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
        let view = create_image_view(&self.device, image, self.depth_format, vk::ImageAspectFlags::DEPTH)?;
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
    /// vertex buffer sized from the screen's bindings ([standard quads | display quads]) and,
    /// when numeric displays exist, the display package's atlas + descriptor set. Nothing here
    /// runs per frame (ADR-013).
    fn create_dynamic_text_resources(&mut self) -> Result<(), BoxError> {
        let standard_quads: usize = self
            .bindings
            .clocks
            .iter()
            .map(|binding| binding.capacity)
            .chain(self.bindings.statuses.iter().map(|binding| binding.capacity))
            .sum();
        let display_quads: usize = self
            .bindings
            .numbers
            .iter()
            .map(|binding| binding.capacity)
            .sum();

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
        {
            if capacity > DYNAMIC_RUN_CAPACITY {
                return Err(box_error(format!(
                    "realtime binding {node_id} needs {capacity} glyphs, above the adapter's per-run capacity of {DYNAMIC_RUN_CAPACITY}"
                )));
            }
        }

        self.dynamic_standard_capacity_vertices = standard_quads * 6;
        self.dynamic_display_capacity_vertices = display_quads * 6;
        let total_vertices =
            self.dynamic_standard_capacity_vertices + self.dynamic_display_capacity_vertices;
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

        if display_quads > 0 {
            self.display_atlas = create_text_atlas_resources(
                &self.instance,
                &self.device,
                self.physical_device,
                self.command_pool,
                self.graphics_queue,
                &self.bindings.display,
            )?;
            let (descriptor_set_layout, descriptor_pool, descriptor_set) =
                create_text_descriptor_resources(&self.device, &self.display_atlas)?;
            self.display_descriptor_set_layout = descriptor_set_layout;
            self.display_descriptor_pool = descriptor_pool;
            self.display_descriptor_set = descriptor_set;
        }

        Ok(())
    }

    /// Rewrites the persistently mapped dynamic vertex buffer from the drained frame inputs:
    /// clocks and status states from the standard package (first range), numeric displays from
    /// the display package (second range). Returns the vertex counts of both ranges. No
    /// allocation: bounded ArrayVec renders + in-place quad writes.
    fn write_dynamic_vertices(
        &mut self,
        inputs: &FrameInputs,
        clock: WallClock,
    ) -> Result<(u32, u32), BoxError> {
        let Some(pointer) = self.dynamic_vertex_ptr else {
            return Ok((0, 0));
        };
        let extent = self.current_extent;
        let surface_width = extent.width.max(1) as f32;
        let surface_height = extent.height.max(1) as f32;
        let total_vertices =
            self.dynamic_standard_capacity_vertices + self.dynamic_display_capacity_vertices;
        // Safety: the buffer was mapped once at creation with exactly this vertex capacity, and
        // the in-flight fence waited in draw_frame guarantees the GPU is done reading it.
        let vertices =
            unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), total_vertices) };

        let standard_runtime =
            TextRuntime::<DYNAMIC_RUN_CAPACITY>::from_validated_package(&self.bindings.standard);
        let display_runtime =
            TextRuntime::<DYNAMIC_RUN_CAPACITY>::from_validated_package(&self.bindings.display);

        let mut standard_cursor = 0usize;
        for binding in &self.bindings.clocks {
            match binding.format {
                mdux::ClockFormat::TimeSeconds => {
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
                mdux::ClockFormat::DateTimeSeconds => {
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

        for binding in &self.bindings.statuses {
            let state_index = usize::from(inputs.status_index(binding.source).unwrap_or(0));
            let run_id = binding.state_run_ids.get(state_index).ok_or_else(|| {
                box_error(format!(
                    "status {} has no run for state {state_index}",
                    binding.node_id
                ))
            })?;
            let (origin_x, origin_y) = binding.state_origins[state_index];
            let commands = standard_runtime.render_run(run_id, origin_x, origin_y)?;
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

        let mut display_cursor = self.dynamic_standard_capacity_vertices;
        for binding in &self.bindings.numbers {
            let value = inputs.number(binding.source).unwrap_or(0);
            let commands = display_runtime.render_numeric_template(
                &binding.template_id,
                value,
                binding.origin_x,
                binding.origin_y,
            )?;
            write_glyph_quads(
                vertices,
                &mut display_cursor,
                total_vertices,
                &commands,
                &self.bindings.display,
                surface_width,
                surface_height,
            )?;
        }

        Ok((
            standard_cursor as u32,
            (display_cursor - self.dynamic_standard_capacity_vertices) as u32,
        ))
    }

    fn create_text_swapchain_resources(&mut self, extent: vk::Extent2D) -> Result<(), BoxError> {
        let (vertex_buffer, vertex_buffer_memory, vertex_count) = create_text_vertex_buffer(
            &self.instance,
            &self.device,
            self.physical_device,
            &self.text_layout,
            extent,
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
        dynamic_display_vertices: u32,
    ) -> Result<(), BoxError> {
        let command_buffer = self.command_buffers[image_index];
        let extent = self.current_extent;
        let begin_info = vk::CommandBufferBeginInfo::default();
        unsafe {
            self.device.reset_command_buffer(
                command_buffer,
                vk::CommandBufferResetFlags::empty(),
            )?;
            self.device.begin_command_buffer(command_buffer, &begin_info)?;
        }

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.12, 0.18, 0.35, 1.0],
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

            let has_static = self.text_pipeline != vk::Pipeline::null() && self.text_vertex_count > 0;
            let has_dynamic = self.text_pipeline != vk::Pipeline::null()
                && (dynamic_standard_vertices > 0 || dynamic_display_vertices > 0);

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

                    if dynamic_display_vertices > 0 {
                        let descriptor_sets = [self.display_descriptor_set];
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
                            dynamic_display_vertices,
                            1,
                            self.dynamic_standard_capacity_vertices as u32,
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
        }
    }

    fn destroy_text_static_objects(&mut self) {
        unsafe {
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
            if self.display_descriptor_pool != vk::DescriptorPool::null() {
                self.device
                    .destroy_descriptor_pool(self.display_descriptor_pool, None);
                self.display_descriptor_pool = vk::DescriptorPool::null();
            }
            self.display_descriptor_set = vk::DescriptorSet::null();
            if self.display_descriptor_set_layout != vk::DescriptorSetLayout::null() {
                self.device
                    .destroy_descriptor_set_layout(self.display_descriptor_set_layout, None);
                self.display_descriptor_set_layout = vk::DescriptorSetLayout::null();
            }
            if self.display_atlas.sampler != vk::Sampler::null() {
                self.device.destroy_sampler(self.display_atlas.sampler, None);
                self.display_atlas.sampler = vk::Sampler::null();
            }
            if self.display_atlas.image_view != vk::ImageView::null() {
                self.device
                    .destroy_image_view(self.display_atlas.image_view, None);
                self.display_atlas.image_view = vk::ImageView::null();
            }
            if self.display_atlas.image != vk::Image::null() {
                self.device.destroy_image(self.display_atlas.image, None);
                self.display_atlas.image = vk::Image::null();
            }
            if self.display_atlas.memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.display_atlas.memory, None);
                self.display_atlas.memory = vk::DeviceMemory::null();
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
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

fn create_instance(entry: &Entry, window: &Window, app_name: &str) -> Result<Instance, BoxError> {
    let app_name = CString::new(app_name)?;
    let engine_name = CString::new("mdux-vulkan-winit")?;
    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_0);

    #[cfg(target_os = "macos")]
    let (required_extensions, instance_flags) = {
        let mut required_extensions =
            ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?.to_vec();
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
    let (required_extensions, instance_flags) = (
        ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?.to_vec(),
        vk::InstanceCreateFlags::empty(),
    );

    let instance_info = vk::InstanceCreateInfo::default()
        .flags(instance_flags)
        .application_info(&app_info)
        .enabled_extension_names(&required_extensions);

    let instance = unsafe { entry.create_instance(&instance_info, None)? };
    Ok(instance)
}

fn pick_physical_device(
    instance: &Instance,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
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

    Err(box_error(
        "no Vulkan device with graphics and present support was found",
    ))
}

fn find_queue_families(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface_loader: &khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<Option<QueueFamilies>, BoxError> {
    let queue_families =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    let mut graphics = None;
    let mut present = None;

    for (index, queue_family) in queue_families.iter().enumerate() {
        if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics = Some(index as u32);
        }

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

    Ok(None)
}

fn create_logical_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_families: QueueFamilies,
) -> Result<(ash::Device, vk::Queue, vk::Queue), BoxError> {
    let priorities = [1.0_f32];
    let mut queue_infos = vec![
        vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_families.graphics)
            .queue_priorities(&priorities),
    ];

    if queue_families.graphics != queue_families.present {
        queue_infos.push(
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_families.present)
                .queue_priorities(&priorities),
        );
    }

    #[cfg(target_os = "macos")]
    let extensions = {
        let mut extensions = vec![khr::swapchain::NAME.as_ptr()];
        let available_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device)? };
        if extension_names_contain(&available_extensions, khr::portability_subset::NAME) {
            extensions.push(khr::portability_subset::NAME.as_ptr());
        }
        extensions
    };

    #[cfg(not(target_os = "macos"))]
    let extensions = vec![khr::swapchain::NAME.as_ptr()];

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

fn create_render_pass(
    device: &ash::Device,
    format: vk::Format,
    depth_format: vk::Format,
) -> Result<vk::RenderPass, BoxError> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
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
    let dependency = vk::SubpassDependency::default()
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

    let attachments = [color_attachment, depth_attachment];
    let subpasses = [subpass];
    let dependencies = [dependency];
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
    package: &mdux::TextPackage,
) -> Result<TextAtlasResources, BoxError> {
    let atlas = package
        .atlases
        .first()
        .ok_or_else(|| box_error("screen text package does not contain an atlas"))?;
    let image_size = vk::DeviceSize::try_from(atlas.pixels.len())?;
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

    if let Err(error) = write_buffer(device, staging_memory, &atlas.pixels) {
        destroy_staging(device);
        return Err(error);
    }

    let (image, memory) = match create_image(
        instance,
        device,
        physical_device,
        atlas.width.into(),
        atlas.height.into(),
        vk::Format::R8_UNORM,
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
            atlas.width.into(),
            atlas.height.into(),
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

    let image_view = match create_image_view(
        device,
        image,
        vk::Format::R8_UNORM,
        vk::ImageAspectFlags::COLOR,
    ) {
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

    let pipeline =
        pipeline_result.map_err(|(_, error)| box_error(format!("failed to create text pipeline: {error}")))?[0];

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
    extent: vk::Extent2D,
) -> Result<Vec<TextVertex>, BoxError> {
    let atlas = text_layout
        .package
        .atlases
        .first()
        .ok_or_else(|| box_error("screen text package does not contain an atlas"))?;
    let width = extent.width.max(1) as f32;
    let height = extent.height.max(1) as f32;
    let atlas_width = atlas.width as f32;
    let atlas_height = atlas.height as f32;
    let commands = text_layout
        .runs
        .iter()
        .flat_map(|run| run.commands.iter());
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

/// Per-render-call glyph budget of the dynamic text path; every realtime binding's capacity is
/// checked against it at construction, so the frame loop's ArrayVecs can never overflow.
const DYNAMIC_RUN_CAPACITY: usize = 64;

/// Writes one render call's glyph commands as quads into the mapped dynamic buffer, advancing
/// `cursor` and enforcing the range's fixed capacity.
fn write_glyph_quads(
    vertices: &mut [TextVertex],
    cursor: &mut usize,
    range_end: usize,
    commands: &[mdux::GlyphDrawCommand],
    package: &mdux::TextPackage,
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
    package: &mdux::TextPackage,
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
    command: &mdux::GlyphDrawCommand,
    glyph: &mdux::AtlasGlyph,
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
    use mdux::{
        CompiledNode, CompiledNodeKind, CompiledScreenPackage, CriticalButtonSpec,
        DEFAULT_STANDARD_HELLO_WORLD_STRING_ID, LayoutKind, LayoutSpec, Rect, SystemEvent,
        default_standard_text_package,
    };

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
    fn converts_unix_seconds_to_utc_civil_time() {
        // 1970-01-01 00:00:00
        assert_eq!(
            civil_from_unix(0),
            WallClock { year: 1970, month: 1, day: 1, hours: 0, minutes: 0, seconds: 0 }
        );
        // 2000-02-29 12:34:56 (leap day) = 951827696
        assert_eq!(
            civil_from_unix(951_827_696),
            WallClock { year: 2000, month: 2, day: 29, hours: 12, minutes: 34, seconds: 56 }
        );
        // 2026-07-03 00:00:00 = 1783036800
        assert_eq!(
            civil_from_unix(1_783_036_800),
            WallClock { year: 2026, month: 7, day: 3, hours: 0, minutes: 0, seconds: 0 }
        );
        // 2023-12-31 23:59:59 = 1704067199
        assert_eq!(
            civil_from_unix(1_704_067_199),
            WallClock { year: 2023, month: 12, day: 31, hours: 23, minutes: 59, seconds: 59 }
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
}
