#include "../../lib/Core.hlsl"
#include "../../lib/Raytracing.hlsl"

cbuffer RTShadowPass : register(b2) {
    float2 passResolution;
    uint outTex;
    uint normalsTex;
    float3 sunDir;
    uint depthTex;
    float3 camForward;
}

[numthreads(8, 8, 1)]
void CS(int3 threadId: SV_DispatchThreadID) {
    uint2 px = threadId.xy;
    if (any(px >= uint2(passResolution)))
        return;

    uint2 gbufPix = px * uint2(resolution) / uint2(passResolution);

    float depth = g_Tex2D[depthTex].Load(int3(gbufPix, 0)).r;
    if(depth <= 0.) { // hit sky
        g_RWTex2D[outTex][threadId.xy] = float4(1, -1, 0, 0);
        return;
    }

    float3 pos = WorldPosFromDepth((gbufPix + 0.5) / resolution, depth);
    float3 normal = normalize(g_Tex2D[normalsTex].Load(int3(gbufPix, 0)).xyz * 2 - 1);
    float viewDepth = dot(pos - cameraPos, camForward);

    float3 V = normalize(cameraPos - pos);
    float bias = 0.005 + 0.001 * viewDepth;
    float3 ro = pos + (normal + V) * bias;
    float3 dir = -sunDir;
    float dist;
    float2 visibility = TraceShadow(ro, dir, 100., dist) ? float2(0., dist) : float2(1., -1.);

    g_RWTex2D[outTex][threadId.xy] = float4(visibility, viewDepth, 0);
}