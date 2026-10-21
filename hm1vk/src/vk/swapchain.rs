//! Window surface, swapchain and the presentation pass.

use ash::vk;

use super::{Gpu, ImageId};
use crate::{debug, info};

pub struct Swapchain {
    pub hwnd: isize,
    pub surface: vk::SurfaceKHR,
    pub swapchain: vk::SwapchainKHR,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub images: Vec<vk::Image>,
    pub views: Vec<vk::ImageView>,
    /// One "render finished" semaphore per swapchain image.
    pub done: Vec<vk::Semaphore>,
    pub vsync: bool,
    pub needs_recreate: bool,
}

impl Swapchain {
    pub unsafe fn destroy(self, gpu: &Gpu) {
        let d = &gpu.device;
        for v in &self.views {
            d.destroy_image_view(*v, None);
        }
        for s in &self.done {
            d.destroy_semaphore(*s, None);
        }
        gpu.swapchain_d.destroy_swapchain(self.swapchain, None);
        gpu.surface_i.destroy_surface(self.surface, None);
    }
}

fn client_size(hwnd: isize) -> (u32, u32) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(hwnd as _, &mut r) };
    ((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32)
}

impl Gpu {
    /// Makes sure a swapchain exists for `hwnd` with the window's current size.
    /// Returns false if presentation is impossible right now (e.g. minimised).
    pub fn ensure_swapchain(&mut self, hwnd: isize, vsync: bool) -> bool {
        let (w, h) = client_size(hwnd);
        if w == 0 || h == 0 {
            return false;
        }
        let ok = match &self.swap {
            Some(s) => {
                s.hwnd == hwnd && !s.needs_recreate && s.extent.width == w && s.extent.height == h && s.vsync == vsync
            }
            None => false,
        };
        if ok {
            return true;
        }
        unsafe {
            let _ = self.device.device_wait_idle();
            let old = self.swap.take();
            let (surface, old_swapchain) = match old {
                Some(o) if o.hwnd == hwnd => {
                    for v in &o.views {
                        self.device.destroy_image_view(*v, None);
                    }
                    for s in &o.done {
                        self.device.destroy_semaphore(*s, None);
                    }
                    (o.surface, o.swapchain)
                }
                Some(o) => {
                    o.destroy(self);
                    (vk::SurfaceKHR::null(), vk::SwapchainKHR::null())
                }
                None => (vk::SurfaceKHR::null(), vk::SwapchainKHR::null()),
            };
            let surface = if surface == vk::SurfaceKHR::null() {
                let hinst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
                let ci = vk::Win32SurfaceCreateInfoKHR::default().hinstance(hinst as isize).hwnd(hwnd);
                match self.win32_i.create_win32_surface(&ci, None) {
                    Ok(s) => s,
                    Err(e) => {
                        info!("vkCreateWin32SurfaceKHR failed: {e}");
                        return false;
                    }
                }
            } else {
                surface
            };
            let supported = self.surface_i.get_physical_device_surface_support(self.pdev, self.qfi, surface).unwrap_or(false);
            if !supported {
                info!("queue family cannot present to this window");
                return false;
            }
            let caps = match self.surface_i.get_physical_device_surface_capabilities(self.pdev, surface) {
                Ok(c) => c,
                Err(e) => {
                    info!("surface caps: {e}");
                    return false;
                }
            };
            let formats = self.surface_i.get_physical_device_surface_formats(self.pdev, surface).unwrap_or_default();
            let fmt = formats
                .iter()
                .find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
                .or_else(|| formats.iter().find(|f| f.format == vk::Format::R8G8B8A8_UNORM))
                .or(formats.first())
                .copied()
                .unwrap_or(vk::SurfaceFormatKHR {
                    format: vk::Format::B8G8R8A8_UNORM,
                    color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
                });
            let modes = self.surface_i.get_physical_device_surface_present_modes(self.pdev, surface).unwrap_or_default();
            let mode = if vsync {
                vk::PresentModeKHR::FIFO
            } else if modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
                vk::PresentModeKHR::IMMEDIATE
            } else if modes.contains(&vk::PresentModeKHR::MAILBOX) {
                vk::PresentModeKHR::MAILBOX
            } else {
                vk::PresentModeKHR::FIFO
            };
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: w.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                    height: h.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                }
            };
            if extent.width == 0 || extent.height == 0 {
                return false;
            }
            let mut count = caps.min_image_count.max(2) + 1;
            if caps.max_image_count != 0 {
                count = count.min(caps.max_image_count);
            }
            let ci = vk::SwapchainCreateInfoKHR::default()
                .surface(surface)
                .min_image_count(count)
                .image_format(fmt.format)
                .image_color_space(fmt.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(mode)
                .clipped(true)
                .old_swapchain(old_swapchain);
            let swapchain = match self.swapchain_d.create_swapchain(&ci, None) {
                Ok(s) => s,
                Err(e) => {
                    info!("vkCreateSwapchainKHR failed: {e}");
                    if old_swapchain != vk::SwapchainKHR::null() {
                        self.swapchain_d.destroy_swapchain(old_swapchain, None);
                    }
                    self.surface_i.destroy_surface(surface, None);
                    return false;
                }
            };
            if old_swapchain != vk::SwapchainKHR::null() {
                self.swapchain_d.destroy_swapchain(old_swapchain, None);
            }
            let images = self.swapchain_d.get_swapchain_images(swapchain).unwrap_or_default();
            let views = images
                .iter()
                .map(|&i| {
                    self.device
                        .create_image_view(
                            &vk::ImageViewCreateInfo::default()
                                .image(i)
                                .view_type(vk::ImageViewType::TYPE_2D)
                                .format(fmt.format)
                                .subresource_range(
                                    vk::ImageSubresourceRange::default()
                                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                                        .level_count(1)
                                        .layer_count(1),
                                ),
                            None,
                        )
                        .unwrap()
                })
                .collect();
            let done = images
                .iter()
                .map(|_| self.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).unwrap())
                .collect();
            info!(
                "swapchain {}x{} {:?} {:?}, {} images",
                extent.width,
                extent.height,
                fmt.format,
                mode,
                images.len()
            );
            self.swap = Some(Swapchain {
                hwnd,
                surface,
                swapchain,
                format: fmt.format,
                extent,
                images,
                views,
                done,
                vsync,
                needs_recreate: false,
            });
            true
        }
    }

    /// Presents `src` (rectangle `src_rect` in pixels) to the window, scaled to
    /// fit while preserving aspect ratio (or to `dst_rect` inside the client
    /// area for windowed blits).
    pub fn present(
        &mut self,
        hwnd: isize,
        src: ImageId,
        src_rect: [u32; 4],
        dst_rect: Option<[i32; 4]>,
        gamma: bool,
        vsync: bool,
    ) {
        if !self.ensure_swapchain(hwnd, vsync) {
            self.end_frame_no_present();
            return;
        }
        let Some(img) = self.image(src) else {
            self.end_frame_no_present();
            return;
        };
        let (sw, sh, src_view) = (img.width as f32, img.height as f32, img.view);
        self.end_pass();
        let _ = self.cmd();
        let acquire = self.frame_acquire_semaphore();
        let sc = self.swap.as_ref().unwrap();
        let (swapchain, extent, format) = (sc.swapchain, sc.extent, sc.format);
        let index = unsafe {
            if self.frames_presented < 3 {
                debug!("present #{}: acquiring", self.frames_presented);
            }
            match self.swapchain_d.acquire_next_image(swapchain, 2_000_000_000, acquire, vk::Fence::null()) {
                Ok((i, suboptimal)) => {
                    if suboptimal {
                        self.swap.as_mut().unwrap().needs_recreate = true;
                    }
                    i
                }
                Err(e) => {
                    info!("vkAcquireNextImageKHR failed: {e} (swapchain will be recreated)");
                    self.swap.as_mut().unwrap().needs_recreate = true;
                    if e == vk::Result::TIMEOUT || e == vk::Result::NOT_READY {
                        // The acquire semaphore may still be signalled later:
                        // start the next frame slot with a fresh one.
                        self.replace_acquire_semaphore();
                    }
                    self.end_frame_no_present();
                    return;
                }
            }
        };
        let sc = self.swap.as_ref().unwrap();
        let (sc_image, sc_view, done) = (sc.images[index as usize], sc.views[index as usize], sc.done[index as usize]);
        let cmd = self.cmd();
        let Some(pipe) = self.pipes.present_pipeline(&self.device, format) else {
            self.end_frame_no_present();
            return;
        };
        // Destination rectangle: letterboxed fit unless given.
        let (dx, dy, dw, dh) = match dst_rect {
            Some(r) => (r[0] as f32, r[1] as f32, (r[2] - r[0]) as f32, (r[3] - r[1]) as f32),
            None => {
                let (ew, eh) = (extent.width as f32, extent.height as f32);
                let (rw, rh) = ((src_rect[2] - src_rect[0]) as f32, (src_rect[3] - src_rect[1]) as f32);
                let scale = (ew / rw).min(eh / rh);
                let (w, h) = ((rw * scale).round(), (rh * scale).round());
                (((ew - w) / 2.0).floor(), ((eh - h) / 2.0).floor(), w, h)
            }
        };
        let uv = [
            src_rect[0] as f32 / sw,
            src_rect[1] as f32 / sh,
            src_rect[2] as f32 / sw,
            src_rect[3] as f32 / sh,
        ];
        let mut ubo = [0u8; 32];
        for (i, v) in uv.iter().enumerate() {
            ubo[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        ubo[16..20].copy_from_slice(&(gamma as u32).to_le_bytes());
        let align = self.ubo_align;
        let ub = self.ring_upload(&ubo, align);
        let lut_view = self.image(self.gamma_lut).map(|i| i.view).unwrap_or_default();
        let linear = crate::vk::SamplerKey { mag_linear: true, min_linear: true, address_u: 3, address_v: 3, ..Default::default() };
        let nearest = crate::vk::SamplerKey { address_u: 3, address_v: 3, ..Default::default() };
        let exact = (dw - (src_rect[2] - src_rect[0]) as f32).abs() < 0.5 && (dh - (src_rect[3] - src_rect[1]) as f32).abs() < 0.5;
        let aniso = self.max_anisotropy;
        let smp = self.pipes.sampler(&self.device, if exact { &nearest } else { &linear }, aniso);
        let lut_smp = self.pipes.sampler(&self.device, &nearest, aniso);
        unsafe {
            let d = &self.device;
            let to_attach = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .image(sc_image)
                .subresource_range(
                    vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1),
                );
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&to_attach)));
            let att = [vk::RenderingAttachmentInfo::default()
                .image_view(sc_view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] } })];
            let ri = vk::RenderingInfo::default()
                .render_area(vk::Rect2D { offset: vk::Offset2D::default(), extent })
                .layer_count(1)
                .color_attachments(&att);
            d.cmd_begin_rendering(cmd, &ri);
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe);
            d.cmd_set_viewport(cmd, 0, &[vk::Viewport { x: dx, y: dy, width: dw.max(1.0), height: dh.max(1.0), min_depth: 0.0, max_depth: 1.0 }]);
            d.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D::default(), extent }]);
            let bi = [vk::DescriptorBufferInfo { buffer: ub.buffer, offset: ub.offset, range: 32 }];
            let src_i = [vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: src_view, image_layout: vk::ImageLayout::GENERAL }];
            let lut_i = [vk::DescriptorImageInfo { sampler: vk::Sampler::null(), image_view: lut_view, image_layout: vk::ImageLayout::GENERAL }];
            let s0 = [vk::DescriptorImageInfo { sampler: smp, image_view: vk::ImageView::null(), image_layout: vk::ImageLayout::UNDEFINED }];
            let s1 = [vk::DescriptorImageInfo { sampler: lut_smp, image_view: vk::ImageView::null(), image_layout: vk::ImageLayout::UNDEFINED }];
            let writes = [
                vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::UNIFORM_BUFFER).buffer_info(&bi),
                vk::WriteDescriptorSet::default().dst_binding(2).descriptor_type(vk::DescriptorType::SAMPLED_IMAGE).image_info(&src_i),
                vk::WriteDescriptorSet::default().dst_binding(3).descriptor_type(vk::DescriptorType::SAMPLED_IMAGE).image_info(&lut_i),
                vk::WriteDescriptorSet::default().dst_binding(6).descriptor_type(vk::DescriptorType::SAMPLER).image_info(&s0),
                vk::WriteDescriptorSet::default().dst_binding(7).descriptor_type(vk::DescriptorType::SAMPLER).image_info(&s1),
            ];
            self.push_d.cmd_push_descriptor_set(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipes.layout, 0, &writes);
            d.cmd_draw(cmd, 6, 1, 0, 0);
            d.cmd_end_rendering(cmd);
            let to_present = vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::NONE)
                .dst_access_mask(vk::AccessFlags2::NONE)
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .image(sc_image)
                .subresource_range(
                    vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1),
                );
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&to_present)));
        }
        self.submit_frame_for_present(acquire, done);
        unsafe {
            let sw = [swapchain];
            let idx = [index];
            let waits = [done];
            let pi = vk::PresentInfoKHR::default().wait_semaphores(&waits).swapchains(&sw).image_indices(&idx);
            match self.swapchain_d.queue_present(self.queue, &pi) {
                Ok(false) => {}
                Ok(true) => self.swap.as_mut().unwrap().needs_recreate = true,
                Err(e) => {
                    debug!("present failed: {e}");
                    if let Some(s) = self.swap.as_mut() {
                        s.needs_recreate = true;
                    }
                }
            }
        }
        if self.frames_presented < 3 {
            debug!("present #{}: done", self.frames_presented);
        }
        self.frames_presented += 1;
    }
}
