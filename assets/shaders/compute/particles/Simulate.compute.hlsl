#include "../../lib/Core.hlsl"
#include "../../lib/Particles.hlsl"
#include "../../lib/ParticlesCompute.hlsl"

RWStructuredBuffer<Particle> g_Particles : register(u0, space4);
AppendStructuredBuffer<uint> g_DeadList : register(u1, space4);
AppendStructuredBuffer<uint2> g_SortList : register(u2, space4);

bool HasDepthCollision(float3 pos, float rad, out float3 norm) {
    norm = float3(0, 0, 0);
    float4 cs = mul(viewProj, float4(pos, 1));
    if(cs.w <= 0)
        return false;
    float2 uv = (cs.xy / cs.w) * float2(0.5, -0.5) + 0.5;
    if(any(uv < 0.) || any(uv > 1.))
        return false;
    int3 px = int3(uv * resolution, 0);
    float depthAtPos = g_Tex2D[depthTex].Load(px).x;
    if(depthAtPos == 0) // sky
        return false;
    float3 surface = WorldPosFromDepth(uv, depthAtPos);
    float pen = distance(pos, cameraPos) + rad - distance(surface, cameraPos);
    if(pen < 0 || pen > thickness) // probably not close to surface
        return false;
    norm = normalize(g_Tex2D[normalTex].Load(px).xyz * 2 - 1);
    return true;
}

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
    float3 collisionNormal;
    if(HasDepthCollision(particle.pos, particle.size, collisionNormal)) {
        float velocityNorm = dot(particle.velocity, collisionNormal);
        if(velocityNorm < 0) {
            particle.velocity -= (1 + restitution) * velocityNorm * collisionNormal;
        }
        particle.pos = oldPos; // dont clip into geometry
    }
    if(particle.pos.y < particle.size) {
        // if we are outside of screen, kinda plane collision
        particle.pos.y = particle.size;
        particle.velocity *= -restitution;
    }

    g_Particles[id] = particle;
    if(particle.lifetimeSeconds <= 0) {
        g_DeadList.Append(id); // DEAD!
        return;
    }

    float3 depth = particle.pos - cameraPos;
    g_SortList.Append(uint2(id, asuint(length(depth) * sortOrder)));
}