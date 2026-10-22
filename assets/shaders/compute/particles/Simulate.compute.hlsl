#include "../../lib/Core.hlsl"
#include "../../lib/Particles.hlsl"
#include "../../lib/ParticlesCompute.hlsl"

RWStructuredBuffer<Particle> g_Particles : register(u0, space4);
AppendStructuredBuffer<uint> g_DeadList : register(u1, space4);
AppendStructuredBuffer<uint2> g_SortList : register(u2, space4);

// simulates the "physics" of the particles
[numthreads(256, 1, 1)]
void CS(uint id : SV_DispatchThreadID) {
    if(id >= maxParticles)
        return;
    Particle particle = g_Particles[id];
    if (particle.lifetimeSeconds <= 0)
        return; // doomed to die

    particle.lifetimeSeconds -= dt;
    float3 oldPos = particle.pos;
    particle.velocity += gravity * dt; // accel
    particle.velocity *= exp(-drag * dt); // drag
    particle.pos += particle.velocity * dt; // speed

    // no collision yet
    g_Particles[id] = particle;
    if(particle.lifetimeSeconds <= 0) {
        g_DeadList.Append(id); // DEAD!
        return;
    }

    float3 depth = particle.pos - cameraPos;
    g_SortList.Append(uint2(id, asuint(length(depth) * sortOrder)));
}