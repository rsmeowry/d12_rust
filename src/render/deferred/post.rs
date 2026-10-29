use crate::render::deferred::GBuffer;
use crate::render::graph::pass::ResLoad;
use crate::render::graph::res::{ResId, ResSize};
use crate::render::graph::FrameGraph;
use crate::render::pipeline::desc::PipelineDesc;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::{Renderer, SCENE_FORMAT};
use crate::wrappers::graphics::GraphicsDevice;
use glam::Vec4;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::D3D12_CULL_MODE_NONE;

#[derive(Debug)]
pub struct PostProcessPass {
    post: PipelineId,
    post_intermediate: ResId
}

impl PostProcessPass {
    pub fn new(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let post = renderer.create_pipeline(PipelineDesc {
            rts: vec![SCENE_FORMAT],
            name: "Post",
            shader: "Post.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;

        let post_intermediate = renderer.gr.color(&renderer.gd, "PostOutput", SCENE_FORMAT, Vec4::ZERO, ResSize::FitScreen)?;

        Ok(Self {
            post,
            post_intermediate
        })
    }

    pub fn output_tex(&self) -> ResId {
        self.post_intermediate
    }

    pub fn add_generic_pass<'a>(&'a self, mut props: PostProperties, bloom: Option<ResId>, renderer: &'a Renderer,
                                albedo: ResId,
                                normals: ResId,
                                materials: ResId,
                                scene_color: ResId,
                                scene_depth: ResId,
                                graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let pipelines = renderer.pipelines();
        let rs = renderer.rs();
        if let Some(bloom) = bloom {
            props.bloom_tex = renderer.gr.get(bloom)?.srv_ptr();
        } else {
            props.bloom_intensity = 0.0;
        }
        let post_va = renderer.upload_cbuffer(&props)?;

        let mut pass = graph.add_pass("Post")
            .reads(albedo)
            .reads(normals)
            .reads(materials)
            .reads(scene_color)
            .reads(scene_depth);
        if let Some(bloom) = bloom {
            pass = pass.reads(bloom);
        }
        pass.writes(self.post_intermediate, ResLoad::Clear)
            .exec(move |ctx| {
                pipelines.get(self.post).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, post_va);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        Ok(())
    }


    pub fn add_pass<'a>(&'a self, mut props: PostProperties, bloom: Option<ResId>, renderer: &'a Renderer, gbuffer: &GBuffer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        self.add_generic_pass(props, bloom, renderer, gbuffer.albedo, gbuffer.normals, gbuffer.materials, gbuffer.scene_color, gbuffer.scene_depth, graph)
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PostProperties {
    pub albedo: u32,
    pub normals: u32,
    pub materials: u32,
    pub depth: u32,
    pub scene_color: u32,
    pub enable_sobel: u32,
    pub enable_vignette: u32,
    pub vignette_strength: f32,
    pub enable_posterization: u32,
    pub posterization_levels: u32,
    pub enable_fly_eye: u32,
    pub bloom_tex: u32,
    pub bloom_intensity: f32,
}

impl Default for PostProperties {
    fn default() -> Self {
        Self {
            albedo: 0,
            normals: 0,
            materials: 0,
            depth: 0,
            scene_color: 0,
            enable_sobel: 0,
            enable_vignette: 0,
            vignette_strength: 1.0,
            enable_posterization: 0,
            posterization_levels: 32,
            enable_fly_eye: 0,
            bloom_tex: 0,
            bloom_intensity: 0.05,
        }
    }
}

#[derive(Debug)]
pub struct BlitPass {
    blit: PipelineId,
    source: ResId,
    transform_hdr: bool
}

impl BlitPass {
    pub fn new(source: ResId, transform_hdr: bool, renderer: &mut Renderer) -> anyhow::Result<Self> {
        let blit = renderer.create_pipeline(PipelineDesc {
            rts: vec![GraphicsDevice::BB_RTV_FORMAT],
            name: "Blit",
            shader: "Blit.hlsl".into(),
            input: &[],
            depth: None,
            cull: D3D12_CULL_MODE_NONE,
            ..Default::default()
        })?;
        Ok(Self {
            blit,
            source,
            transform_hdr
        })
    }

    pub fn add_pass<'a>(&'a self, renderer: &'a Renderer, graph: &mut FrameGraph<'a>) -> anyhow::Result<()> {
        let gr = &renderer.gr;

        let blit_va = renderer.upload_cbuffer(&BlitProperties {
            source_tex: gr.get(self.source)?.srv_ptr(),
            transform_hdr: self.transform_hdr as u32,
        })?;

        let rs = renderer.rs();
        let pipelines = renderer.pipelines();

        graph.add_pass("Blit")
            .reads(self.source)
            .uses_backbuffer()
            .exec(move |ctx| {
                pipelines.get(self.blit).bind(ctx.cmd);
                rs.set_pass(ctx.cmd, blit_va);
                rs.set_draw_index(ctx.cmd, 0);
                unsafe {
                    ctx.cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.cmd.DrawInstanced(3, 1, 0, 0);
                }
                Ok(())
            });

        Ok(())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlitProperties {
    pub source_tex: u32,
    pub transform_hdr: u32,
}
