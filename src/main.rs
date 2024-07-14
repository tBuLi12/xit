use std::borrow::Cow;
use std::collections::HashMap;
use std::default::Default;
use std::error::Error;
use std::ffi;
use std::fs;
use std::hash::Hash;
use std::io::Cursor;
use std::mem;
use std::mem::offset_of;

use ash::ext::debug_utils;
use ash::khr::surface;
use ash::vk;
use ash::vk::DescriptorPoolCreateInfo;
use ash::vk::DescriptorSetAllocateInfo;
use ash::vk::DescriptorSetLayout;
use ash::vk::DescriptorSetLayoutBinding;
use ash::vk::DescriptorType;
use ash::vk::PipelineVertexInputStateCreateInfo;
use ash::vk::ShaderStageFlags;
use cosmic_text::ttf_parser::head;
use notify::event;
use notify::Watcher;
use static_ui::Color;
use static_ui::Component;

use static_ui::Point;
use winit::event::ElementState;
use winit::event::Event;
use winit::event::KeyEvent;
use winit::event::MouseScrollDelta;
use winit::event::WindowEvent;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::keyboard::Key;
use winit::keyboard::NamedKey;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window;
use winit::window::WindowAttributes;

mod gen_ref;
mod pin_signal;
mod signal;
mod static_ui;
// mod ui;

struct Renderer {
    entry: ash::Entry,
    instance: ash::Instance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    present_queue: vk::Queue,
    command_pool: vk::CommandPool,
    draw_command_buffer: vk::CommandBuffer,
    pipeline_layout: vk::PipelineLayout,
    graphic_pipeline: vk::Pipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    uniform_buffers: Vec<Buffer>,
    instance_input_buffers: Vec<Buffer>,
    swapchain_stuff: SwapchainStuff,
    renderpass: vk::RenderPass,
    draw_commands_reuse_fence: vk::Fence,
    present_complete_semaphore: vk::Semaphore,
    rendering_complete_semaphore: vk::Semaphore,
    rectangles: Vec<RenderedRectangle>,
    surface: vk::SurfaceKHR,
    surface_loader: surface::Instance,
    surface_format: vk::SurfaceFormatKHR,
    desired_image_count: u32,
    vertex_shader_module: vk::ShaderModule,
    fragment_shader_module: vk::ShaderModule,
    pre_transform: vk::SurfaceTransformFlagsKHR,
    present_mode: vk::PresentModeKHR,
    font_system: cosmic_text::FontSystem,
    swash_cache: cosmic_text::SwashCache,
    atlas: Atlas,
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
struct ViewportSize {
    width: f32,
    height: f32,
}

#[derive(Copy, Clone, Debug)]
struct Rectangle {
    pos: [f32; 2],
    size: [f32; 2],
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
struct RenderedRectangle {
    pos: [f32; 2],
    size: [f32; 2],
    tex_coords: [f32; 2],
    tex_blend: f32,
    bg_color: Color,
    border_color: Color,
    border_width: f32,
    corner_radius: f32,
}

#[derive(Clone, Copy, Debug)]
struct CachedGlyph {
    left: f32,
    top: f32,
    size: [f32; 2],
    tex_position: Option<[f32; 2]>,
}

struct Atlas {
    texture: vk::Image,
    texture_view: vk::ImageView,
    texture_memory: vk::DeviceMemory,
    sampler: vk::Sampler,
    current_texture_layout: vk::ImageLayout,
    upload_buffer: Buffer,
    glyph_cache: HashMap<cosmic_text::CacheKey, CachedGlyph>,
    allocator: etagere::BucketedAtlasAllocator,
}

impl Atlas {
    fn upload(
        &mut self,
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        position: [u32; 2],
        size: [u32; 2],
    ) {
        unsafe {
            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY);

            let command_buffer = device
                .allocate_command_buffers(&command_buffer_allocate_info)
                .expect("Failed to allocate Command Buffers!")[0];

            let command_buffer_begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            device
                .begin_command_buffer(command_buffer, &command_buffer_begin_info)
                .expect("Failed to begin recording Command Buffer at beginning!");

            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);

            if self.current_texture_layout == vk::ImageLayout::UNDEFINED {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .image(self.texture)
                        .subresource_range(subresource_range)],
                );
            } else {
                device.cmd_pipeline_barrier(
                    command_buffer,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .image(self.texture)
                        .subresource_range(subresource_range)],
                );
            }

            let buffer_image_regions = [vk::BufferImageCopy {
                image_subresource: vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
                image_extent: vk::Extent3D {
                    width: size[0],
                    height: size[1],
                    depth: 1,
                },
                buffer_offset: 0,
                buffer_image_height: size[1],
                buffer_row_length: size[0],
                image_offset: vk::Offset3D {
                    x: position[0] as i32,
                    y: position[1] as i32,
                    z: 0,
                },
            }];

            device.cmd_copy_buffer_to_image(
                command_buffer,
                self.upload_buffer.handle,
                self.texture,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &buffer_image_regions,
            );

            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(self.texture)
                    .subresource_range(subresource_range)],
            );

            device
                .end_command_buffer(command_buffer)
                .expect("Failed to record Command Buffer at Ending!");

            let buffers_to_submit = [command_buffer];

            let submit_infos = [vk::SubmitInfo::default().command_buffers(&buffers_to_submit)];

            println!("submitting upload");

            device
                .queue_submit(queue, &submit_infos, vk::Fence::null())
                .expect("Failed to Queue Submit!");
            device
                .queue_wait_idle(queue)
                .expect("Failed to wait Queue idle!");
            device.free_command_buffers(command_pool, &buffers_to_submit);
            println!("done upload");

            self.current_texture_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        }
    }
}

fn create_atlas(
    device: &ash::Device,
    width: u32,
    height: u32,
    required_memory_properties: vk::MemoryPropertyFlags,
    device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
) -> Atlas {
    unsafe {
        let upload_buffer = create_buffer(
            device,
            4096,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            device_memory_properties,
        );

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8_UNORM)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .tiling(vk::ImageTiling::OPTIMAL)
            .samples(vk::SampleCountFlags::TYPE_1)
            .mip_levels(1)
            .array_layers(1)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        let texture = device.create_image(&image_create_info, None).unwrap();

        let texture_memory_req = device.get_image_memory_requirements(texture);
        let texture_allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(texture_memory_req.size)
            .memory_type_index(
                find_memorytype_index(
                    &texture_memory_req,
                    device_memory_properties,
                    required_memory_properties,
                )
                .unwrap(),
            );

        let texture_memory = device
            .allocate_memory(&texture_allocate_info, None)
            .unwrap();
        device
            .bind_image_memory(texture, texture_memory, 0)
            .unwrap();

        let texture_view_create_info = vk::ImageViewCreateInfo::default()
            .image(texture)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::R8_UNORM)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            );

        let texture_view = device
            .create_image_view(&texture_view_create_info, None)
            .unwrap();

        let sampler_create_info = vk::SamplerCreateInfo::default()
            .min_filter(vk::Filter::NEAREST)
            .mag_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_anisotropy(1.0)
            .unnormalized_coordinates(true);

        let sampler = device.create_sampler(&sampler_create_info, None).unwrap();

        Atlas {
            texture,
            texture_view,
            texture_memory,
            sampler,
            upload_buffer,
            glyph_cache: HashMap::new(),
            allocator: etagere::BucketedAtlasAllocator::new(etagere::size2(
                width as i32,
                height as i32,
            )),
            current_texture_layout: vk::ImageLayout::UNDEFINED,
        }
    }
}

fn create_buffer(
    device: &ash::Device,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    required_memory_properties: vk::MemoryPropertyFlags,
    device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
) -> Buffer {
    unsafe {
        let buffer_info = vk::BufferCreateInfo::default().size(size).usage(usage);

        let buffer = device.create_buffer(&buffer_info, None).unwrap();

        let buffer_memory_req = device.get_buffer_memory_requirements(buffer);

        let buffer_memory_index = find_memorytype_index(
            &buffer_memory_req,
            &device_memory_properties,
            required_memory_properties,
        )
        .expect("Unable to find suitable memory type for the buffer.");

        let allocate_info = vk::MemoryAllocateInfo {
            allocation_size: buffer_memory_req.size,
            memory_type_index: buffer_memory_index,
            ..Default::default()
        };

        let buffer_memory = device.allocate_memory(&allocate_info, None).unwrap();

        device.bind_buffer_memory(buffer, buffer_memory, 0).unwrap();

        Buffer {
            handle: buffer,
            memory: buffer_memory,
            size: buffer_memory_req.size,
        }
    }
}

impl Renderer {
    fn new(window: &window::Window) -> Result<Self, Box<dyn Error>> {
        unsafe {
            // let entry = ash::Entry::load_from("C:/Users/tbuli/Downloads/vulkan-1.dll").unwrap();
            let entry = ash::Entry::load().unwrap();

            let (entry, instance) = unsafe {
                let app_info =
                    vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 3, 0));

                let mut extension_names =
                    ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())
                        .unwrap()
                        .iter()
                        .copied()
                        .chain(std::iter::once(
                            ash::vk::EXT_DEBUG_UTILS_NAME.as_ptr() as *const _
                        ))
                        .collect::<Vec<_>>();

                // let layer_names = [b"VK_LAYER_KHRONOS_validation\0".as_ptr() as *const _];
                let layer_names = [b"VK_LAYER_KHRONOS_validation\0".as_ptr() as *const _];

                let create_info = vk::InstanceCreateInfo::default()
                    .application_info(&app_info)
                    .enabled_layer_names(&layer_names)
                    .enabled_extension_names(&extension_names);

                let instance: ash::Instance = entry
                    .create_instance(&create_info, None)
                    .expect("Instance creation error");

                (entry, instance)
            };

            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(vulkan_debug_callback));

            let debug_utils_loader = debug_utils::Instance::new(&entry, &instance);
            debug_utils_loader
                .create_debug_utils_messenger(&debug_info, None)
                .unwrap();

            let surface = unsafe {
                ash_window::create_surface(
                    &entry,
                    &instance,
                    window.display_handle()?.as_raw(),
                    window.window_handle()?.as_raw(),
                    None,
                )
                .unwrap()
            };

            let surface_loader = surface::Instance::new(&entry, &instance);

            let (physical_device, queue_family_index) = unsafe {
                let physical_devices = instance
                    .enumerate_physical_devices()
                    .expect("Physical device error");

                let (physical_device, queue_family_index) = physical_devices
                    .iter()
                    .find_map(|&physical_device| {
                        instance
                            .get_physical_device_queue_family_properties(physical_device)
                            .iter()
                            .enumerate()
                            .find_map(|(index, info)| {
                                let supports_graphic_and_surface =
                                    info.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                                        && surface_loader
                                            .get_physical_device_surface_support(
                                                physical_device,
                                                index as u32,
                                                surface,
                                            )
                                            .unwrap();
                                if supports_graphic_and_surface {
                                    Some((physical_device, index))
                                } else {
                                    None
                                }
                            })
                    })
                    .expect("Couldn't find suitable device.");
                (physical_device, queue_family_index as u32)
            };

            let device_extension_names_raw = [ash::khr::swapchain::NAME.as_ptr()];
            let features = vk::PhysicalDeviceFeatures {
                shader_clip_distance: 1,
                ..Default::default()
            };
            let priorities = [1.0];

            let queue_info = vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family_index)
                .queue_priorities(&priorities);

            let device_create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(std::slice::from_ref(&queue_info))
                .enabled_extension_names(&device_extension_names_raw)
                .enabled_features(&features);

            let device = instance
                .create_device(physical_device, &device_create_info, None)
                .unwrap();

            let present_queue = device.get_device_queue(queue_family_index, 0);

            let surface_format = surface_loader
                .get_physical_device_surface_formats(physical_device, surface)
                .unwrap()[0];

            let surface_capabilities = surface_loader
                .get_physical_device_surface_capabilities(physical_device, surface)
                .unwrap();
            let mut desired_image_count = surface_capabilities.min_image_count + 1;
            if surface_capabilities.max_image_count > 0
                && desired_image_count > surface_capabilities.max_image_count
            {
                desired_image_count = surface_capabilities.max_image_count;
            }
            let mut surface_resolution = match surface_capabilities.current_extent.width {
                u32::MAX => vk::Extent2D {
                    width: 1024,
                    height: 1024,
                },
                _ => surface_capabilities.current_extent,
            };
            let pre_transform = if surface_capabilities
                .supported_transforms
                .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
            {
                vk::SurfaceTransformFlagsKHR::IDENTITY
            } else {
                surface_capabilities.current_transform
            };

            let present_modes = unsafe {
                surface_loader
                    .get_physical_device_surface_present_modes(physical_device, surface)
                    .unwrap()
            };

            println!("{:?}", present_modes);

            let present_mode = present_modes
                .iter()
                .cloned()
                .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
                .unwrap_or(vk::PresentModeKHR::FIFO);

            let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

            let pool_create_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);

            let command_pool = device.create_command_pool(&pool_create_info, None).unwrap();

            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
                .command_buffer_count(2)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY);

            let command_buffers = device
                .allocate_command_buffers(&command_buffer_allocate_info)
                .unwrap();
            let setup_command_buffer = command_buffers[0];
            let draw_command_buffer = command_buffers[1];

            let (mut swapchain, mut present_images, mut present_image_views) = create_swapchain(
                &device,
                &instance,
                &swapchain_loader,
                surface,
                surface_format,
                surface_resolution,
                desired_image_count,
                pre_transform,
                present_mode,
            );

            let device_memory_properties =
                instance.get_physical_device_memory_properties(physical_device);

            let fence_create_info =
                vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

            let draw_commands_reuse_fence = device
                .create_fence(&fence_create_info, None)
                .expect("Create fence failed.");

            let semaphore_create_info = vk::SemaphoreCreateInfo::default();

            let present_complete_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .unwrap();
            let rendering_complete_semaphore = device
                .create_semaphore(&semaphore_create_info, None)
                .unwrap();

            let renderpass_attachments = [
                vk::AttachmentDescription {
                    format: surface_format.format,
                    samples: vk::SampleCountFlags::TYPE_1,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                    ..Default::default()
                },
                // vk::AttachmentDescription {
                //     format: vk::Format::D16_UNORM,
                //     samples: vk::SampleCountFlags::TYPE_1,
                //     load_op: vk::AttachmentLoadOp::CLEAR,
                //     initial_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                //     final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                //     ..Default::default()
                // },
            ];
            let color_attachment_refs = [vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }];
            // let depth_attachment_ref = vk::AttachmentReference {
            //     attachment: 1,
            //     layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            // };
            let dependencies = [vk::SubpassDependency {
                src_subpass: vk::SUBPASS_EXTERNAL,
                src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                ..Default::default()
            }];

            let subpass = vk::SubpassDescription::default()
                .color_attachments(&color_attachment_refs)
                // .depth_stencil_attachment(&depth_attachment_ref)
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

            let renderpass_create_info = vk::RenderPassCreateInfo::default()
                .attachments(&renderpass_attachments)
                .subpasses(std::slice::from_ref(&subpass))
                .dependencies(&dependencies);

            let renderpass = device
                .create_render_pass(&renderpass_create_info, None)
                .unwrap();

            let mut framebuffers = create_framebuffers(
                &device,
                renderpass,
                &present_image_views,
                // depth_image_view,
                surface_resolution,
            );

            let instance_input_buffers = present_images
                .iter()
                .map(|_| {
                    create_buffer(
                        &device,
                        1024 * mem::size_of::<RenderedRectangle>() as u64,
                        vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                        vk::MemoryPropertyFlags::HOST_VISIBLE
                            | vk::MemoryPropertyFlags::HOST_COHERENT,
                        &device_memory_properties,
                    )
                })
                .collect();

            let rectangles = vec![];

            let uniform_buffers: Vec<Buffer> = present_images
                .iter()
                .map(|_| {
                    let buffer = create_buffer(
                        &device,
                        mem::size_of::<ViewportSize>() as u64,
                        vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                        vk::MemoryPropertyFlags::HOST_VISIBLE
                            | vk::MemoryPropertyFlags::HOST_COHERENT,
                        &device_memory_properties,
                    );

                    buffer
                })
                .collect();

            let mut vertex_spv_file = Cursor::new(&include_bytes!("../vert.spv")[..]);
            let mut frag_spv_file = Cursor::new(&include_bytes!("../frag.spv")[..]);

            let vertex_code = ash::util::read_spv(&mut vertex_spv_file)
                .expect("Failed to read vertex shader spv file");
            let vertex_shader_info = vk::ShaderModuleCreateInfo::default().code(&vertex_code);

            let frag_code = ash::util::read_spv(&mut frag_spv_file)
                .expect("Failed to read fragment shader spv file");
            let frag_shader_info = vk::ShaderModuleCreateInfo::default().code(&frag_code);

            let vertex_shader_module = device
                .create_shader_module(&vertex_shader_info, None)
                .expect("Vertex shader module error");

            let fragment_shader_module = device
                .create_shader_module(&frag_shader_info, None)
                .expect("Fragment shader module error");

            let bindings = [
                DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .stage_flags(ShaderStageFlags::VERTEX)
                    .descriptor_type(DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1),
                DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .stage_flags(ShaderStageFlags::FRAGMENT)
                    .descriptor_type(DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1),
            ];

            let set_layout_create_info =
                vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

            let set_layout = device
                .create_descriptor_set_layout(&set_layout_create_info, None)
                .unwrap();

            let set_layouts = [set_layout];

            let layout_create_info =
                vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);

            let pipeline_layout = device
                .create_pipeline_layout(&layout_create_info, None)
                .unwrap();

            let shader_entry_name = ffi::CStr::from_bytes_with_nul_unchecked(b"main\0");
            let shader_stage_create_infos = [
                vk::PipelineShaderStageCreateInfo {
                    module: vertex_shader_module,
                    p_name: shader_entry_name.as_ptr(),
                    stage: vk::ShaderStageFlags::VERTEX,
                    ..Default::default()
                },
                vk::PipelineShaderStageCreateInfo {
                    module: fragment_shader_module,
                    p_name: shader_entry_name.as_ptr(),
                    stage: vk::ShaderStageFlags::FRAGMENT,
                    ..Default::default()
                },
            ];
            let vertex_input_binding_descriptions = [vk::VertexInputBindingDescription {
                binding: 0,
                stride: mem::size_of::<RenderedRectangle>() as u32,
                input_rate: vk::VertexInputRate::INSTANCE,
            }];
            let vertex_input_attribute_descriptions = [
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, pos) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, size) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, tex_coords) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 3,
                    binding: 0,
                    format: vk::Format::R32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, tex_blend) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 4,
                    binding: 0,
                    format: vk::Format::R32G32B32A32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, bg_color) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 5,
                    binding: 0,
                    format: vk::Format::R32G32B32A32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, border_color) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 6,
                    binding: 0,
                    format: vk::Format::R32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, border_width) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 7,
                    binding: 0,
                    format: vk::Format::R32_SFLOAT,
                    offset: offset_of!(RenderedRectangle, corner_radius) as u32,
                },
            ];

            let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
                topology: vk::PrimitiveTopology::TRIANGLE_LIST,
                ..Default::default()
            };
            let mut viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: surface_resolution.width as f32,
                height: surface_resolution.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            let mut scissors = [surface_resolution.into()];
            let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
                .scissors(&scissors)
                .viewports(&viewports);

            let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
                front_face: vk::FrontFace::COUNTER_CLOCKWISE,
                line_width: 1.0,
                polygon_mode: vk::PolygonMode::FILL,
                ..Default::default()
            };
            let multisample_state_info = vk::PipelineMultisampleStateCreateInfo {
                rasterization_samples: vk::SampleCountFlags::TYPE_1,
                ..Default::default()
            };

            let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
                blend_enable: vk::TRUE,
                src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
                dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                color_blend_op: vk::BlendOp::ADD,
                src_alpha_blend_factor: vk::BlendFactor::ONE,
                dst_alpha_blend_factor: vk::BlendFactor::ZERO,
                alpha_blend_op: vk::BlendOp::ADD,
                color_write_mask: vk::ColorComponentFlags::RGBA,
            }];
            let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
                .logic_op(vk::LogicOp::CLEAR)
                .attachments(&color_blend_attachment_states);

            let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic_state_info =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_state);

            let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::default()
                .vertex_attribute_descriptions(&vertex_input_attribute_descriptions)
                .vertex_binding_descriptions(&vertex_input_binding_descriptions);

            let graphic_pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&shader_stage_create_infos)
                .vertex_input_state(&vertex_input_state_info)
                .input_assembly_state(&vertex_input_assembly_state_info)
                .viewport_state(&viewport_state_info)
                .rasterization_state(&rasterization_info)
                .multisample_state(&multisample_state_info)
                // .depth_stencil_state(&depth_state_info)
                .color_blend_state(&color_blend_state)
                .dynamic_state(&dynamic_state_info)
                .layout(pipeline_layout)
                .render_pass(renderpass);

            let graphics_pipelines = device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &[graphic_pipeline_info],
                    None,
                )
                .expect("Unable to create graphics pipeline");

            let graphic_pipeline = graphics_pipelines[0];

            let pool_sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::UNIFORM_BUFFER,
                    descriptor_count: present_images.len() as u32,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: present_images.len() as u32,
                },
            ];

            let descriptor_pool = device
                .create_descriptor_pool(
                    &DescriptorPoolCreateInfo::default()
                        .max_sets(present_images.len() as u32 * 2)
                        .pool_sizes(&pool_sizes),
                    None,
                )
                .unwrap();

            let all_set_layouts = (0..present_images.len())
                .map(|_| set_layout)
                .collect::<Vec<_>>();

            let descriptor_sets = device
                .allocate_descriptor_sets(
                    &DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&all_set_layouts),
                )
                .unwrap();

            println!("{:?}", descriptor_sets.len());
            println!("{:?}", present_images.len());

            let atlas = create_atlas(
                &device,
                2048,
                2048,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
                &device_memory_properties,
            );

            for (i, &descritptor_set) in descriptor_sets.iter().enumerate() {
                let descriptor_buffer_info = [vk::DescriptorBufferInfo {
                    buffer: uniform_buffers[i].handle,
                    offset: 0,
                    range: std::mem::size_of::<ViewportSize>() as u64,
                }];

                let descriptor_image_info = [vk::DescriptorImageInfo {
                    image_view: atlas.texture_view,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    sampler: atlas.sampler,
                }];

                let descriptor_write_sets = [
                    vk::WriteDescriptorSet::default()
                        .dst_set(descritptor_set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .descriptor_count(1)
                        .buffer_info(&descriptor_buffer_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(descritptor_set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .descriptor_count(1)
                        .image_info(&descriptor_image_info),
                ];

                unsafe {
                    device.update_descriptor_sets(&descriptor_write_sets, &[]);
                }
            }

            let font_system = cosmic_text::FontSystem::new();
            let swash_cache = cosmic_text::SwashCache::new();

            Ok(Self {
                entry,
                instance,
                device,
                physical_device,
                surface,
                surface_loader,
                present_queue,
                command_pool,
                draw_command_buffer,
                renderpass,
                pipeline_layout,
                graphic_pipeline,
                descriptor_pool,
                descriptor_sets,
                uniform_buffers,
                instance_input_buffers,
                draw_commands_reuse_fence,
                present_complete_semaphore,
                rendering_complete_semaphore,
                rectangles,
                fragment_shader_module,
                vertex_shader_module,
                desired_image_count,
                pre_transform,
                present_mode,
                surface_format,
                swapchain_stuff: SwapchainStuff {
                    swapchain_loader,
                    swapchain,
                    present_images,
                    present_image_views,
                    framebuffers,
                    surface_resolution,
                    viewports,
                    scissors,
                },
                font_system,
                swash_cache,
                atlas,
            })
        }
    }

    fn draw_frame(&mut self) -> Result<(), Box<dyn Error>> {
        unsafe {
            let (present_index, _) = self
                .swapchain_stuff
                .swapchain_loader
                .acquire_next_image(
                    self.swapchain_stuff.swapchain,
                    u64::MAX,
                    self.present_complete_semaphore,
                    vk::Fence::null(),
                )
                .unwrap();

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            }];

            let render_pass_begin_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.renderpass)
                .framebuffer(self.swapchain_stuff.framebuffers[present_index as usize])
                .render_area(self.swapchain_stuff.surface_resolution.into())
                .clear_values(&clear_values);

            self.uniform_buffers[present_index as usize].copy_value(
                &self.device,
                ViewportSize {
                    width: self.swapchain_stuff.surface_resolution.width as f32,
                    height: self.swapchain_stuff.surface_resolution.height as f32,
                },
            );

            self.instance_input_buffers[present_index as usize]
                .copy_from_slice(&self.device, &self.rectangles);

            record_submit_commandbuffer(
                &self.device,
                self.draw_command_buffer,
                self.draw_commands_reuse_fence,
                self.present_queue,
                &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
                &[self.present_complete_semaphore],
                &[self.rendering_complete_semaphore],
                |device, draw_command_buffer| {
                    device.cmd_begin_render_pass(
                        draw_command_buffer,
                        &render_pass_begin_info,
                        vk::SubpassContents::INLINE,
                    );
                    device.cmd_bind_pipeline(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.graphic_pipeline,
                    );

                    device.cmd_bind_descriptor_sets(
                        draw_command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_layout,
                        0,
                        &[self.descriptor_sets[present_index as usize]],
                        &[],
                    );
                    device.cmd_bind_vertex_buffers(
                        draw_command_buffer,
                        0,
                        &[self.instance_input_buffers[present_index as usize].handle],
                        &[0],
                    );
                    device.cmd_set_viewport(
                        draw_command_buffer,
                        0,
                        &self.swapchain_stuff.viewports,
                    );
                    device.cmd_set_scissor(draw_command_buffer, 0, &self.swapchain_stuff.scissors);
                    device.cmd_draw(draw_command_buffer, 6, self.rectangles.len() as u32, 0, 0);
                    device.cmd_end_render_pass(draw_command_buffer);
                },
            );
            let wait_semaphors = [self.rendering_complete_semaphore];
            let swapchains = [self.swapchain_stuff.swapchain];
            let image_indices = [present_index];
            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&wait_semaphors) // &rendering_complete_semaphore)
                .swapchains(&swapchains)
                .image_indices(&image_indices);

            self.swapchain_stuff
                .swapchain_loader
                .queue_present(self.present_queue, &present_info)
                .unwrap();
        }

        Ok(())
    }

    fn recreate_swapchain(&mut self, window: &window::Window) -> Result<(), Box<dyn Error>> {
        unsafe {
            self.device.device_wait_idle().unwrap();

            let surface_format = self
                .surface_loader
                .get_physical_device_surface_formats(self.physical_device, self.surface)
                .unwrap()[0];

            let [width, height]: [u32; 2] = window.inner_size().into();

            self.swapchain_stuff.surface_resolution = vk::Extent2D { width, height };

            self.swapchain_stuff.viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.swapchain_stuff.surface_resolution.width as f32,
                height: self.swapchain_stuff.surface_resolution.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.swapchain_stuff.scissors = [self.swapchain_stuff.surface_resolution.into()];

            self.swapchain_stuff
                .swapchain_loader
                .destroy_swapchain(self.swapchain_stuff.swapchain, None);

            for image_view in &self.swapchain_stuff.present_image_views {
                self.device.destroy_image_view(*image_view, None);
            }

            let (new_swapchain, new_present_images, new_present_image_views) = create_swapchain(
                &self.device,
                &self.instance,
                &self.swapchain_stuff.swapchain_loader,
                self.surface,
                self.surface_format,
                self.swapchain_stuff.surface_resolution,
                self.desired_image_count,
                self.pre_transform,
                self.present_mode,
            );

            self.swapchain_stuff.swapchain = new_swapchain;
            self.swapchain_stuff.present_images = new_present_images;
            self.swapchain_stuff.present_image_views = new_present_image_views;

            self.swapchain_stuff.framebuffers = create_framebuffers(
                &self.device,
                self.renderpass,
                &self.swapchain_stuff.present_image_views,
                // depth_image_view,
                self.swapchain_stuff.surface_resolution,
            );
        }

        Ok(())
    }

    fn free_buffer(&self, buffer: Buffer) {
        unsafe {
            self.device.destroy_buffer(buffer.handle, None);
            self.device.free_memory(buffer.memory, None);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Buffer {
    handle: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
}

impl Buffer {
    fn copy_from_slice<T: Copy>(&self, device: &ash::Device, data: &[T]) {
        unsafe {
            let instance_ptr = device
                .map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())
                .unwrap();

            instance_ptr.copy_from_nonoverlapping(
                data.as_ptr() as *const _,
                data.len() * mem::size_of::<T>(),
            );
            device.unmap_memory(self.memory);
        }
    }

    fn copy_value<T>(&self, device: &ash::Device, value: T) {
        unsafe {
            let uniform_ptr = device
                .map_memory(
                    self.memory,
                    0,
                    mem::size_of::<T>() as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap() as *mut T;

            uniform_ptr.copy_from_nonoverlapping(&value, 1);
            device.unmap_memory(self.memory);
        }
    }
}

struct SwapchainStuff {
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    present_images: Vec<vk::Image>,
    present_image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    surface_resolution: vk::Extent2D,
    viewports: [vk::Viewport; 1],
    scissors: [vk::Rect2D; 1],
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_pipeline(self.graphic_pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_shader_module(self.vertex_shader_module, None);
            self.device
                .destroy_shader_module(self.fragment_shader_module, None);
            for &framebuffer in &self.swapchain_stuff.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            for buffer in &self.instance_input_buffers {
                self.free_buffer(*buffer);
            }
            for &buffer in &self.uniform_buffers {
                self.free_buffer(buffer);
            }
            self.device.destroy_render_pass(self.renderpass, None);
        }
    }
}

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

pub fn record_submit_commandbuffer<F: FnOnce(&ash::Device, vk::CommandBuffer)>(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    command_buffer_reuse_fence: vk::Fence,
    submit_queue: vk::Queue,
    wait_mask: &[vk::PipelineStageFlags],
    wait_semaphores: &[vk::Semaphore],
    signal_semaphores: &[vk::Semaphore],
    f: F,
) {
    unsafe {
        device
            .wait_for_fences(&[command_buffer_reuse_fence], true, u64::MAX)
            .expect("Wait for fence failed.");

        device
            .reset_fences(&[command_buffer_reuse_fence])
            .expect("Reset fences failed.");

        device
            .reset_command_buffer(
                command_buffer,
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
            .expect("Reset command buffer failed.");

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        device
            .begin_command_buffer(command_buffer, &command_buffer_begin_info)
            .expect("Begin commandbuffer");
        f(device, command_buffer);
        device
            .end_command_buffer(command_buffer)
            .expect("End commandbuffer");

        let command_buffers = vec![command_buffer];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(signal_semaphores);

        device
            .queue_submit(submit_queue, &[submit_info], command_buffer_reuse_fence)
            .expect("queue submit failed.");
    }
}

unsafe fn create_swapchain(
    device: &ash::Device,
    instance: &ash::Instance,
    loader: &ash::khr::swapchain::Device,
    surface: vk::SurfaceKHR,
    surface_format: vk::SurfaceFormatKHR,
    surface_resolution: vk::Extent2D,
    desired_image_count: u32,
    pre_transform: vk::SurfaceTransformFlagsKHR,
    present_mode: vk::PresentModeKHR,
) -> (vk::SwapchainKHR, Vec<vk::Image>, Vec<vk::ImageView>) {
    println!("resolution: {:?}", surface_resolution);

    let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(desired_image_count)
        .image_color_space(surface_format.color_space)
        .image_format(surface_format.format)
        .image_extent(surface_resolution)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(pre_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .image_array_layers(1);

    println!("creating");

    let swapchain = loader
        .create_swapchain(&swapchain_create_info, None)
        .unwrap();

    let present_images = loader.get_swapchain_images(swapchain).unwrap();
    println!("getting present images");
    let present_image_views: Vec<vk::ImageView> = present_images
        .iter()
        .map(|&image| {
            let create_view_info = vk::ImageViewCreateInfo::default()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(surface_format.format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image);
            println!("creating image views");
            device.create_image_view(&create_view_info, None).unwrap()
        })
        .collect();

    println!("swapchain created");

    (swapchain, present_images, present_image_views)
}

unsafe fn create_framebuffers(
    device: &ash::Device,
    renderpass: vk::RenderPass,
    present_image_views: &[vk::ImageView],
    // depth_image_view: vk::ImageView,
    surface_resolution: vk::Extent2D,
) -> Vec<vk::Framebuffer> {
    let framebuffers: Vec<vk::Framebuffer> = present_image_views
        .iter()
        .map(|&present_image_view| {
            // let framebuffer_attachments = [present_image_view, depth_image_view];
            let framebuffer_attachments = [present_image_view];
            let frame_buffer_create_info = vk::FramebufferCreateInfo::default()
                .render_pass(renderpass)
                .attachments(&framebuffer_attachments)
                .width(surface_resolution.width)
                .height(surface_resolution.height)
                .layers(1);

            device
                .create_framebuffer(&frame_buffer_create_info, None)
                .unwrap()
        })
        .collect();

    framebuffers
}

impl static_ui::Runtime for Renderer {
    fn draw_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        corner_radius: f32,
        border_width: f32,
        bg_color: Color,
        border_color: Color,
    ) {
        self.rectangles.push(RenderedRectangle {
            pos: [x, y],
            size: [width, height],
            tex_coords: [0.0, 0.0],
            tex_blend: 0.0,
            bg_color,
            border_color,
            border_width,
            corner_radius,
        });
    }

    fn draw_glyph(&mut self, x: f32, y: f32, size: [f32; 2], tex_coords: [f32; 2], color: Color) {
        self.rectangles.push(RenderedRectangle {
            pos: [x, y],
            size: size,
            tex_coords,
            tex_blend: 1.0,
            bg_color: color,
            border_color: Color::clear(),
            border_width: 0.0,
            corner_radius: 0.0,
        })
    }

    fn font_system(&mut self) -> &mut cosmic_text::FontSystem {
        &mut self.font_system
    }

    fn get_glyph(&mut self, key: cosmic_text::CacheKey) -> Option<CachedGlyph> {
        if let Some(glyph) = self.atlas.glyph_cache.get(&key) {
            return Some(*glyph);
        }

        let Some(image) = self
            .swash_cache
            .get_image_uncached(&mut self.font_system, key)
        else {
            println!("No glyph");
            return None;
        };

        let mut glyph = CachedGlyph {
            left: image.placement.left as f32,
            top: image.placement.top as f32,
            size: [image.placement.width as f32, image.placement.height as f32],
            tex_position: None,
        };

        if image.data.len() == 0 {
            return Some(glyph);
        }

        let upload_buffer = self.atlas.upload_buffer;
        if (upload_buffer.size as usize) < image.data.len() {
            panic!(
                "Upload buffer too small {} < {}",
                upload_buffer.size,
                image.data.len()
            );
        }

        upload_buffer.copy_from_slice(&self.device, &image.data);

        println!(
            "allocating {}x{}",
            image.placement.width, image.placement.height
        );

        let tex_glyph_rect = self
            .atlas
            .allocator
            .allocate(etagere::size2(
                image.placement.width as i32,
                image.placement.height as i32,
            ))
            .unwrap();

        self.atlas.upload(
            &self.device,
            self.command_pool,
            self.present_queue,
            [
                tex_glyph_rect.rectangle.min.x as u32,
                tex_glyph_rect.rectangle.min.y as u32,
            ],
            [image.placement.width, image.placement.height],
        );

        glyph.tex_position = Some([
            tex_glyph_rect.rectangle.min.x as f32,
            tex_glyph_rect.rectangle.min.y as f32,
        ]);

        self.atlas.glyph_cache.insert(key, glyph);

        Some(glyph)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    unsafe {
        let event_loop = EventLoop::<notify::Event>::with_user_event().build()?;
        let window = event_loop.create_window(WindowAttributes::new())?;

        let mut renderer = Renderer::new(&window)?;

        let mut mouse_x = 0.0;
        let mut mouse_y = 0.0;

        let proxy = event_loop.create_proxy();

        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                match event {
                    Ok(event) => {
                        proxy.send_event(event).unwrap();
                    }
                    Err(error) => eprintln!("watch error: {error}"),
                };
            })
            .unwrap();

        let current_dir = std::env::current_dir().unwrap();

        watcher
            .watch(&current_dir, notify::RecursiveMode::Recursive)
            .unwrap();
        watcher.unwatch(&current_dir.join(".git")).unwrap();

        let file_tree = static_ui::FileForest::from_path(&current_dir);

        let mut app_ui = static_ui::App::new(file_tree);

        app_ui.set_bounds(
            static_ui::Size {
                width: window.inner_size().width as f32,
                height: window.inner_size().height as f32,
            },
            &mut renderer,
        );

        event_loop
            .run(move |event, elwp| {
                elwp.set_control_flow(ControlFlow::Wait);

                match event {
                    Event::WindowEvent {
                        event:
                            WindowEvent::CloseRequested
                            | WindowEvent::KeyboardInput {
                                event:
                                    KeyEvent {
                                        state: ElementState::Pressed,
                                        logical_key: Key::Named(NamedKey::Escape),
                                        ..
                                    },
                                ..
                            },
                        ..
                    } => {
                        elwp.exit();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::RedrawRequested,
                        ..
                    } => {
                        renderer.rectangles.clear();
                        app_ui.draw(
                            static_ui::Point { x: 0.0, y: 0.0 },
                            Some(Point {
                                x: mouse_x,
                                y: mouse_y,
                            }),
                            &mut renderer,
                        );
                        renderer.draw_frame().unwrap();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::Resized(size),
                        ..
                    } => {
                        renderer.recreate_swapchain(&window).unwrap();
                        app_ui.set_bounds(
                            static_ui::Size {
                                width: window.inner_size().width as f32,
                                height: window.inner_size().height as f32,
                            },
                            &mut renderer,
                        );
                    }
                    Event::WindowEvent {
                        event: WindowEvent::CursorMoved { position, .. },
                        ..
                    } => {
                        app_ui.mouse_move(
                            position.x as f32 - mouse_x,
                            position.y as f32 - mouse_y,
                            &mut renderer,
                        );
                        mouse_x = position.x as f32;
                        mouse_y = position.y as f32;
                        window.request_redraw();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::MouseInput { state, button, .. },
                        ..
                    } => {
                        match state {
                            ElementState::Pressed => {
                                app_ui.click(
                                    static_ui::Point {
                                        x: mouse_x,
                                        y: mouse_y,
                                    },
                                    &mut renderer,
                                );
                            }
                            ElementState::Released => {
                                app_ui.mouse_up(
                                    static_ui::Point {
                                        x: mouse_x,
                                        y: mouse_y,
                                    },
                                    &mut renderer,
                                );
                            }
                        }
                        window.request_redraw();
                    }
                    Event::WindowEvent {
                        event:
                            WindowEvent::MouseWheel {
                                delta: MouseScrollDelta::LineDelta(dx, dy),
                                ..
                            },
                        ..
                    } => {
                        app_ui.scroll(dx * 100.0, dy * 100.0, &mut renderer);
                        window.request_redraw();
                    }
                    Event::WindowEvent {
                        event: WindowEvent::KeyboardInput { event, .. },
                        ..
                    } => {
                        eprintln!("{:?}", event);
                        if let Some(text) = event.text {
                            eprintln!("textn {}", &text);
                            app_ui.key_pressed(&text, &mut renderer);
                            window.request_redraw();
                        }
                    }
                    Event::UserEvent(event) => {
                        if event.need_rescan() {
                            app_ui.rescan_files(&mut renderer);
                        }

                        eprintln!("Event: {:?}", event);
                        match event.kind {
                            notify::EventKind::Create(_) => {
                                app_ui.add_files(&event.paths, &mut renderer);
                                window.request_redraw();
                            }
                            notify::EventKind::Remove(_) => {
                                app_ui.remove_files(&event.paths, &mut renderer);
                                window.request_redraw();
                            }
                            notify::EventKind::Modify(notify::event::ModifyKind::Name(mode)) => {
                                match mode {
                                    notify::event::RenameMode::From => {
                                        app_ui.rename_from(event.paths);
                                        window.request_redraw();
                                    }
                                    notify::event::RenameMode::To => {
                                        app_ui.rename_to(event.paths, &mut renderer);
                                        window.request_redraw();
                                    }
                                    notify::event::RenameMode::Both => {
                                        if event.paths.len() != 2 {
                                            eprintln!(
                                                "Rename both mode has {} paths",
                                                event.paths.len()
                                            );
                                            return;
                                        }

                                        app_ui.rename_one(
                                            &event.paths[0],
                                            &event.paths[1],
                                            &mut renderer,
                                        );
                                        window.request_redraw();
                                    }
                                    mode => {
                                        eprintln!("Unhandled rename mode: {:?}", mode);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => (),
                }
            })
            .unwrap();
    };
    Ok(())
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{message_severity:?}:\n{message_type:?} [{message_id_name} ({message_id_number})] : {message}\n",
    );

    vk::FALSE
}
