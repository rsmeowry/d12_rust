#define COMPUTE

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

#define LIGHT_SCATTERING 0.1
#define LIGHT_CONTRIBUTION float3(2.5, 2.5, 2.5)
#define NOISE_INTENSITY 0.2

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

float3 SampleMainLight(float g2, Light light, float3 rd, float3 rpos) {
    float phase = HenyeyGreenstein(g2, dot(rd, -light.direction), LIGHT_SCATTERING);
    float shadow = SampleCascades(rpos, float3(0, 0, 0), false);

    return light.color * LIGHT_CONTRIBUTION * phase * shadow;
}

float3 SampleLight(float g2, Light light, float3 rd, float3 rpos) {
    float3 L = normalize(light.position - rpos);
    float phase = HenyeyGreenstein(g2, dot(rd, L), LIGHT_SCATTERING);
    float dist = length(rpos - light.position);
    float atten = DistanceAttenuation(dist, light.range);
    if (atten <= 0.0) {
        return float3(0, 0, 0);
    }
    float shadow = light.type == LIGHT_POINT ? PointShadow(light, rpos, float3(0, 0, 0), false) : SpotShadow(light, rpos, float3(0, 0, 0), false);
    float3 o = light.color
        * light.intensity
        * Luma(LIGHT_CONTRIBUTION).xxx
        * phase
        * shadow
        * atten;

    if(light.type == LIGHT_SPOT) {
        o *= smoothstep(light.spotOuterCos, light.spotInnerCos, dot(-L, light.direction));
    }

    return o;
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
    float distanceTravelled = stepDistance * NOISE_INTENSITY * InterleavedGradientNoise(float2(id.xy));
    uint c = 0;

    [loop]
    while (distanceTravelled < dist) {
        float dt = min(stepDistance, dist - distanceTravelled);
        float3 pos = cameraPos + rd * (distanceTravelled + 0.5 * dt);

        float alpha = 1.0 - exp(-GetDensity(noiseTex, pos) * dt);

        float3 finalColor = fogColor;
        if((c & 2) == 0) {
            finalColor += SampleMainLight(g2, g_Lights[0], rd, pos);

            for(uint lightIdx = 1; lightIdx < lightCount; lightIdx++) {
                finalColor += SampleLight(g2, g_Lights[lightIdx], rd, pos);
            }
        }
        scattering += transmittance * alpha * finalColor;
        transmittance *= 1.0 - alpha;
        distanceTravelled += dt;

        if(transmittance < 0.01) {
            break;
        }
    }

    g_RWTex2D[outTex][id.xy] = float4(scattering, transmittance);
}
