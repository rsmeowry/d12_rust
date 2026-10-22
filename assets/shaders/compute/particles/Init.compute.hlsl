#include "../../lib/Core.hlsl"
#include "../../lib/Particles.hlsl"
#include "../../lib/ParticlesCompute.hlsl"

RWStructuredBuffer<Particle> g_Particles : register(u0, space4);
RWStructuredBuffer<uint> g_DeadList : register(u1, space4);

// resets/inits all particles
[numthreads(256, 1, 1)]
void CS(uint id : SV_DispatchThreadID) {
    if (id == 0)
        g_RWBuffers[countersBuf].Store(DEAD_COUNTER, maxParticles); // all dead!
    if (id >= maxParticles)
        return;
    g_Particles[id].lifetimeSeconds = 0;
    g_DeadList[id] = id;
}