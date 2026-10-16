use crate::render::graph::FrameGraph;
use crate::render::graph::pass::{DepthUse, Pass, PassCtx, ResLoad};
use crate::render::graph::res::ResId;

impl<'a> FrameGraph<'a> {
    pub fn add_pass(&mut self, name: &'static str) -> PassBuilder<'_, 'a> {
        PassBuilder {
            graph: self,
            name,
            reads: Vec::new(),
            writes_color: Vec::new(),
            uses_depth: None,
            partial_depth: false,
            writes_uav: Vec::new(),
            has_side_effects: false,
        }
    }
}

pub struct PassBuilder<'g, 'f> {
    graph: &'g mut FrameGraph<'f>,
    name: &'static str,
    reads: Vec<ResId>,
    writes_color: Vec<(ResId, ResLoad)>,
    uses_depth: Option<(ResId, DepthUse, u32)>,
    partial_depth: bool,
    writes_uav: Vec<(ResId, ResLoad)>,
    has_side_effects: bool,
}

impl<'g, 'f> PassBuilder<'g, 'f> {
    pub fn reads(mut self, res: ResId) -> Self {
        self.reads.push(res);
        self
    }

    pub fn writes(mut self, res: ResId, load: ResLoad) -> Self {
        self.writes_color.push((res, load));
        self
    }

    pub fn writes_depth(self, res: ResId, load: ResLoad) -> Self {
        self.writes_depth_slice(res, load, 0)
    }

    pub fn writes_depth_slice(mut self, res: ResId, load: ResLoad, slice: u32) -> Self {
        self.uses_depth = Some((res, DepthUse::Write(load), slice));
        if slice != 0 {
            self.partial_depth = true;
        }
        self
    }

    pub fn writes_uav(mut self, res: ResId, load: ResLoad) -> Self {
        self.writes_uav.push((res, load));
        self
    }

    pub fn reads_depth(mut self, res: ResId) -> Self {
        self.uses_depth = Some((res, DepthUse::Test, 0));
        self
    }

    pub fn uses_backbuffer(mut self) -> Self {
        self.has_side_effects = true;
        self
    }

    pub fn exec<F: FnOnce(&PassCtx) -> anyhow::Result<()> + 'f>(self, exec: F) {
        let graph = self.graph;
        graph.passes.push(Pass {
            name: self.name,
            reads: self.reads,
            writes_color: self.writes_color,
            uses_depth: self.uses_depth,
            partial_depth: self.partial_depth,
            has_side_effects: self.has_side_effects,
            writes_uav: self.writes_uav,
            exec: Box::new(exec)
        });
    }
}