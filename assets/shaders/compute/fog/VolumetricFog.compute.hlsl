#include <../../lib/Core.hlsl>
#include <../../lib/Lighting.hlsl>

cbuffer ComputePass : register(b2) {
    Cascades csm;
}

#include <../../lib/Shadows.hlsl>

float GetDensity(float3 wp) {
    return 0.1;
}

float3 EvalLight(float g2, Light light) {
    return float3(1, 1, 1);
}

[numthreads(16, 16, 1)]
void CS(int3 threadId : SV_DispatchThreadID) {

}