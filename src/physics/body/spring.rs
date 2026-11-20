use crate::physics::body::particle::BodyParticle;

#[derive(Debug, Clone)]
pub enum SpringKind {
    Structural,
    Shear
}

const MIN_SPRING_LEN: f32 = 1e-9;
#[derive(Debug, Clone)]
pub struct Spring {
    pub from: usize,
    pub to: usize,
    pub rest_len: f32,
    pub k: f32,
    pub c: f32,
    pub kind: SpringKind,
}

impl Spring {
    pub fn apply(&self, particles: &mut [BodyParticle]) {
        let delta_pos = particles[self.to].position - particles[self.from].position;
        let spring_len = delta_pos.length();
        if spring_len < MIN_SPRING_LEN {
            return;
        }
        let dir = delta_pos.normalize();
        let rel_v = (particles[self.to].velocity - particles[self.from].velocity).dot(dir);
        let force = (self.k * (spring_len - self.rest_len) + self.c * rel_v) * dir; // hooke
        particles[self.from].force += force; // pushing them together
        particles[self.to].force -= force;
    }

    pub fn potential_energy(&self, particles: &[BodyParticle]) -> f32 {
        let dl = (particles[self.to].position - particles[self.from].position).length() - self.rest_len;
        0.5 * self.k * dl * dl
    }
}