use glam::{DVec3, IVec2, IVec3, UVec3, Vec3Swizzles};
use noise::{Billow, Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti, Simplex, SuperSimplex, Worley};
use crate::render::voxels::generate::GenerationStrategy;
use crate::render::voxels::{uvec3_to_index, NOISE_SCALE};
use crate::render::voxels::svo::Voxel;
use crate::scenes::voxels::{VOXEL_SCENE_MAT_DIAMOND_BLOCK, VOXEL_SCENE_MAT_DIRT, VOXEL_SCENE_MAT_MOSS, VOXEL_SCENE_MAT_ORE, VOXEL_SCENE_MAT_SNOW, VOXEL_SCENE_MAT_STONE};

#[derive(Debug, Clone)]
pub struct GroundGenStrategy {
    ridged: RidgedMulti<SuperSimplex>,
    billow: Billow<Simplex>,
    erosion: Fbm<SuperSimplex>,
    cave_noise: Fbm<Simplex>,
    cave_b: Fbm<Simplex>,
    features: Fbm<SuperSimplex>,
    continent: Fbm<SuperSimplex>,
    temperature: Fbm<SuperSimplex>,
    humidity: Fbm<SuperSimplex>,
    warp: Fbm<SuperSimplex>,
}

struct Biome {
    t: f64,
    h: f64,
    top: u8,
    filler: u8,
    filler_depth: f64,
    amp: f64,
    ridge_w: f64
}

struct Column {
    h: f64,
    slope: f64,
    temp: f64,
    biome: &'static Biome
}

const BIOMES: &[Biome] = &[
    Biome { t: -0.6, h:  0.0, top: VOXEL_SCENE_MAT_SNOW, filler: VOXEL_SCENE_MAT_STONE, filler_depth: 2., amp: 10., ridge_w: 1.0 },
    Biome { t:  0.0, h:  0.3, top: VOXEL_SCENE_MAT_MOSS, filler: VOXEL_SCENE_MAT_DIRT,  filler_depth: 4., amp:  6., ridge_w: 0.6 },
    Biome { t:  0.3, h: -0.4, top: VOXEL_SCENE_MAT_DIRT, filler: VOXEL_SCENE_MAT_DIRT,  filler_depth: 3., amp:  3., ridge_w: 0.2 },
];

impl GroundGenStrategy {
    pub fn new(control_seed: u32) -> Self {
        Self {
            ridged: RidgedMulti::new(control_seed).set_octaves(2).set_frequency(1. / 512.).set_persistence(0.2),
            billow: Billow::new((control_seed >> 1) + 255).set_octaves(3).set_lacunarity(2.3).set_frequency(1. / 128.),
            erosion: Fbm::new((control_seed >> 2) + 604).set_octaves(5).set_frequency(1. / 256.).set_persistence(0.7),
            cave_noise: Fbm::new(control_seed + 235).set_octaves(2).set_lacunarity(2.6).set_frequency(1. / 64.),
            cave_b: Fbm::new(control_seed + 654).set_octaves(2).set_lacunarity(2.0).set_frequency(1. / 64.),
            features: Fbm::new(control_seed + 521).set_octaves(3).set_lacunarity(1.5).set_frequency(1. / 32.),
            continent: Fbm::new(control_seed + 50).set_octaves(2).set_lacunarity(2.0).set_frequency(1. / 512.),
            temperature: Fbm::new(control_seed + 90).set_octaves(2).set_frequency(1. / 1024.),
            humidity: Fbm::new(control_seed + 1200).set_octaves(2).set_frequency(1. / 1024.),
            warp: Fbm::new(control_seed + 5400).set_octaves(2).set_frequency(1. / 128.)
        }
    }

    fn blend(t: f64, h: f64) -> (f64, f64) {
        let (mut sw, mut amp, mut ridge) = (0., 0., 0.);
        for b in BIOMES {
            let w = 1. / ((b.t - t).powi(2) + (b.h - h).powi(2) + 1e-3).powi(2);
            sw += w;
            amp += b.amp * w;
            ridge += b.ridge_w * w;
        }
        (amp / sw, ridge / sw)
    }

    fn pick(t: f64, h: f64) -> &'static Biome {
        BIOMES.iter().min_by(|a, b| {
            let da = (a.t - t).powi(2) + (a.h - h).powi(2);
            let db = (b.t - t).powi(2) + (b.h - h).powi(2);
            da.total_cmp(&db)
        }).unwrap()
    }

    fn curve(x: f64, pts: &[(f64, f64)]) -> f64 {
        let i = pts.partition_point(|p| p.0 < x).clamp(1, pts.len() - 1);
        let ((x0, y0), (x1, y1)) = (pts[i - 1], pts[i]);
        let t = ((x - x0) / (x1 - x0)).clamp(0., 1.);
        y0 + (y1 - y0) * t
    }

    fn norm(n: f64) -> f64 {
        (n / 0.55).clamp(-1., 1.)
    }

    fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
        let t = ((x - a) / (b - a)).clamp(0., 1.); t * t * (3. - 2. * t)
    }

    fn column(&self, x: f64, z: f64) -> (f64, f64, f64) {
        let cont = Self::norm(self.continent.get([x, z]));
        let ero  = Self::norm(self.erosion.get([x, z])) * 0.5 + 0.5;
        let base = Self::curve(cont, &[(-1., -40.), (-0.3, -10.), (-0.1, 0.), (0., 2.), (0.4, 12.), (1., 35.)]);

        let (t, hum) = (Self::norm(self.temperature.get([x, z])), Self::norm(self.humidity.get([x, z])));
        let (amp, ridge_w) = Self::blend(t, hum);

        let (wx, wz) = (self.warp.get([x, z]) * 24., self.warp.get([x + 517., z - 931.]) * 24.);
        let hills = self.billow.get([x + wx, z + wz]) * amp * (1.3 - ero);

        let mountain = Self::smoothstep(0.05, 0.4, cont) * Self::smoothstep(0.65, 0.3, ero);
        let peaks = self.ridged.get([x + wx, z + wz]).max(0.).powf(1.5) * 70. * ridge_w * mountain;

        let h = base + hills + peaks;
        (h, t - h * 0.008, hum)
    }

    fn sample_caves(&self, world_pos: IVec3, d: f64) -> f64 {
        let q = [world_pos.x as f64, world_pos.y as f64, world_pos.z as f64];

        // spaghetti
        let tunnel = 0.06 - self.cave_noise.get(q).abs().max(self.cave_b.get(q).abs());
        // cheese
        let cheese = self.cave_noise.get(q) - 0.55;

        let c = tunnel.max(cheese) * 24.;
        let fade = (-d / 8.).clamp(0., 1.);
        c - (1. - fade) * 4.
    }
}

impl GenerationStrategy for GroundGenStrategy {
    fn generate_chunk(&self, origin: IVec3, size: usize, out: &mut [Voxel]) {
        let w = size + 2;
        let raw: Vec<(f64, f64, f64)> = (0..w * w).map(|i| {
            let (cx, cz) = ((i % w) as i32 - 1, (i / w) as i32 - 1);
            self.column((origin.x + cx) as f64, (origin.z + cz) as f64)
        }).collect();
        let at = |x: usize, z: usize| raw[(x + 1) + (z + 1) * w];
        let r = |x: usize, z: usize| raw[z * w + x];

        if raw.iter().all(|c| c.0 < origin.y as f64 - 2.) {
            return;
        }

        for z in 0..size {
            for x in 0..size {
                let (h, t, hum) = r(x + 1, z + 1);
                let slope = (r(x + 2, z + 1).0 - r(x, z + 1).0).hypot(r(x + 1, z + 2).0 - r(x + 1, z).0) / 2.;
                let (wx, wz) = (origin.x + x as i32, origin.z + z as i32);
                let biome = Self::pick(t + self.features.get([wx as f64, wz as f64]) * 0.05, hum);
                let norm = (1. + slope * slope).sqrt();

                for y in 0..size {
                    let abs_y = origin.y + y as i32;
                    let depth = h - abs_y as f64;
                    let mut d = -depth / norm;
                    if d < 0. {
                        d = d.max(self.sample_caves(IVec3::new(wx, abs_y, wz), d));
                    }

                    let material = if slope > 1.3 && depth < 3. {
                        if self.features.get([wx as f64, abs_y as f64, wz as f64]) > 0.4 { VOXEL_SCENE_MAT_ORE }
                        else { VOXEL_SCENE_MAT_STONE }
                    }
                    else if depth < 1. { biome.top }
                    else if depth < biome.filler_depth { biome.filler }
                    else { VOXEL_SCENE_MAT_STONE };

                    out[uvec3_to_index(UVec3::new(x as u32, y as u32, z as u32), size)] = Voxel {
                        density: (d.clamp(-2., 2.) * 63.) as i8,
                        material,
                    };
                }
            }
        }
    }
}