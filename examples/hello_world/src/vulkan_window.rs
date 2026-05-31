use std::{
    error::Error,
    ffi::CString,
    io::Cursor,
    mem::{size_of, size_of_val},
    process,
    ptr,
    time::{Duration, Instant},
};

use ash::{khr, util::read_spv, vk, Entry, Instance};
use mdux::HelloWorldDemoRun;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use crate::hello_text;

type BoxError = Box<dyn Error>;

const TEXT_VERT_SPV: &[u8] = include_bytes!(env!("HELLO_WORLD_TEXT_VERT_SPV"));
const TEXT_FRAG_SPV: &[u8] = include_bytes!(env!("HELLO_WORLD_TEXT_FRAG_SPV"));

pub fn run(demo: HelloWorldDemoRun, auto_close_after: Option<Duration>) -> Result<(), BoxError> {
    let config = demo.framework.ui_runtime().config().clone();
    let greeting = demo
        .framework
        .ui_runtime()
        .components()
        .first()
        .map(|component| component.label.clone())
        .unwrap_or_else(|| "Hello world".to_string());

    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title(format!("{greeting} - Vulkan"))
        .with_inner_size(LogicalSize::new(config.width as f64, config.height as f64))
        .build(&event_loop)?;

    let mut renderer = Some(VulkanRenderer::new(&window, &demo)?);
    println!(
        "vulkan_device={}",
        renderer
            .as_ref()
            .map(|value| value.device_name())
            .unwrap_or("unknown")
    );

    let started_at = Instant::now();
    let window_id = window.id();
    event_loop.run(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { window_id: id, event } if id == window_id => match event {
                WindowEvent::CloseRequested => {
                    process::exit(0);
                }
                WindowEvent::Resized(_) => {}
                WindowEvent::RedrawRequested => {
                    if let Some(active_renderer) = renderer.as_mut() {
                        if let Err(error) = active_renderer.draw_frame(&window) {
                            eprintln!("failed to render frame: {error}");
                            process::exit(1);
                        }
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                if let Some(auto_close_after) = auto_close_after {
                    if started_at.elapsed() >= auto_close_after {
                        process::exit(0);
                    }
                }

                window.request_redraw();
            }
            Event::LoopExiting => {}
            _ => {}
        }
    })?;

    Ok(())
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
        Self { position, tex_coord }
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

struct VulkanRenderer {
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
    text_layout: hello_text::HelloWorldTextLayout,
    text_atlas: TextAtlasResources,
    text_descriptor_set_layout: vk::DescriptorSetLayout,
    text_descriptor_pool: vk::DescriptorPool,
    text_descriptor_set: vk::DescriptorSet,
    text_pipeline_layout: vk::PipelineLayout,
    text_pipeline: vk::Pipeline,
    text_vertex_buffer: vk::Buffer,
    text_vertex_buffer_memory: vk::DeviceMemory,
    text_vertex_count: u32,
}

impl VulkanRenderer {
    fn new(window: &Window, demo: &HelloWorldDemoRun) -> Result<Self, BoxError> {
        let entry = unsafe { Entry::load()? };
        let instance = create_instance(&entry, window, demo)?;
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
        let text_layout = hello_text::hello_world_text_layout_from_dsl()
            .map_err(|error| box_error(format!("failed to prepare hello world text package: {error}")))?;

        let mut renderer = Self {
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
        };

        renderer.create_text_static_resources()?;
        renderer.recreate_swapchain(window)?;
        Ok(renderer)
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn draw_frame(&mut self, window: &Window) -> Result<(), BoxError> {
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
                return Err(box_error(format!("failed to acquire swapchain image: {error}")));
            }
        };

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

        let present_result =
            unsafe { self.swapchain_loader.queue_present(self.present_queue, &present_info) };

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
            .map(|&image| create_image_view(&self.device, image, surface_format.format, vk::ImageAspectFlags::COLOR))
            .collect::<Result<Vec<_>, _>>()?;

        self.render_pass = create_render_pass(&self.device, surface_format.format)?;
        self.framebuffers = self
            .swapchain_image_views
            .iter()
            .map(|&image_view| create_framebuffer(&self.device, self.render_pass, image_view, extent))
            .collect::<Result<Vec<_>, _>>()?;

        self.create_text_swapchain_resources(extent)?;
        self.command_buffers = allocate_command_buffers(
            &self.device,
            self.command_pool,
            self.framebuffers.len() as u32,
        )?;
        self.record_command_buffers(extent)?;

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
        let (descriptor_set_layout, descriptor_pool, descriptor_set) = create_text_descriptor_resources(
            &self.device,
            &self.text_atlas,
        )?;
        self.text_descriptor_set_layout = descriptor_set_layout;
        self.text_descriptor_pool = descriptor_pool;
        self.text_descriptor_set = descriptor_set;
        self.text_pipeline_layout = create_text_pipeline_layout(
            &self.device,
            self.text_descriptor_set_layout,
        )?;
        Ok(())
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

    fn record_command_buffers(&mut self, extent: vk::Extent2D) -> Result<(), BoxError> {
        unsafe {
            self.device
                .reset_command_pool(self.command_pool, vk::CommandPoolResetFlags::empty())?;
        }

        for (index, command_buffer) in self.command_buffers.iter().enumerate() {
            let begin_info = vk::CommandBufferBeginInfo::default();
            unsafe {
                self.device.begin_command_buffer(*command_buffer, &begin_info)?;
            }

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.12, 0.18, 0.35, 1.0],
                },
            }];
            let render_area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            };
            let render_pass_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[index])
                .render_area(render_area)
                .clear_values(&clear_values);

            unsafe {
                self.device.cmd_begin_render_pass(
                    *command_buffer,
                    &render_pass_info,
                    vk::SubpassContents::INLINE,
                );

                if self.text_pipeline != vk::Pipeline::null() && self.text_vertex_count > 0 {
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
                    let vertex_buffers = [self.text_vertex_buffer];
                    let offsets = [0];
                    let descriptor_sets = [self.text_descriptor_set];
                    let push_constants = TextPushConstants::overlay();

                    self.device.cmd_bind_pipeline(
                        *command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.text_pipeline,
                    );
                    self.device
                        .cmd_set_viewport(*command_buffer, 0, &[viewport]);
                    self.device.cmd_set_scissor(*command_buffer, 0, &[scissor]);
                    self.device.cmd_bind_descriptor_sets(
                        *command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.text_pipeline_layout,
                        0,
                        &descriptor_sets,
                        &[],
                    );
                    self.device.cmd_bind_vertex_buffers(
                        *command_buffer,
                        0,
                        &vertex_buffers,
                        &offsets,
                    );
                    self.device.cmd_push_constants(
                        *command_buffer,
                        self.text_pipeline_layout,
                        vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                        0,
                        bytes_of(&push_constants),
                    );
                    self.device
                        .cmd_draw(*command_buffer, self.text_vertex_count, 1, 0, 0);
                }

                self.device.cmd_end_render_pass(*command_buffer);
                self.device.end_command_buffer(*command_buffer)?;
            }
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

            if self.text_vertex_buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.text_vertex_buffer, None);
                self.text_vertex_buffer = vk::Buffer::null();
            }
            if self.text_vertex_buffer_memory != vk::DeviceMemory::null() {
                self.device.free_memory(self.text_vertex_buffer_memory, None);
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
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
        }
    }

    fn destroy_text_static_objects(&mut self) {
        unsafe {
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
                self.device.destroy_image_view(self.text_atlas.image_view, None);
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

fn create_instance(
    entry: &Entry,
    window: &Window,
    demo: &HelloWorldDemoRun,
) -> Result<Instance, BoxError> {
    let app_name = CString::new(demo.framework.identity().name.clone())?;
    let engine_name = CString::new("MduX-rust hello_world")?;
    let app_info = vk::ApplicationInfo::default()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_0);

    let required_extensions =
        ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?;
    let instance_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(required_extensions);

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

    let extensions = [khr::swapchain::NAME.as_ptr()];
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&extensions);

    let device = unsafe { instance.create_device(physical_device, &create_info, None)? };
    let graphics_queue = unsafe { device.get_device_queue(queue_families.graphics, 0) };
    let present_queue = unsafe { device.get_device_queue(queue_families.present, 0) };
    Ok((device, graphics_queue, present_queue))
}

fn create_command_pool(device: &ash::Device, queue_family: u32) -> Result<vk::CommandPool, BoxError> {
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
    let capabilities =
        unsafe { surface_loader.get_physical_device_surface_capabilities(physical_device, surface)? };
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

fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> Result<vk::SurfaceFormatKHR, BoxError> {
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
        width: size
            .width
            .clamp(capabilities.min_image_extent.width, capabilities.max_image_extent.width),
        height: size
            .height
            .clamp(capabilities.min_image_extent.height, capabilities.max_image_extent.height),
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

fn create_render_pass(device: &ash::Device, format: vk::Format) -> Result<vk::RenderPass, BoxError> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
    let color_reference = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let color_references = [color_reference];
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_references);
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        );

    let attachments = [color_attachment];
    let subpasses = [subpass];
    let dependencies = [dependency];
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    let render_pass = unsafe { device.create_render_pass(&create_info, None)? };
    Ok(render_pass)
}

fn create_framebuffer(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    image_view: vk::ImageView,
    extent: vk::Extent2D,
) -> Result<vk::Framebuffer, BoxError> {
    let attachments = [image_view];
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
        .ok_or_else(|| box_error("hello world text package does not contain an atlas"))?;
    let image_size = vk::DeviceSize::try_from(atlas.pixels.len())?;
    let (staging_buffer, staging_memory) = create_buffer(
        instance,
        device,
        physical_device,
        image_size,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    write_buffer(device, staging_memory, &atlas.pixels)?;

    let (image, memory) = create_image(
        instance,
        device,
        physical_device,
        atlas.width.into(),
        atlas.height.into(),
        vk::Format::R8_UNORM,
        vk::ImageTiling::OPTIMAL,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    transition_image_layout(
        device,
        command_pool,
        graphics_queue,
        image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    )?;
    copy_buffer_to_image(
        device,
        command_pool,
        graphics_queue,
        staging_buffer,
        image,
        atlas.width.into(),
        atlas.height.into(),
    )?;
    transition_image_layout(
        device,
        command_pool,
        graphics_queue,
        image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    )?;

    unsafe {
        device.destroy_buffer(staging_buffer, None);
        device.free_memory(staging_memory, None);
    }

    let image_view = create_image_view(device, image, vk::Format::R8_UNORM, vk::ImageAspectFlags::COLOR)?;
    let sampler_info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::NEAREST)
        .min_filter(vk::Filter::NEAREST)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .max_lod(1.0);
    let sampler = unsafe { device.create_sampler(&sampler_info, None)? };

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
) -> Result<(vk::DescriptorSetLayout, vk::DescriptorPool, vk::DescriptorSet), BoxError> {
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
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);

    let pipeline = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|(_, error)| box_error(format!("failed to create text pipeline: {error}")))?[0];

    unsafe {
        device.destroy_shader_module(vertex_shader_module, None);
        device.destroy_shader_module(fragment_shader_module, None);
    }

    Ok(pipeline)
}

fn create_text_vertex_buffer(
    instance: &Instance,
    device: &ash::Device,
    physical_device: vk::PhysicalDevice,
    text_layout: &hello_text::HelloWorldTextLayout,
    extent: vk::Extent2D,
) -> Result<(vk::Buffer, vk::DeviceMemory, u32), BoxError> {
    let vertices = build_text_vertices(text_layout, extent)?;
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
    text_layout: &hello_text::HelloWorldTextLayout,
    extent: vk::Extent2D,
) -> Result<Vec<TextVertex>, BoxError> {
    let atlas = text_layout
        .package
        .atlases
        .first()
        .ok_or_else(|| box_error("hello world text package does not contain an atlas"))?;
    let width = extent.width.max(1) as f32;
    let height = extent.height.max(1) as f32;
    let atlas_width = atlas.width as f32;
    let atlas_height = atlas.height as f32;
    let mut vertices = Vec::with_capacity(text_layout.commands.len() * 6);

    for command in &text_layout.commands {
        let glyph = text_layout
            .package
            .find_glyph(command.atlas_index, command.glyph_id)
            .ok_or_else(|| {
                box_error(format!(
                    "missing atlas glyph {} for hello world overlay",
                    command.glyph_id
                ))
            })?;

        let left = (2.0 * command.x as f32 / width) - 1.0;
        let right = (2.0 * (command.x + i32::from(command.width)) as f32 / width) - 1.0;
        let top = -1.0 + (2.0 * command.y as f32 / height);
        let bottom = -1.0 + (2.0 * (command.y + i32::from(command.height)) as f32 / height);

        let u0 = glyph.x as f32 / atlas_width;
        let v0 = glyph.y as f32 / atlas_height;
        let u1 = (glyph.x + glyph.width) as f32 / atlas_width;
        let v1 = (glyph.y + glyph.height) as f32 / atlas_height;

        vertices.extend_from_slice(&[
            TextVertex::new([left, top], [u0, v0]),
            TextVertex::new([right, top], [u1, v0]),
            TextVertex::new([right, bottom], [u1, v1]),
            TextVertex::new([left, top], [u0, v0]),
            TextVertex::new([right, bottom], [u1, v1]),
            TextVertex::new([left, bottom], [u0, v1]),
        ]);
    }

    Ok(vertices)
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

fn write_buffer(device: &ash::Device, memory: vk::DeviceMemory, bytes: &[u8]) -> Result<(), BoxError> {
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
        (
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ) => (
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
    let begin_info = vk::CommandBufferBeginInfo::default()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
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
    let memory_properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_textured_quads_for_the_hello_world_overlay() {
        let layout = hello_text::hello_world_text_layout_from_dsl()
            .expect("hello world layout should compile");
        let extent = vk::Extent2D {
            width: 320,
            height: 128,
        };
        let vertices = build_text_vertices(&layout, extent).expect("vertex generation should succeed");
        let atlas = layout.package.atlases.first().expect("atlas should exist");
        let first_glyph = layout
            .package
            .find_glyph(layout.commands[0].atlas_index, layout.commands[0].glyph_id)
            .expect("first glyph should exist");
        let first_command = &layout.commands[0];
        let last_command = layout.commands.last().expect("last command should exist");
        let last_glyph = layout
            .package
            .find_glyph(last_command.atlas_index, last_command.glyph_id)
            .expect("last glyph should exist");

        assert_eq!(vertices.len(), layout.commands.len() * 6);
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
        assert_vertex(
            &vertices[1],
            expected_position(
                first_command.x + i32::from(first_command.width),
                first_command.y,
                extent,
            ),
            [
                (first_glyph.x + first_glyph.width) as f32 / atlas.width as f32,
                first_glyph.y as f32 / atlas.height as f32,
            ],
        );
        assert_vertex(
            &vertices[2],
            expected_position(
                first_command.x + i32::from(first_command.width),
                first_command.y + i32::from(first_command.height),
                extent,
            ),
            [
                (first_glyph.x + first_glyph.width) as f32 / atlas.width as f32,
                (first_glyph.y + first_glyph.height) as f32 / atlas.height as f32,
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
            &last_quad[1],
            expected_position(
                last_command.x + i32::from(last_command.width),
                last_command.y,
                extent,
            ),
            [
                (last_glyph.x + last_glyph.width) as f32 / atlas.width as f32,
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
    fn rejects_text_vertex_generation_without_an_atlas() {
        let mut layout = hello_text::hello_world_text_layout_from_dsl()
            .expect("hello world layout should compile");
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

    fn assert_vertex(vertex: &TextVertex, expected_position: [f32; 2], expected_tex_coord: [f32; 2]) {
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
}
