use crate::render::graph::FrameGraph;
use crate::render::graph::pass::{Pass, PassCtx, ResLoad};
use crate::render::graph::res::ResId;

impl<'a> FrameGraph<'a> {
    pub fn add_pass(&mut self, name: &'static str) -> PassBuilder<'_, 'a> {
        PassBuilder {
            graph: self,
            name,
            reads: Vec::new(),
            writes_color: Vec::new(),
            writes_depth: None,
            has_side_effects: false,
        }
    }
}

pub struct PassBuilder<'g, 'f> {
    graph: &'g mut FrameGraph<'f>,
    name: &'static str,
    reads: Vec<ResId>,
    writes_color: Vec<(ResId, ResLoad)>,
    writes_depth: Option<(ResId, ResLoad)>,
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

    pub fn writes_depth(mut self, res: ResId, load: ResLoad) -> Self {
        self.writes_depth = Some((res, load));
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
            writes_depth: self.writes_depth,
            has_side_effects: self.has_side_effects,
            exec: Box::new(exec)
        });
    }
}