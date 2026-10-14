#include <../../lib/Core.hlsl>
#include <../../lib/Lighting.hlsl>

cbuffer FogPass : register(b2) {
    Cascades csm;
    uint2 outSize;
    uint depthTex;
    uint outTex;
    uint downscale;
    float maxDistance;
    float stepDistance;
    float density;
    float3 fogColor;
    uint fogTex;
}

#include <../../lib/Shadows.hlsl>

#define LIGHT_SCATTERING 0.3
#define LIGHT_CONTRIBUTION float3(0.8, 0.6, 0.6)

float GetDensity(float3 wp) {
    return density;
}

float HenyeyGreenstein(float g2, float angle, float scattering) {
    float denom = 1.0 + g2 - 2.0 * scattering * angle;
    return (1.0 - g2) / (4.0 * PI * denom * sqrt(denom));
}

[numthreads(8, 8, 1)]
void CS(uint3 id : SV_DispatchThreadID) {
    if (any(id.xy >= outSize))
        return;

    uint2 pixel = id.xy * downscale;
    float2 uv = (pixel + 0.5) / resolution;
    float depth = g_Tex2D[depthTex].Load(int3(pixel, 0)).r;

    float3 rd = normalize(WorldPosFromDepth(uv, 1.0) - cameraPos);
    float dist = depth == 0.0 ? maxDistance : min(distance(WorldPosFromDepth(uv, depth), cameraPos), maxDistance);

    float3 scattering = 0;
    float transmittance = 1.0;
    float t = 0;

    [loop]
    while (t < dist) {
        float dt = min(stepDistance, dist - t);
        float3 pos = cameraPos + rd * (t + 0.5 * dt);

        float alpha = 1.0 - exp(-GetDensity(pos) * dt);
        scattering += transmittance * alpha * fogColor;
        transmittance *= 1.0 - alpha;
        t += dt;
    }

    g_RWTex2D[outTex][id.xy] = float4(scattering, transmittance);
}
