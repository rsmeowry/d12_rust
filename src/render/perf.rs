use std::borrow::Cow;
use std::time::Instant;
use crate::app::{CullingMode, RenderOpts};
use crate::assets::LoadedScene;
use crate::render::mesh::Mesh;
use crate::render::perf::frustum::Frustum;
use crate::render::perf::octree::Octree;
use crate::render::pipeline::PipelineId;
use crate::render::renderer::ObjectData;

pub mod frustum;
pub mod octree;

#[derive(Debug)]
pub struct Culler {
    scene_octree: Octree,
    total_objects: usize,
    culled_objects: usize,
    elapsed_time_ms: f64,
}

impl Culler {
    pub fn new(objects: &[ObjectData], meshes: &[&Mesh]) -> Self {
        let boxes = objects.iter().zip(meshes)
            .map(|(o, m)| m.aabb().transformed(o.model))
            .collect();
        Self {
            total_objects: 0,
            culled_objects: 0,
            scene_octree: Octree::build(boxes),
            elapsed_time_ms: 0.
        }
    }

    pub fn push_visible_raw<'a>(&mut self,
                                visible_objects: &mut Vec<ObjectData>,
                                visible_meshes: &mut Vec<(&'a Mesh, PipelineId)>,
                                all_objects: &Vec<ObjectData>,
                                all_meshes: &Vec<&'a Mesh>,
                                pso: PipelineId,
                                frustum: &Frustum,
                                opts: &mut RenderOpts) {
        let began = Instant::now();
        self.total_objects += all_objects.len();
        match opts.culling_mode {
            CullingMode::Off => {
                visible_objects.extend(all_objects);
                visible_meshes.extend(all_meshes.into_iter().map(|it| (*it, pso)));
            }
            CullingMode::Frustum => {
                for (o, m) in all_objects.into_iter().zip(all_meshes) {
                    if frustum.contains(m.aabb(), o.model) {
                        visible_objects.push(*o);
                        visible_meshes.push((m, pso))
                    } else {
                        self.culled_objects += 1;
                    }
                }
            }
            CullingMode::Octree => {
                let mut visible = Vec::new();
                self.scene_octree.query(&frustum, &mut visible);
                self.culled_objects += (all_objects.len() - visible.len());
                for i in visible {
                    visible_objects.push(all_objects[i as usize]);
                    visible_meshes.push((all_meshes[i as usize], pso));
                }
            }
        }
        let elapsed = Instant::now() - began;
        self.elapsed_time_ms += elapsed.as_secs_f64() * 1000.0;
    }

    pub fn push_visible<'a>(&mut self,
                            visible_objects: &mut Vec<ObjectData>,
                            visible_meshes: &mut Vec<(&'a Mesh, PipelineId)>,
                            scene: &'a LoadedScene,
                            pso: PipelineId,
                            frustum: &Frustum,
                            opts: &mut RenderOpts) {
        let (all_objects, all_meshes) = scene.to_objects_and_draws();
        self.push_visible_raw(visible_objects, visible_meshes, &all_objects, &all_meshes, pso, frustum, opts);
    }

    pub fn finish(&mut self, opts: &mut RenderOpts) {
        opts.push_debug_line(Cow::Owned(format!("Culling: {:?} | Vis: {} / {} | T: {:.2}ms", opts.culling_mode, self.total_objects - self.culled_objects, self.total_objects, self.elapsed_time_ms)));
        self.total_objects = 0;
        self.culled_objects = 0;
        self.elapsed_time_ms = 0.;
    }
}