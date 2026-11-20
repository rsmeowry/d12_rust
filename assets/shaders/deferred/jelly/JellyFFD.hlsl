#include "../../lib/Core.hlsl"
#include "../../lib/FFD.hlsl"

#define MAX_DISTANCE 15.
#define DENSITY 1.3
#define REFRACT_STRENGTH 1.3
// more r b absorption than g
#define ABSORPTION float3(1.5, 0.2, 1.7)
#define JELLY_COLOR_A float3(0.35, 0.8, 0.12)
#define JELLY_COLOR_B float3(0.5, 0.9, 0.08)
#define SPEC_COLOR float3(0.7, 0.8, 0.4)

SamplerState LinearClamp : register(s1);

cbuffer TransparentPass : register(b2) {
    uint depthTex;
    uint sceneTex;
    uint prepassTex;
    uint irradianceTex;
    float3 lightDir;
    uint _padd;
}

float4 frag(Varyings i): SV_Target {
    Texture2D scene = g_Tex2D[sceneTex];
    float backfaceDepth = g_Tex2D[prepassTex].Load(int3(i.pos.xy, 0)).r;

    uint2 pos = uint2(i.pos.xy);

    float frontDist = distance(i.posW, cameraPos);
    float3 rd = normalize(i.posW - cameraPos);
    float sceneZ = g_Tex2D[depthTex].Load(int3(pos, 0)).r;

    clip(i.pos.z - sceneZ);

    float sceneDist = sceneZ == 0.0 ? MAX_DISTANCE : distance(WorldPosFromDepth(i.pos.xy, sceneZ), cameraPos);

    float thickness = max(min(backfaceDepth, sceneDist) - frontDist, 0.0);

    // refraction
    float3 N = normalize(i.normalW);
    float3 refrDir = refract(rd, N, 1.0 / REFRACT_STRENGTH);
    float3 exitPoint = i.posW + refrDir * thickness;
    float4 exitClip = mul(viewProj, float4(exitPoint, 1.));
    float2 exitSs = (exitClip / exitClip.w) * float2(0.5, -0.5) + 0.5;
    uint2 refrPix = uint2(exitSs * resolution);
    float refrZ = g_Tex2D[depthTex].Load(int3(refrPix, 0)).r;
    if (refrZ > i.pos.z)
        refrPix = pos;

    float4 baseScene = scene.Load(int3(refrPix, 0));

    // absorption
    float3 transmittance = exp(-ABSORPTION * thickness);
    float3 transmitted = baseScene * transmittance;

    // subsurface stuff
    float wrapDiff = saturate((dot(N, -lightDir) + 0.5) / 1.5) + 0.1;
    float scatter = 1.0 - exp(-DENSITY * thickness);
    float3 inscatter = JELLY_COLOR_A * wrapDiff * scatter;

    // fresnel
    float3 V = -rd;
    float3 H = normalize(V - lightDir);
    float F = 0.04 + 0.96 * pow(1.0 - saturate(dot(N, V)), 3.);
    float3 irradiance = g_TexCube[irradianceTex].SampleLevel(LinearClamp, reflect(rd, N), 0).rgb * JELLY_COLOR_B;

    float3 spec = pow(saturate(dot(N, H)), 128.0) * SPEC_COLOR;

    float3 col = transmitted + inscatter + F * irradiance + spec;

    return float4(col, 1);
}