#include "../../lib/Core.hlsl"
#include "../../lib/Particles.hlsl"
#include "../../lib/ParticlesCompute.hlsl"

// runs once to clear the sortlist
[numthreads(1, 1, 1)]
void CS(uint id : SV_DispatchThreadID) {
    RWByteAddressBuffer counter = g_RWBuffers[countersBuf];
    counter.Store(SNAPSHOT, counter.Load(DEAD_COUNTER)); // how many slots can we consume?
    counter.Store(SORT_COUNTER, 0); // and this one we need to reset each frame
}
