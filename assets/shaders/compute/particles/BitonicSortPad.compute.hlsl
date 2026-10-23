#include "../../lib/Core.hlsl"
#include "../../lib/Particles.hlsl"
#include "../../lib/ParticlesCompute.hlsl"

RWStructuredBuffer<uint2> g_SortList : register(u2, space4);

[numthreads(256, 1, 1)]
void CS(uint i : SV_DispatchThreadID) {
    if(i >= g_RWBuffers[countersBuf].Load(SORT_COUNTER) && i < maxParticles) {
        g_SortList[i] = uint2(0, 0x7f800000); // +inf
    }
}
