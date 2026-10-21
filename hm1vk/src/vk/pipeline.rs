//! Pipeline layout, shader modules, fixed-function pipeline cache and samplers.

use std::collections::HashMap;

use ash::vk;

use super::COLOR_FORMAT;

const FF_VS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ff_vs_main.spv"));
const FF_FS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ff_fs_main.spv"));
const PRESENT_VS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/present_vs_main.spv"));
const PRESENT_FS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/present_fs_main.spv"));

/// Size in bytes of one canonical vertex (see `device::CanonVertex`).
pub const VERTEX_STRIDE: u32 = 84;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlendKey {
    pub src: vk::BlendFactor,
    pub dst: vk::BlendFactor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StencilState {
    pub fail: vk::StencilOp,
    pub zfail: vk::StencilOp,
    pub pass: vk::StencilOp,
    pub cmp: vk::CompareOp,
    pub reference: u32,
    pub read_mask: u32,
    pub write_mask: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RasterState {
    pub blend: Option<BlendKey>,
    pub color_write: bool,
    pub cull: vk::CullModeFlags,
    pub topology: vk::PrimitiveTopology,
    pub polygon: vk::PolygonMode,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_cmp: vk::CompareOp,
    pub depth_bias: f32,
    pub stencil: Option<StencilState>,
    pub viewport: vk::Viewport,
    pub scissor: vk::Rect2D,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PipeKey {
    pub blend: Option<BlendKey>,
    pub color_write: bool,
    pub topology_class: u8,
    pub polygon: i32,
    pub has_color: bool,
    pub has_depth: bool,
}

impl PipeKey {
    pub fn from_raster(r: &RasterState, has_color: bool, has_depth: bool) -> PipeKey {
        let topology_class = match r.topology {
            vk::PrimitiveTopology::POINT_LIST => 0,
            vk::PrimitiveTopology::LINE_LIST | vk::PrimitiveTopology::LINE_STRIP => 1,
            _ => 2,
        };
        PipeKey {
            blend: r.blend,
            color_write: r.color_write,
            topology_class,
            polygon: r.polygon.as_raw(),
            has_color,
            has_depth,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerKey {
    pub mag_linear: bool,
    pub min_linear: bool,
    /// 0 none, 1 point, 2 linear
    pub mip: u8,
    pub address_u: u8,
    pub address_v: u8,
    pub anisotropy: u8,
    /// MIPMAPLODBIAS as f32 bits
    pub lod_bias: u32,
    pub max_mip_level: u8,
    /// 0 transparent black, 1 opaque black, 2 opaque white
    pub border: u8,
}

impl Default for SamplerKey {
    fn default() -> Self {
        SamplerKey {
            mag_linear: false,
            min_linear: false,
            mip: 0,
            address_u: 1,
            address_v: 1,
            anisotropy: 1,
            lod_bias: 0,
            max_mip_level: 0,
            border: 0,
        }
    }
}

pub struct Pipelines {
    pub set_layout: vk::DescriptorSetLayout,
    pub layout: vk::PipelineLayout,
    ff_vs: vk::ShaderModule,
    ff_fs: vk::ShaderModule,
    present_vs: vk::ShaderModule,
    present_fs: vk::ShaderModule,
    depth_format: vk::Format,
    cache: HashMap<PipeKey, vk::Pipeline>,
    present: HashMap<vk::Format, vk::Pipeline>,
    samplers: HashMap<(SamplerKey, u32), vk::Sampler>,
}

fn shader(device: &ash::Device, bytes: &[u8]) -> Result<vk::ShaderModule, String> {
    let words: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    unsafe {
        device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
            .map_err(|e| format!("shader module: {e}"))
    }
}

impl Pipelines {
    pub fn new(device: &ash::Device, depth_format: vk::Format) -> Result<Pipelines, String> {
        unsafe {
            let stages = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
            let mut bindings = vec![
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(stages),
                vk::DescriptorSetLayoutBinding::default()
                    .binding(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(stages),
            ];
            for i in 0..4 {
                bindings.push(
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(2 + i)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                );
                bindings.push(
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(6 + i)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                );
            }
            let set_layout = device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR)
                        .bindings(&bindings),
                    None,
                )
                .map_err(|e| format!("set layout: {e}"))?;
            let layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(std::slice::from_ref(&set_layout)),
                    None,
                )
                .map_err(|e| format!("pipeline layout: {e}"))?;
            Ok(Pipelines {
                set_layout,
                layout,
                ff_vs: shader(device, FF_VS)?,
                ff_fs: shader(device, FF_FS)?,
                present_vs: shader(device, PRESENT_VS)?,
                present_fs: shader(device, PRESENT_FS)?,
                depth_format,
                cache: HashMap::new(),
                present: HashMap::new(),
                samplers: HashMap::new(),
            })
        }
    }

    pub fn ff_pipeline(&mut self, device: &ash::Device, key: &PipeKey) -> Option<vk::Pipeline> {
        if let Some(p) = self.cache.get(key) {
            return Some(*p);
        }
        let p = unsafe { self.build_ff(device, key) };
        match p {
            Ok(p) => {
                self.cache.insert(*key, p);
                Some(p)
            }
            Err(e) => {
                crate::info!("pipeline creation failed for {key:?}: {e}");
                None
            }
        }
    }

    unsafe fn build_ff(&self, device: &ash::Device, key: &PipeKey) -> Result<vk::Pipeline, vk::Result> {
        let entry = c"vs_main";
        let fentry = c"fs_main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(self.ff_vs).name(entry),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(self.ff_fs)
                .name(fentry),
        ];
        let vb = [vk::VertexInputBindingDescription { binding: 0, stride: VERTEX_STRIDE, input_rate: vk::VertexInputRate::VERTEX }];
        let attrs = [
            (0, vk::Format::R32G32B32A32_SFLOAT, 0),
            (1, vk::Format::R32G32B32_SFLOAT, 16),
            (2, vk::Format::B8G8R8A8_UNORM, 28),
            (3, vk::Format::B8G8R8A8_UNORM, 32),
            (4, vk::Format::R32G32B32A32_SFLOAT, 36),
            (5, vk::Format::R32G32B32A32_SFLOAT, 52),
            (6, vk::Format::R32G32_SFLOAT, 68),
            (7, vk::Format::R32G32_SFLOAT, 76),
        ]
        .map(|(l, f, o)| vk::VertexInputAttributeDescription { location: l, binding: 0, format: f, offset: o });
        let vi = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&vb)
            .vertex_attribute_descriptions(&attrs);
        let topo = match key.topology_class {
            0 => vk::PrimitiveTopology::POINT_LIST,
            1 => vk::PrimitiveTopology::LINE_LIST,
            _ => vk::PrimitiveTopology::TRIANGLE_LIST,
        };
        let ia = vk::PipelineInputAssemblyStateCreateInfo::default().topology(topo);
        let vp = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let rs = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::from_raw(key.polygon))
            .line_width(1.0);
        let ms = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let ds = vk::PipelineDepthStencilStateCreateInfo::default();
        let mut att = vk::PipelineColorBlendAttachmentState::default().color_write_mask(if key.color_write {
            vk::ColorComponentFlags::RGBA
        } else {
            vk::ColorComponentFlags::empty()
        });
        if let Some(b) = key.blend {
            att = att
                .blend_enable(true)
                .src_color_blend_factor(b.src)
                .dst_color_blend_factor(b.dst)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(alpha_factor(b.src))
                .dst_alpha_blend_factor(alpha_factor(b.dst))
                .alpha_blend_op(vk::BlendOp::ADD);
        }
        let atts = [att];
        let cb = vk::PipelineColorBlendStateCreateInfo::default().attachments(if key.has_color { &atts } else { &[] });
        let dyns = [
            vk::DynamicState::VIEWPORT,
            vk::DynamicState::SCISSOR,
            vk::DynamicState::CULL_MODE,
            vk::DynamicState::FRONT_FACE,
            vk::DynamicState::PRIMITIVE_TOPOLOGY,
            vk::DynamicState::DEPTH_TEST_ENABLE,
            vk::DynamicState::DEPTH_WRITE_ENABLE,
            vk::DynamicState::DEPTH_COMPARE_OP,
            vk::DynamicState::DEPTH_BIAS_ENABLE,
            vk::DynamicState::DEPTH_BIAS,
            vk::DynamicState::STENCIL_TEST_ENABLE,
            vk::DynamicState::STENCIL_OP,
            vk::DynamicState::STENCIL_COMPARE_MASK,
            vk::DynamicState::STENCIL_WRITE_MASK,
            vk::DynamicState::STENCIL_REFERENCE,
        ];
        let dy = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dyns);
        let cfmt = [COLOR_FORMAT];
        let mut rinfo = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(if key.has_color {
            &cfmt
        } else {
            &[]
        });
        if key.has_depth {
            rinfo = rinfo.depth_attachment_format(self.depth_format).stencil_attachment_format(self.depth_format);
        }
        let gp = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vi)
            .input_assembly_state(&ia)
            .viewport_state(&vp)
            .rasterization_state(&rs)
            .multisample_state(&ms)
            .depth_stencil_state(&ds)
            .color_blend_state(&cb)
            .dynamic_state(&dy)
            .layout(self.layout)
            .push_next(&mut rinfo);
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[gp], None)
            .map(|v| v[0])
            .map_err(|(_, e)| e)
    }

    pub fn present_pipeline(&mut self, device: &ash::Device, format: vk::Format) -> Option<vk::Pipeline> {
        if let Some(p) = self.present.get(&format) {
            return Some(*p);
        }
        unsafe {
            let entry = c"vs_main";
            let fentry = c"fs_main";
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(self.present_vs)
                    .name(entry),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(self.present_fs)
                    .name(fentry),
            ];
            let vi = vk::PipelineVertexInputStateCreateInfo::default();
            let ia = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let vp = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
            let rs = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .line_width(1.0);
            let ms = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let atts = [vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA)];
            let cb = vk::PipelineColorBlendStateCreateInfo::default().attachments(&atts);
            let dyns = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dy = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dyns);
            let cfmt = [format];
            let mut rinfo = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&cfmt);
            let gp = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vi)
                .input_assembly_state(&ia)
                .viewport_state(&vp)
                .rasterization_state(&rs)
                .multisample_state(&ms)
                .color_blend_state(&cb)
                .dynamic_state(&dy)
                .layout(self.layout)
                .push_next(&mut rinfo);
            match device.create_graphics_pipelines(vk::PipelineCache::null(), &[gp], None) {
                Ok(v) => {
                    self.present.insert(format, v[0]);
                    Some(v[0])
                }
                Err((_, e)) => {
                    crate::info!("present pipeline creation failed: {e}");
                    None
                }
            }
        }
    }

    pub fn sampler(&mut self, device: &ash::Device, key: &SamplerKey, max_aniso: f32) -> vk::Sampler {
        let force = crate::state::config().force_anisotropy;
        let ck = (*key, force);
        if let Some(s) = self.samplers.get(&ck) {
            return *s;
        }
        let addr = |a: u8| match a {
            2 => vk::SamplerAddressMode::MIRRORED_REPEAT,
            3 => vk::SamplerAddressMode::CLAMP_TO_EDGE,
            4 => vk::SamplerAddressMode::CLAMP_TO_BORDER,
            _ => vk::SamplerAddressMode::REPEAT,
        };
        let mut aniso = key.anisotropy.max(1) as f32;
        if force > 1 && key.min_linear {
            aniso = aniso.max(force as f32);
        }
        aniso = aniso.min(max_aniso);
        let lod_bias = f32::from_bits(key.lod_bias);
        let (mipmode, max_lod) = match key.mip {
            0 => (vk::SamplerMipmapMode::NEAREST, 0.25),
            1 => (vk::SamplerMipmapMode::NEAREST, vk::LOD_CLAMP_NONE),
            _ => (vk::SamplerMipmapMode::LINEAR, vk::LOD_CLAMP_NONE),
        };
        let border = match key.border {
            1 => vk::BorderColor::FLOAT_OPAQUE_BLACK,
            2 => vk::BorderColor::FLOAT_OPAQUE_WHITE,
            _ => vk::BorderColor::FLOAT_TRANSPARENT_BLACK,
        };
        let ci = vk::SamplerCreateInfo::default()
            .mag_filter(if key.mag_linear { vk::Filter::LINEAR } else { vk::Filter::NEAREST })
            .min_filter(if key.min_linear { vk::Filter::LINEAR } else { vk::Filter::NEAREST })
            .mipmap_mode(mipmode)
            .address_mode_u(addr(key.address_u))
            .address_mode_v(addr(key.address_v))
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .mip_lod_bias(lod_bias.clamp(-15.0, 15.0))
            .anisotropy_enable(aniso > 1.0)
            .max_anisotropy(aniso)
            .min_lod(key.max_mip_level as f32)
            .max_lod(if key.mip == 0 { key.max_mip_level as f32 + 0.25 } else { max_lod })
            .border_color(border);
        let s = unsafe { device.create_sampler(&ci, None).unwrap_or_default() };
        self.samplers.insert(ck, s);
        s
    }

    pub fn destroy(&mut self, device: &ash::Device) {
        unsafe {
            for (_, p) in self.cache.drain() {
                device.destroy_pipeline(p, None);
            }
            for (_, p) in self.present.drain() {
                device.destroy_pipeline(p, None);
            }
            for (_, s) in self.samplers.drain() {
                device.destroy_sampler(s, None);
            }
            device.destroy_shader_module(self.ff_vs, None);
            device.destroy_shader_module(self.ff_fs, None);
            device.destroy_shader_module(self.present_vs, None);
            device.destroy_shader_module(self.present_fs, None);
            device.destroy_pipeline_layout(self.layout, None);
            device.destroy_descriptor_set_layout(self.set_layout, None);
        }
    }
}

/// D3D applies the colour blend factor to alpha as well; map the colour
/// factors to their alpha equivalents.
fn alpha_factor(f: vk::BlendFactor) -> vk::BlendFactor {
    match f {
        vk::BlendFactor::SRC_COLOR => vk::BlendFactor::SRC_ALPHA,
        vk::BlendFactor::ONE_MINUS_SRC_COLOR => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        vk::BlendFactor::DST_COLOR => vk::BlendFactor::DST_ALPHA,
        vk::BlendFactor::ONE_MINUS_DST_COLOR => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        vk::BlendFactor::SRC_ALPHA_SATURATE => vk::BlendFactor::ONE,
        other => other,
    }
}
