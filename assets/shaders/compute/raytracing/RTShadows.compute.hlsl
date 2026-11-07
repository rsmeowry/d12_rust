#include "../../lib/Core.hlsl"
#include "../../lib/Raytracing.hlsl"

cbuffer RTShadowPass : register(b2) {
    float2 passResolution;
    uint outTex;
    uint normalsTex;
    float3 sunDir;
    uint depthTex;
}

[numthreads(8, 8, 1)]
void CS(int3 threadId: SV_DispatchThreadID) {
    float2 uv = threadId.xy / passResolution;
    int2 gbufPix = int2(uv * resolution);
    float depth = g_Tex2D[depthTex].Load(int3(gbufPix, 0)).r;
    if(depth <= 0.) { // hit sky
        g_RWTex2D[outTex][threadId.xy] = 0.;
        return;
    }

    float3 pos = WorldPosFromDepth((threadId.xy + 0.5) / passResolution, depth);
    float3 normal = normalize(g_Tex2D[normalsTex].Load(int3(gbufPix, 0)).xyz * 2 - 1);

    float3 ro = pos + normal * 0.01;
    float3 dir = -sunDir;
    float dist;
    float2 visibility = TraceShadow(ro, dir, 100., dist) ? float2(0., dist) : float2(1., -1.);
    g_RWTex2D[outTex][threadId.xy] = float4(visibility, 0, 0);
}