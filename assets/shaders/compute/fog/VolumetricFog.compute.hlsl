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
    uint fogNoiseTex;
    uint lightCount;
    uint _padL[3];
}

#include <../../lib/Shadows.hlsl>

#define LIGHT_SCATTERING 0.3
#define LIGHT_CONTRIBUTION float3(0.8, 0.6, 0.6)

SamplerState LinearWrap : register(s0);
StructuredBuffer<Light> g_Lights : register(t3);

float Luma(float3 c) {
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}

float GetDensity(Texture3D tex, float3 wp) {
    float3 uvw = (wp / 8.) + float3(time * 0.1, 0, 0);
    return tex.SampleLevel(LinearWrap, uvw, 0.0).x * density * 2.;
}

float HenyeyGreenstein(float g2, float angle, float scattering) {
    float denom = 1.0 + g2 - 2.0 * scattering * angle;
    return (1.0 - g2) / (4.0 * PI * denom * sqrt(denom));
}

float3 SampleMainLight(float g2, Light light, float3 rd, float3 rpos, float density) {
    float phase = HenyeyGreenstein(g2, dot(rd, -light.direction), LIGHT_SCATTERING);
    float shadow = SampleCascades(rpos, float3(0, 0, 0), false);

    return light.color * LIGHT_CONTRIBUTION * phase * density * stepDistance * shadow;
}

[numthreads(8, 8, 1)]
void CS(uint3 id : SV_DispatchThreadID) {
    if (any(id.xy >= outSize))
        return;
    Texture3D noiseTex = g_Tex3D[fogNoiseTex];

    float g2 = dot(LIGHT_SCATTERING, LIGHT_SCATTERING);
    uint2 pixel = id.xy * downscale;
    float2 uv = (pixel + 0.5) / resolution;
    float depth = g_Tex2D[depthTex].Load(int3(pixel, 0)).r;

    float3 rd = normalize(WorldPosFromDepth(uv, 1.0) - cameraPos);
    float dist = depth == 0.0 ? maxDistance : min(distance(WorldPosFromDepth(uv, depth), cameraPos), maxDistance);

    float3 scattering = 0;
    float transmittance = 1.0;
    float distanceTravelled = DitherInterleavedGradientNoise(1., uv * resolution, time / max(0.0001, deltaTime));
    uint c = 0;

    [loop]
    while (distanceTravelled < dist) {
        float dt = min(stepDistance, dist - distanceTravelled);
        float3 pos = cameraPos + rd * (distanceTravelled + 0.5 * dt);

        float alpha = 1.0 - exp(-GetDensity(noiseTex, pos) * dt);
        scattering += transmittance * alpha * fogColor;
        transmittance *= 1.0 - alpha;
        distanceTravelled += dt;
        if (transmittance < 0.01) {
            break;
        }

        if((c & 2) == 0) {
            scattering += transmittance * SampleMainLight(g2, g_Lights[0], rd, pos, transmittance);
        }
    }

    g_RWTex2D[outTex][id.xy] = float4(scattering, transmittance);
}
