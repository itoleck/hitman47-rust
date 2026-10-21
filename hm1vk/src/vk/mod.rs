//! Vulkan backend: device, frames, upload ring, images, pipelines, presentation.
//!
//! Design notes:
//! * Vulkan 1.3 core features are used: dynamic rendering, synchronization2 and
//!   extended dynamic state. VK_KHR_push_descriptor pushes per-draw bindings.
//! * Every image lives in `VK_IMAGE_LAYOUT_GENERAL`. Hazards between passes,
//!   transfers and sampling are covered with global memory barriers. This
//!   trades a little GPU efficiency (irrelevant for a 2000-era game) for much
//!   simpler, robust state tracking.
//! * Two frames in flight. Each frame slot owns a host-visible ring buffer that
//!   holds vertices, indices, uniforms and texture staging data.

pub mod pipeline;
pub mod swapchain;

use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::mem::ManuallyDrop;

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc};
use gpu_allocator::MemoryLocation;

use crate::{debug, info};
pub use pipeline::{BlendKey, PipeKey, RasterState, SamplerKey, StencilState};

pub const FRAMES: usize = 2;
const RING_SIZE: u64 = 96 * 1024 * 1024;

/// Format of every colour render target and colour texture.
pub const COLOR_FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    /// Sampled texture (may also be rendered to / blitted).
    Color,
    /// BC-compressed sampled texture.
    Compressed,
    /// Depth/stencil buffer.
    Depth,
}

pub struct GpuImage {
    pub image: vk::Image,
    pub view: vk::ImageView,
    alloc: Option<Allocation>,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub kind: ImageKind,
}

enum Garbage {
    Image(vk::Image, vk::ImageView, Option<Allocation>),
    Buffer(vk::Buffer, Option<Allocation>),
}

struct Ring {
    buffer: vk::Buffer,
    alloc: Option<Allocation>,
    ptr: *mut u8,
    size: u64,
    offset: u64,
}

struct FrameRes {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    acquire: vk::Semaphore,
    ring: Ring,
    garbage: Vec<Garbage>,
    /// A submission that signals `fence` is outstanding.
    pending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct PassTargets {
    color: Option<ImageId>,
    depth: Option<ImageId>,
}

/// A buffer range in the current frame's ring.
#[derive(Clone, Copy, Debug)]
pub struct RingSlice {
    pub buffer: vk::Buffer,
    pub offset: u64,
    pub size: u64,
}

pub struct Gpu {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub surface_i: ash::khr::surface::Instance,
    pub win32_i: ash::khr::win32_surface::Instance,
    pub pdev: vk::PhysicalDevice,
    pub device: ash::Device,
    pub swapchain_d: ash::khr::swapchain::Device,
    pub push_d: ash::khr::push_descriptor::Device,
    debug_utils: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    pub queue: vk::Queue,
    pub qfi: u32,
    allocator: ManuallyDrop<Allocator>,
    pub props: vk::PhysicalDeviceProperties,
    pub device_name: String,
    pub vram_bytes: u64,
    pub bc_supported: bool,
    pub depth_format: vk::Format,
    pub max_anisotropy: f32,
    pub ubo_align: u64,

    pub pipes: pipeline::Pipelines,

    images: Vec<Option<GpuImage>>,
    free_ids: Vec<u32>,

    frames: Vec<FrameRes>,
    cur: usize,
    recording: bool,
    /// Increments whenever ring contents become invalid (frame advance or flush).
    pub epoch: u64,
    pass: Option<PassTargets>,
    pass_extent: (u32, u32),

    pub swap: Option<swapchain::Swapchain>,
    pub white: ImageId,
    pub gamma_lut: ImageId,
    readback: Option<(vk::Buffer, Allocation, u64)>,
    pub frames_presented: u64,
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut std::ffi::c_void,
) -> vk::Bool32 {
    if !data.is_null() && !(*data).p_message.is_null() {
        let msg = CStr::from_ptr((*data).p_message).to_string_lossy();
        info!("[vulkan {:?}] {}", severity, msg);
    }
    vk::FALSE
}

impl Gpu {
    pub fn new(validation: bool) -> Result<Gpu, String> {
        unsafe { Self::new_inner(validation) }
    }

    unsafe fn new_inner(validation: bool) -> Result<Gpu, String> {
        let entry = ash::Entry::load().map_err(|e| format!("vulkan-1.dll: {e}"))?;
        let app_name = c"Hitman: Codename 47 (HitMan-Rust)";
        let engine_name = c"hm1vk";
        let app = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(1)
            .engine_name(engine_name)
            .engine_version(1)
            .api_version(vk::API_VERSION_1_3);
        let mut exts: Vec<*const c_char> = vec![ash::khr::surface::NAME.as_ptr(), ash::khr::win32_surface::NAME.as_ptr()];
        let mut layers: Vec<*const c_char> = Vec::new();
        let validation_layer = c"VK_LAYER_KHRONOS_validation";
        let mut use_validation = false;
        if validation {
            let avail = entry.enumerate_instance_layer_properties().unwrap_or_default();
            if avail.iter().any(|l| CStr::from_ptr(l.layer_name.as_ptr()) == validation_layer) {
                layers.push(validation_layer.as_ptr());
                exts.push(ash::ext::debug_utils::NAME.as_ptr());
                use_validation = true;
            } else {
                info!("validation requested but VK_LAYER_KHRONOS_validation is not installed");
            }
        }
        let ici = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&exts)
            .enabled_layer_names(&layers);
        let instance = entry.create_instance(&ici, None).map_err(|e| format!("vkCreateInstance: {e}"))?;
        let debug_utils = if use_validation {
            let du = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let ci = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback));
            du.create_debug_utils_messenger(&ci, None).ok().map(|m| (du, m))
        } else {
            None
        };
        let surface_i = ash::khr::surface::Instance::new(&entry, &instance);
        let win32_i = ash::khr::win32_surface::Instance::new(&entry, &instance);

        // Pick a device: prefer discrete GPUs with Vulkan 1.3 + push descriptors.
        let pdevs = instance.enumerate_physical_devices().map_err(|e| format!("enumerate devices: {e}"))?;
        let mut best: Option<(i32, vk::PhysicalDevice, u32)> = None;
        for pd in pdevs {
            let p = instance.get_physical_device_properties(pd);
            if p.api_version < vk::API_VERSION_1_3 {
                continue;
            }
            let exts = instance.enumerate_device_extension_properties(pd).unwrap_or_default();
            let has = |n: &CStr| exts.iter().any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == n);
            if !has(ash::khr::swapchain::NAME) || !has(ash::khr::push_descriptor::NAME) {
                continue;
            }
            let qf = instance.get_physical_device_queue_family_properties(pd);
            let Some(qi) = qf.iter().position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS)) else {
                continue;
            };
            let score = match p.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 3,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 2,
                _ => 1,
            };
            if best.map(|b| score > b.0).unwrap_or(true) {
                best = Some((score, pd, qi as u32));
            }
        }
        let (_, pdev, qfi) = best.ok_or("no Vulkan 1.3 device with VK_KHR_push_descriptor found")?;
        let props = instance.get_physical_device_properties(pdev);
        let device_name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy().into_owned();
        let feats = instance.get_physical_device_features(pdev);
        let memprops = instance.get_physical_device_memory_properties(pdev);
        let vram_bytes: u64 = memprops.memory_heaps[..memprops.memory_heap_count as usize]
            .iter()
            .filter(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
            .map(|h| h.size)
            .max()
            .unwrap_or(256 << 20);
        info!(
            "Vulkan device: {} (api {}.{}.{}, driver {:#x}), VRAM {} MB",
            device_name,
            vk::api_version_major(props.api_version),
            vk::api_version_minor(props.api_version),
            vk::api_version_patch(props.api_version),
            props.driver_version,
            vram_bytes >> 20
        );

        let enabled = vk::PhysicalDeviceFeatures::default()
            .sampler_anisotropy(feats.sampler_anisotropy == vk::TRUE)
            .fill_mode_non_solid(feats.fill_mode_non_solid == vk::TRUE)
            .texture_compression_bc(feats.texture_compression_bc == vk::TRUE)
            .wide_lines(false)
            .depth_bias_clamp(feats.depth_bias_clamp == vk::TRUE);
        let mut f13 = vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true).synchronization2(true);
        let prio = [1.0f32];
        let qci = [vk::DeviceQueueCreateInfo::default().queue_family_index(qfi).queue_priorities(&prio)];
        let dexts = [ash::khr::swapchain::NAME.as_ptr(), ash::khr::push_descriptor::NAME.as_ptr()];
        let dci = vk::DeviceCreateInfo::default()
            .queue_create_infos(&qci)
            .enabled_extension_names(&dexts)
            .enabled_features(&enabled)
            .push_next(&mut f13);
        let device = instance.create_device(pdev, &dci, None).map_err(|e| format!("vkCreateDevice: {e}"))?;
        let queue = device.get_device_queue(qfi, 0);
        let swapchain_d = ash::khr::swapchain::Device::new(&instance, &device);
        let push_d = ash::khr::push_descriptor::Device::new(&instance, &device);

        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: pdev,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| format!("allocator: {e}"))?;

        let depth_format = [vk::Format::D24_UNORM_S8_UINT, vk::Format::D32_SFLOAT_S8_UINT]
            .into_iter()
            .find(|&f| {
                instance
                    .get_physical_device_format_properties(pdev, f)
                    .optimal_tiling_features
                    .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
            })
            .unwrap_or(vk::Format::D32_SFLOAT_S8_UINT);

        let pipes = pipeline::Pipelines::new(&device, depth_format)?;

        let mut gpu = Gpu {
            entry,
            instance,
            surface_i,
            win32_i,
            pdev,
            device,
            swapchain_d,
            push_d,
            debug_utils,
            queue,
            qfi,
            allocator: ManuallyDrop::new(allocator),
            props,
            device_name,
            vram_bytes,
            bc_supported: feats.texture_compression_bc == vk::TRUE,
            depth_format,
            max_anisotropy: if feats.sampler_anisotropy == vk::TRUE { props.limits.max_sampler_anisotropy } else { 1.0 },
            ubo_align: props.limits.min_uniform_buffer_offset_alignment.max(16),
            pipes,
            images: Vec::new(),
            free_ids: Vec::new(),
            frames: Vec::new(),
            cur: 0,
            recording: false,
            epoch: 1,
            pass: None,
            pass_extent: (0, 0),
            swap: None,
            white: ImageId(0),
            gamma_lut: ImageId(0),
            readback: None,
            frames_presented: 0,
        };
        for _ in 0..FRAMES {
            let f = gpu.create_frame()?;
            gpu.frames.push(f);
        }
        gpu.white = gpu.create_image(1, 1, 1, ImageKind::Color, COLOR_FORMAT)?;
        gpu.upload_level(gpu.white, 0, 1, 1, &[255, 255, 255, 255]);
        gpu.gamma_lut = gpu.create_image(256, 1, 1, ImageKind::Color, COLOR_FORMAT)?;
        let mut lut = vec![0u8; 256 * 4];
        for i in 0..256 {
            lut[i * 4..i * 4 + 4].copy_from_slice(&[i as u8, i as u8, i as u8, 255]);
        }
        gpu.upload_level(gpu.gamma_lut, 0, 256, 1, &lut);
        info!("depth format {:?}, BC textures {}, max anisotropy {}", gpu.depth_format, gpu.bc_supported, gpu.max_anisotropy);
        Ok(gpu)
    }

    unsafe fn create_frame(&mut self) -> Result<FrameRes, String> {
        let d = &self.device;
        let pool = d
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(self.qfi)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .map_err(|e| e.to_string())?;
        let cmd = d
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .map_err(|e| e.to_string())?[0];
        let fence = d.create_fence(&vk::FenceCreateInfo::default(), None).map_err(|e| e.to_string())?;
        let acquire = d.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).map_err(|e| e.to_string())?;
        let (buffer, alloc) = self.create_buffer(
            RING_SIZE,
            vk::BufferUsageFlags::VERTEX_BUFFER
                | vk::BufferUsageFlags::INDEX_BUFFER
                | vk::BufferUsageFlags::UNIFORM_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "ring",
        )?;
        let ptr = alloc.mapped_ptr().ok_or("ring not mappable")?.as_ptr() as *mut u8;
        Ok(FrameRes {
            pool,
            cmd,
            fence,
            acquire,
            ring: Ring { buffer, alloc: Some(alloc), ptr, size: RING_SIZE, offset: 0 },
            garbage: Vec::new(),
            pending: false,
        })
    }

    pub unsafe fn create_buffer(
        &mut self,
        size: u64,
        usage: vk::BufferUsageFlags,
        loc: MemoryLocation,
        name: &str,
    ) -> Result<(vk::Buffer, Allocation), String> {
        let buffer = self
            .device
            .create_buffer(&vk::BufferCreateInfo::default().size(size).usage(usage), None)
            .map_err(|e| e.to_string())?;
        let req = self.device.get_buffer_memory_requirements(buffer);
        let alloc = self
            .allocator
            .allocate(&AllocationCreateDesc {
                name,
                requirements: req,
                location: loc,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| e.to_string())?;
        self.device.bind_buffer_memory(buffer, alloc.memory(), alloc.offset()).map_err(|e| e.to_string())?;
        Ok((buffer, alloc))
    }

    // ------------------------------------------------------------ images

    pub fn create_image(
        &mut self,
        width: u32,
        height: u32,
        levels: u32,
        kind: ImageKind,
        format: vk::Format,
    ) -> Result<ImageId, String> {
        unsafe {
            let (usage, aspect) = match kind {
                ImageKind::Color => (
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::COLOR_ATTACHMENT,
                    vk::ImageAspectFlags::COLOR,
                ),
                ImageKind::Compressed => (
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
                    vk::ImageAspectFlags::COLOR,
                ),
                ImageKind::Depth => (
                    vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                    vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
                ),
            };
            let w = width.max(1);
            let h = height.max(1);
            let ici = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D { width: w, height: h, depth: 1 })
                .mip_levels(levels.max(1))
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(usage)
                .initial_layout(vk::ImageLayout::UNDEFINED);
            let image = self.device.create_image(&ici, None).map_err(|e| format!("vkCreateImage {w}x{h}: {e}"))?;
            let req = self.device.get_image_memory_requirements(image);
            let alloc = self
                .allocator
                .allocate(&AllocationCreateDesc {
                    name: "image",
                    requirements: req,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| format!("image memory: {e}"))?;
            self.device.bind_image_memory(image, alloc.memory(), alloc.offset()).map_err(|e| e.to_string())?;
            let view_aspect = if kind == ImageKind::Depth { vk::ImageAspectFlags::DEPTH } else { aspect };
            let view = self
                .device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(view_aspect)
                                .level_count(levels.max(1))
                                .layer_count(1),
                        ),
                    None,
                )
                .map_err(|e| e.to_string())?;
            // Transition to GENERAL once.
            self.end_pass();
            let cmd = self.cmd();
            let barrier = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::GENERAL)
                .image(image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(aspect)
                        .level_count(vk::REMAINING_MIP_LEVELS)
                        .layer_count(1),
                );
            self.device.cmd_pipeline_barrier2(
                cmd,
                &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier)),
            );
            let img = GpuImage { image, view, alloc: Some(alloc), format, width: w, height: h, levels: levels.max(1), kind };
            let id = if let Some(i) = self.free_ids.pop() {
                self.images[i as usize] = Some(img);
                i
            } else {
                self.images.push(Some(img));
                (self.images.len() - 1) as u32
            };
            Ok(ImageId(id))
        }
    }

    pub fn image(&self, id: ImageId) -> Option<&GpuImage> {
        self.images.get(id.0 as usize).and_then(|i| i.as_ref())
    }

    pub fn destroy_image(&mut self, id: ImageId) {
        if let Some(slot) = self.images.get_mut(id.0 as usize) {
            if let Some(mut img) = slot.take() {
                if self.pass.map(|p| p.color == Some(id) || p.depth == Some(id)).unwrap_or(false) {
                    self.end_pass();
                }
                let g = Garbage::Image(img.image, img.view, img.alloc.take());
                self.frames[self.cur].garbage.push(g);
                self.free_ids.push(id.0);
            }
        }
    }

    // ------------------------------------------------------------ frames / commands

    /// Current command buffer, beginning it (and waiting for the frame slot)
    /// if necessary.
    pub fn cmd(&mut self) -> vk::CommandBuffer {
        if !self.recording {
            unsafe { self.begin_frame_slot() };
        }
        self.frames[self.cur].cmd
    }

    unsafe fn begin_frame_slot(&mut self) {
        let fr = &mut self.frames[self.cur];
        if fr.pending {
            loop {
                match self.device.wait_for_fences(&[fr.fence], true, 5_000_000_000) {
                    Err(vk::Result::TIMEOUT) => info!("GPU fence wait exceeded 5 s (still waiting)"),
                    Err(e) => {
                        info!("vkWaitForFences failed: {e}");
                        break;
                    }
                    Ok(()) => break,
                }
            }
            let _ = self.device.reset_fences(&[fr.fence]);
            fr.pending = false;
        }
        let garbage = std::mem::take(&mut fr.garbage);
        fr.ring.offset = 0;
        let cmd = fr.cmd;
        for g in garbage {
            self.free_garbage(g);
        }
        let _ = self.device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty());
        let _ = self.device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        );
        self.recording = true;
        self.epoch += 1;
    }

    unsafe fn free_garbage(&mut self, g: Garbage) {
        match g {
            Garbage::Image(i, v, a) => {
                self.device.destroy_image_view(v, None);
                self.device.destroy_image(i, None);
                if let Some(a) = a {
                    let _ = self.allocator.free(a);
                }
            }
            Garbage::Buffer(b, a) => {
                self.device.destroy_buffer(b, None);
                if let Some(a) = a {
                    let _ = self.allocator.free(a);
                }
            }
        }
    }

    /// Submits everything recorded so far and waits for the GPU. The ring is
    /// reset, so any `RingSlice` obtained earlier becomes invalid.
    pub fn flush_wait(&mut self) {
        if !self.recording {
            return;
        }
        unsafe {
            self.end_pass();
            let fr = &mut self.frames[self.cur];
            let _ = self.device.end_command_buffer(fr.cmd);
            let cbi = [vk::CommandBufferSubmitInfo::default().command_buffer(fr.cmd)];
            let submit = vk::SubmitInfo2::default().command_buffer_infos(&cbi);
            let _ = self.device.queue_submit2(self.queue, &[submit], fr.fence);
            fr.pending = true;
            self.recording = false;
            self.begin_frame_slot();
        }
    }

    // ------------------------------------------------------------ ring

    /// Makes sure `bytes` can be allocated from the ring without an implicit
    /// flush in between (a flush invalidates earlier slices).
    pub fn ring_reserve(&mut self, bytes: u64) {
        let _ = self.cmd();
        let r = &self.frames[self.cur].ring;
        if bytes <= RING_SIZE / 2 && r.offset + bytes + 256 > r.size {
            self.flush_wait();
        }
    }

    /// Allocates `size` bytes in the current frame's ring and copies `data` in.
    pub fn ring_upload(&mut self, data: &[u8], align: u64) -> RingSlice {
        let (slice, ptr) = self.ring_alloc(data.len() as u64, align);
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len()) };
        slice
    }

    /// Allocates ring space, returning the slice and a CPU pointer to fill.
    pub fn ring_alloc(&mut self, size: u64, align: u64) -> (RingSlice, *mut u8) {
        let _ = self.cmd();
        let size = size.max(4);
        if size > RING_SIZE / 2 {
            // Huge one-off upload: dedicated buffer, freed with this frame.
            unsafe {
                let (buf, alloc) = self
                    .create_buffer(
                        size,
                        vk::BufferUsageFlags::VERTEX_BUFFER
                            | vk::BufferUsageFlags::INDEX_BUFFER
                            | vk::BufferUsageFlags::TRANSFER_SRC,
                        MemoryLocation::CpuToGpu,
                        "big-upload",
                    )
                    .expect("big upload buffer");
                let ptr = alloc.mapped_ptr().unwrap().as_ptr() as *mut u8;
                self.frames[self.cur].garbage.push(Garbage::Buffer(buf, Some(alloc)));
                return (RingSlice { buffer: buf, offset: 0, size }, ptr);
            }
        }
        let align = align.max(4);
        {
            let r = &self.frames[self.cur].ring;
            let off = (r.offset + align - 1) & !(align - 1);
            if off + size > r.size {
                debug!("ring full ({} MB) - flushing", r.size >> 20);
                self.flush_wait();
            }
        }
        let r = &mut self.frames[self.cur].ring;
        let off = (r.offset + align - 1) & !(align - 1);
        r.offset = off + size;
        (RingSlice { buffer: r.buffer, offset: off, size }, unsafe { r.ptr.add(off as usize) })
    }

    // ------------------------------------------------------------ passes

    fn barrier_all(&mut self) {
        let cmd = self.cmd();
        let mb = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE);
        unsafe {
            self.device
                .cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(std::slice::from_ref(&mb)))
        };
    }

    pub fn end_pass(&mut self) {
        if self.pass.take().is_some() {
            let cmd = self.frames[self.cur].cmd;
            unsafe { self.device.cmd_end_rendering(cmd) };
            self.barrier_all();
        }
    }

    pub fn in_pass(&self) -> bool {
        self.pass.is_some()
    }

    /// Begins (or continues) rendering into `color` / `depth`.
    pub fn begin_pass(&mut self, color: Option<ImageId>, depth: Option<ImageId>) -> bool {
        let want = PassTargets { color, depth };
        if self.pass == Some(want) {
            return true;
        }
        self.end_pass();
        let cmd = self.cmd();
        let (cview, cext) = match color.and_then(|c| self.image(c)) {
            Some(i) => (Some(i.view), (i.width, i.height)),
            None => (None, (u32::MAX, u32::MAX)),
        };
        let (dview, dext) = match depth.and_then(|d| self.image(d)) {
            Some(i) => (Some(i.view), (i.width, i.height)),
            None => (None, (u32::MAX, u32::MAX)),
        };
        if cview.is_none() && dview.is_none() {
            return false;
        }
        let w = cext.0.min(dext.0);
        let h = cext.1.min(dext.1);
        let catt = cview.map(|v| {
            vk::RenderingAttachmentInfo::default()
                .image_view(v)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE)
        });
        let datt = dview.map(|v| {
            vk::RenderingAttachmentInfo::default()
                .image_view(v)
                .image_layout(vk::ImageLayout::GENERAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE)
        });
        let colors: Vec<vk::RenderingAttachmentInfo> = catt.into_iter().collect();
        let mut ri = vk::RenderingInfo::default()
            .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: w, height: h } })
            .layer_count(1)
            .color_attachments(&colors);
        if let Some(d) = datt.as_ref() {
            ri = ri.depth_attachment(d).stencil_attachment(d);
        }
        unsafe { self.device.cmd_begin_rendering(cmd, &ri) };
        self.pass = Some(want);
        self.pass_extent = (w, h);
        true
    }

    pub fn pass_extent(&self) -> (u32, u32) {
        self.pass_extent
    }

    /// Clears rectangles of the current pass's attachments.
    pub fn clear_rects(&mut self, rects: &[vk::Rect2D], color: Option<[f32; 4]>, depth: Option<f32>, stencil: Option<u32>) {
        let Some(p) = self.pass else { return };
        let mut atts = Vec::new();
        if let (Some(c), true) = (color, p.color.is_some()) {
            atts.push(vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                color_attachment: 0,
                clear_value: vk::ClearValue { color: vk::ClearColorValue { float32: c } },
            });
        }
        if p.depth.is_some() && (depth.is_some() || stencil.is_some()) {
            let mut aspect = vk::ImageAspectFlags::empty();
            if depth.is_some() {
                aspect |= vk::ImageAspectFlags::DEPTH;
            }
            if stencil.is_some() {
                aspect |= vk::ImageAspectFlags::STENCIL;
            }
            atts.push(vk::ClearAttachment {
                aspect_mask: aspect,
                color_attachment: 0,
                clear_value: vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: depth.unwrap_or(1.0).clamp(0.0, 1.0),
                        stencil: stencil.unwrap_or(0),
                    },
                },
            });
        }
        if atts.is_empty() {
            return;
        }
        let (w, h) = self.pass_extent;
        let crs: Vec<vk::ClearRect> = rects
            .iter()
            .filter_map(|r| {
                let x0 = r.offset.x.clamp(0, w as i32);
                let y0 = r.offset.y.clamp(0, h as i32);
                let x1 = (r.offset.x + r.extent.width as i32).clamp(0, w as i32);
                let y1 = (r.offset.y + r.extent.height as i32).clamp(0, h as i32);
                if x1 <= x0 || y1 <= y0 {
                    return None;
                }
                Some(vk::ClearRect {
                    rect: vk::Rect2D {
                        offset: vk::Offset2D { x: x0, y: y0 },
                        extent: vk::Extent2D { width: (x1 - x0) as u32, height: (y1 - y0) as u32 },
                    },
                    base_array_layer: 0,
                    layer_count: 1,
                })
            })
            .collect();
        if crs.is_empty() {
            return;
        }
        let cmd = self.cmd();
        unsafe { self.device.cmd_clear_attachments(cmd, &atts, &crs) };
    }

    // ------------------------------------------------------------ transfers

    /// Uploads one mip level. `data` is tightly packed (BGRA8 rows, or BC blocks).
    pub fn upload_level(&mut self, id: ImageId, level: u32, w: u32, h: u32, data: &[u8]) {
        let Some(img) = self.image(id) else { return };
        let (image, kind) = (img.image, img.kind);
        let slice = self.ring_upload(data, 16);
        self.end_pass();
        let cmd = self.cmd();
        let region = vk::BufferImageCopy::default()
            .buffer_offset(slice.offset)
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(if kind == ImageKind::Depth { vk::ImageAspectFlags::DEPTH } else { vk::ImageAspectFlags::COLOR })
                    .mip_level(level)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D { width: w.max(1), height: h.max(1), depth: 1 });
        unsafe {
            self.device.cmd_copy_buffer_to_image(cmd, slice.buffer, image, vk::ImageLayout::GENERAL, &[region]);
        }
        self.barrier_all();
    }

    /// Copies a sub-rectangle of BGRA8 pixels into level 0 of a colour image.
    pub fn upload_rect(&mut self, id: ImageId, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        let Some(img) = self.image(id) else { return };
        let image = img.image;
        let slice = self.ring_upload(data, 16);
        self.end_pass();
        let cmd = self.cmd();
        let region = vk::BufferImageCopy::default()
            .buffer_offset(slice.offset)
            .image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1))
            .image_offset(vk::Offset3D { x: x as i32, y: y as i32, z: 0 })
            .image_extent(vk::Extent3D { width: w, height: h, depth: 1 });
        unsafe { self.device.cmd_copy_buffer_to_image(cmd, slice.buffer, image, vk::ImageLayout::GENERAL, &[region]) };
        self.barrier_all();
    }

    /// GPU blit between two colour images (scaling / mirroring allowed).
    pub fn blit(&mut self, src: ImageId, src_rect: [i32; 4], dst: ImageId, dst_rect: [i32; 4], linear: bool) {
        let (Some(s), Some(d)) = (self.image(src), self.image(dst)) else { return };
        let (si, di) = (s.image, d.image);
        self.end_pass();
        let cmd = self.cmd();
        let sub = vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1);
        let region = vk::ImageBlit::default()
            .src_subresource(sub)
            .src_offsets([
                vk::Offset3D { x: src_rect[0], y: src_rect[1], z: 0 },
                vk::Offset3D { x: src_rect[2], y: src_rect[3], z: 1 },
            ])
            .dst_subresource(sub)
            .dst_offsets([
                vk::Offset3D { x: dst_rect[0], y: dst_rect[1], z: 0 },
                vk::Offset3D { x: dst_rect[2], y: dst_rect[3], z: 1 },
            ]);
        unsafe {
            self.device.cmd_blit_image(
                cmd,
                si,
                vk::ImageLayout::GENERAL,
                di,
                vk::ImageLayout::GENERAL,
                &[region],
                if linear { vk::Filter::LINEAR } else { vk::Filter::NEAREST },
            );
        }
        self.barrier_all();
    }

    /// Fills a rectangle of a colour image (outside of normal draws).
    pub fn fill_color(&mut self, id: ImageId, rect: vk::Rect2D, color: [f32; 4]) {
        let prev = self.pass;
        self.begin_pass(Some(id), None);
        self.clear_rects(&[rect], Some(color), None, None);
        if let Some(p) = prev {
            self.begin_pass(p.color, p.depth);
        }
    }

    /// Fills a rectangle of a depth image.
    pub fn fill_depth(&mut self, id: ImageId, rect: vk::Rect2D, depth: f32, stencil: Option<u32>) {
        let prev = self.pass;
        self.begin_pass(None, Some(id));
        self.clear_rects(&[rect], None, Some(depth), stencil);
        if let Some(p) = prev {
            self.begin_pass(p.color, p.depth);
        }
    }

    /// Reads level 0 of a colour image back to the CPU as packed BGRA8.
    pub fn read_image(&mut self, id: ImageId) -> Option<Vec<u8>> {
        let img = self.image(id)?;
        if img.kind != ImageKind::Color {
            return None;
        }
        let (image, w, h) = (img.image, img.width, img.height);
        let size = (w as u64) * (h as u64) * 4;
        unsafe {
            if self.readback.as_ref().map(|r| r.2 < size).unwrap_or(true) {
                if let Some((b, a, _)) = self.readback.take() {
                    self.device.device_wait_idle().ok();
                    self.device.destroy_buffer(b, None);
                    let _ = self.allocator.free(a);
                }
                let (b, a) = self
                    .create_buffer(size, vk::BufferUsageFlags::TRANSFER_DST, MemoryLocation::GpuToCpu, "readback")
                    .ok()?;
                self.readback = Some((b, a, size));
            }
            let buf = self.readback.as_ref().unwrap().0;
            self.end_pass();
            let cmd = self.cmd();
            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1))
                .image_extent(vk::Extent3D { width: w, height: h, depth: 1 });
            self.device.cmd_copy_image_to_buffer(cmd, image, vk::ImageLayout::GENERAL, buf, &[region]);
            self.barrier_all();
            self.flush_wait();
            let ptr = self.readback.as_ref().unwrap().1.mapped_ptr()?.as_ptr() as *const u8;
            Some(std::slice::from_raw_parts(ptr, size as usize).to_vec())
        }
    }

    pub fn wait_idle(&mut self) {
        self.flush_wait();
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }

    /// Submits the current frame and moves to the next frame slot without
    /// presenting (used when there is no swapchain, e.g. minimised).
    pub fn end_frame_no_present(&mut self) {
        if !self.recording {
            return;
        }
        unsafe {
            self.end_pass();
            let fr = &mut self.frames[self.cur];
            let _ = self.device.end_command_buffer(fr.cmd);
            let cbi = [vk::CommandBufferSubmitInfo::default().command_buffer(fr.cmd)];
            let submit = vk::SubmitInfo2::default().command_buffer_infos(&cbi);
            let _ = self.device.queue_submit2(self.queue, &[submit], fr.fence);
            fr.pending = true;
        }
        self.recording = false;
        self.cur = (self.cur + 1) % FRAMES;
    }

    /// Replaces the current frame's acquire semaphore (after a timed-out acquire).
    pub(crate) fn replace_acquire_semaphore(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            let old = self.frames[self.cur].acquire;
            self.device.destroy_semaphore(old, None);
            self.frames[self.cur].acquire =
                self.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).unwrap_or_default();
        }
    }

    pub(crate) fn frame_acquire_semaphore(&self) -> vk::Semaphore {
        self.frames[self.cur].acquire
    }

    pub(crate) fn submit_frame_for_present(&mut self, wait: vk::Semaphore, signal: vk::Semaphore) {
        unsafe {
            let fr = &mut self.frames[self.cur];
            let _ = self.device.end_command_buffer(fr.cmd);
            let cbi = [vk::CommandBufferSubmitInfo::default().command_buffer(fr.cmd)];
            let waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(wait)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let signals = [vk::SemaphoreSubmitInfo::default()
                .semaphore(signal)
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
            let submit = vk::SubmitInfo2::default()
                .command_buffer_infos(&cbi)
                .wait_semaphore_infos(&waits)
                .signal_semaphore_infos(&signals);
            let _ = self.device.queue_submit2(self.queue, &[submit], fr.fence);
            fr.pending = true;
        }
        self.recording = false;
        self.cur = (self.cur + 1) % FRAMES;
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            if let Some(sc) = self.swap.take() {
                sc.destroy(self);
            }
            for i in 0..self.images.len() {
                if let Some(mut img) = self.images[i].take() {
                    self.device.destroy_image_view(img.view, None);
                    self.device.destroy_image(img.image, None);
                    if let Some(a) = img.alloc.take() {
                        let _ = self.allocator.free(a);
                    }
                }
            }
            let frames = std::mem::take(&mut self.frames);
            for mut f in frames {
                for g in std::mem::take(&mut f.garbage) {
                    self.free_garbage(g);
                }
                self.device.destroy_buffer(f.ring.buffer, None);
                if let Some(a) = f.ring.alloc.take() {
                    let _ = self.allocator.free(a);
                }
                self.device.destroy_fence(f.fence, None);
                self.device.destroy_semaphore(f.acquire, None);
                self.device.destroy_command_pool(f.pool, None);
            }
            if let Some((b, a, _)) = self.readback.take() {
                self.device.destroy_buffer(b, None);
                let _ = self.allocator.free(a);
            }
            self.pipes.destroy(&self.device);
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            if let Some((du, m)) = self.debug_utils.take() {
                du.destroy_debug_utils_messenger(m, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// Helper used by pipeline/draw code: a draw's complete description.
pub struct Draw<'a> {
    pub raster: RasterState,
    pub vs_ubo: RingSlice,
    pub fs_ubo: RingSlice,
    pub textures: [(ImageId, SamplerKey); 4],
    pub vertices: RingSlice,
    pub vertex_count: u32,
    pub indices: Option<RingSlice>,
    pub index_count: u32,
    pub _marker: std::marker::PhantomData<&'a ()>,
}

impl Gpu {
    /// Records a fixed-function draw into the current pass.
    pub fn draw(&mut self, d: &Draw<'_>) {
        if self.pass.is_none() {
            return;
        }
        let cmd = self.cmd();
        let has_depth = self.pass.map(|p| p.depth.is_some()).unwrap_or(false);
        let has_color = self.pass.map(|p| p.color.is_some()).unwrap_or(false);
        let key = PipeKey::from_raster(&d.raster, has_color, has_depth);
        let pipe = match self.pipes.ff_pipeline(&self.device, &key) {
            Some(p) => p,
            None => return,
        };
        let mut views = [vk::ImageView::null(); 4];
        let mut samplers = [vk::Sampler::null(); 4];
        let white_view = self.image(self.white).map(|i| i.view).unwrap_or_default();
        for (i, (img, sk)) in d.textures.iter().enumerate() {
            views[i] = self.image(*img).map(|i| i.view).unwrap_or(white_view);
            let aniso = self.max_anisotropy;
            samplers[i] = self.pipes.sampler(&self.device, sk, aniso);
        }
        unsafe {
            let dev = &self.device;
            dev.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe);
            let r = &d.raster;
            dev.cmd_set_viewport(cmd, 0, &[r.viewport]);
            dev.cmd_set_scissor(cmd, 0, &[r.scissor]);
            dev.cmd_set_cull_mode(cmd, r.cull);
            dev.cmd_set_front_face(cmd, vk::FrontFace::CLOCKWISE);
            dev.cmd_set_primitive_topology(cmd, r.topology);
            let dt = has_depth && r.depth_test;
            dev.cmd_set_depth_test_enable(cmd, dt);
            dev.cmd_set_depth_write_enable(cmd, has_depth && r.depth_write);
            dev.cmd_set_depth_compare_op(cmd, r.depth_cmp);
            dev.cmd_set_depth_bias_enable(cmd, r.depth_bias != 0.0);
            dev.cmd_set_depth_bias(cmd, r.depth_bias, 0.0, r.depth_bias * 0.5);
            let st = r.stencil.filter(|_| has_depth);
            dev.cmd_set_stencil_test_enable(cmd, st.is_some());
            if let Some(s) = st {
                dev.cmd_set_stencil_op(cmd, vk::StencilFaceFlags::FRONT_AND_BACK, s.fail, s.pass, s.zfail, s.cmp);
                dev.cmd_set_stencil_compare_mask(cmd, vk::StencilFaceFlags::FRONT_AND_BACK, s.read_mask);
                dev.cmd_set_stencil_write_mask(cmd, vk::StencilFaceFlags::FRONT_AND_BACK, s.write_mask);
                dev.cmd_set_stencil_reference(cmd, vk::StencilFaceFlags::FRONT_AND_BACK, s.reference);
            }
            let vsb = [vk::DescriptorBufferInfo { buffer: d.vs_ubo.buffer, offset: d.vs_ubo.offset, range: d.vs_ubo.size }];
            let fsb = [vk::DescriptorBufferInfo { buffer: d.fs_ubo.buffer, offset: d.fs_ubo.offset, range: d.fs_ubo.size }];
            let imgs: Vec<[vk::DescriptorImageInfo; 1]> = views
                .iter()
                .map(|v| [vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: *v, image_layout: vk::ImageLayout::GENERAL }])
                .collect();
            let smps: Vec<[vk::DescriptorImageInfo; 1]> = samplers
                .iter()
                .map(|s| [vk::DescriptorImageInfo { sampler: *s, image_view: vk::ImageView::null(), image_layout: vk::ImageLayout::UNDEFINED }])
                .collect();
            let mut writes = vec![
                vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&vsb),
                vk::WriteDescriptorSet::default().dst_binding(1).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&fsb),
            ];
            for i in 0..4 {
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_binding(2 + i as u32)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .image_info(&imgs[i]),
                );
                writes.push(
                    vk::WriteDescriptorSet::default()
                        .dst_binding(6 + i as u32)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .image_info(&smps[i]),
                );
            }
            self.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipes.layout, 0, &writes);
            dev.cmd_bind_vertex_buffers(cmd, 0, &[d.vertices.buffer], &[d.vertices.offset]);
            if let Some(ib) = d.indices {
                dev.cmd_bind_index_buffer(cmd, ib.buffer, ib.offset, vk::IndexType::UINT16);
                dev.cmd_draw_indexed(cmd, d.index_count, 1, 0, 0, 0);
            } else {
                dev.cmd_draw(cmd, d.vertex_count, 1, 0, 0);
            }
        }
    }
}

pub fn vk_rect(x: i32, y: i32, w: u32, h: u32) -> vk::Rect2D {
    vk::Rect2D { offset: vk::Offset2D { x, y }, extent: vk::Extent2D { width: w, height: h } }
}

#[allow(dead_code)]
pub type SamplerCache = HashMap<SamplerKey, vk::Sampler>;
