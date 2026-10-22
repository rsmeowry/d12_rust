#include "../../lib/Core.hlsl"
#include "../../lib/Particles.hlsl"
#include "../../lib/ParticlesCompute.hlsl"

RWStructuredBuffer<Particle> g_Particles : register(u0, space4);
ConsumeStructuredBuffer<uint> g_DeadList : register(u1, space4);

// appends recent particles over the DT
[numthreads(64, 1, 1)]
void CS(uint id : SV_DispatchThreadID) {
    uint free = g_RWBuffers[countersBuf].Load(SNAPSHOT);
    if (id >= emitCount || id >= free)
        return;
    g_Particles[g_DeadList.Consume()] = SpawnParticle(id);
}