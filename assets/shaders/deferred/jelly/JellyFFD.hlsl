#include "../../lib/Core.hlsl"
#include "../../lib/FFD.hlsl"

#define MAX_DISTANCE 30.
//#define DENSITY 1.3
//#define REFRACT_STRENGTH 1.3
//// more r b absorption than g
//#define ABSORPTION float3(0.1, 1.4, 0.9)
//#define JELLY_COLOR_A float3(0.8, 0.2, 0.22)
//#define JELLY_COLOR_B float3(1.0, 0.2, 0.3)
//#define SPEC_COLOR float3(0.7, 0.8, 0.4)
#define MAX_BACKFACE_LAYERS 8

SamplerState LinearClamp : register(s1);

cbuffer TransparentPass : register(b2) {
    uint depthTex;
    uint sceneTex;
    uint _paddd;
    uint irradianceTex;
    float3 lightDir;
    uint _padd;
    uint prepassTexs[MAX_BACKFACE_LAYERS];
}

struct JellyMaterial {
    uint _texpads[8];

    float3 specColor;
    uint _pad;

    float3 absorption;
    float density;
    float3 jellyColorA;
    float refractStrength;
    float3 jellyColorB;
    uint isChroma;
};

StructuredBuffer<JellyMaterial> g_Materials : register(t2);

float FindBackfaceDistance(uint2 pixel, float frontDistance)
{
    for (uint layer = 0; layer < MAX_BACKFACE_LAYERS; ++layer)
    {
        float2 hit = g_Tex2D[prepassTexs[layer]].Load(int3(pixel, 0));

        if (hit.y >= MAX_DISTANCE)
            break;

        if (hit.y > frontDistance + 0.0001) {
            return hit.y;
        }
    }

    return MAX_DISTANCE;
}

float4 frag(Varyings i): SV_Target {
    JellyMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    Texture2D scene = g_Tex2D[sceneTex];
    uint2 pos = uint2(i.pos.xy);

    float frontDist = distance(i.posW, cameraPos);
    float backfaceDist = FindBackfaceDistance(pos, frontDist);
    float3 rd = normalize(i.posW - cameraPos);
    float sceneZ = g_Tex2D[depthTex].Load(int3(pos, 0)).r;

    clip(i.pos.z - sceneZ);

    float sceneDist = sceneZ == 0.0 ? MAX_DISTANCE : distance(WorldPosFromDepth(i.pos.xy, sceneZ), cameraPos);

    float thickness = max(min(backfaceDist, sceneDist) - frontDist, 0.0) * 0.1;

    // refraction
    float3 N = normalize(i.normalW);
    float3 refrDir = refract(rd, N, 1.0 / mat.refractStrength);
    float cosTheta = max(dot(refrDir, rd), 0.001);
    float pathLength = thickness / cosTheta;
    float3 exitPoint = i.posW + refrDir * pathLength;
    float4 exitClip = mul(viewProj, float4(exitPoint, 1.));
    float2 exitSs = (exitClip / exitClip.w) * float2(0.5, -0.5) + 0.5;
    uint2 refrPix = uint2(exitSs * resolution);
    float refrZ = g_Tex2D[depthTex].Load(int3(refrPix, 0)).r;
    if (refrZ > i.pos.z)
        refrPix = pos;

    float4 baseScene = scene.Load(int3(refrPix, 0));

    // absorption
    float3 transmittance = exp(-mat.absorption * thickness);
    float3 transmitted = baseScene * transmittance;

    // subsurface stuff
    float wrapDiff = saturate((dot(N, -lightDir) + 0.5) / 1.5) + 0.1;
    float scatter = 1.0 - exp(-mat.density * thickness);
    float3 inscatter = mat.jellyColorA * wrapDiff * scatter;

    // fresnel
    float3 V = -rd;
    float3 H = normalize(V - lightDir);
    float F = 0.04 + 0.96 * pow(1.0 - saturate(dot(N, V)), 3.);
    float3 irradiance = g_TexCube[irradianceTex].SampleLevel(LinearClamp, reflect(rd, N), 0).rgb * mat.jellyColorB;

    float3 spec = pow(saturate(dot(N, H)), 128.0) * mat.specColor;

    float3 col = transmitted + inscatter + F * irradiance + spec;

    return float4(col, 1);
}