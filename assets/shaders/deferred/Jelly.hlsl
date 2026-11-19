#include "../lib/Core.hlsl"

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

cbuffer DrawConstants : register(b0) { uint drawIndex; }
SamplerState AnisoWrap : register(s3);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 posW : WORLD_POS;
    float3 normalW : NORMAL;
    float2 uv : TEXCOORD;
};

#define DISP_AMP 0.03
#define DISP_K 5
#define DISP_FREQ 1.2
#define DISP_PHASE 6.
#define DISP_DIR float3(0, 1, 0)

static uint g_Rng;

float Rand() {
    g_Rng = PcgHash(g_Rng);
    return (g_Rng >> 8) * (1.0 / 16777216.0);
}

Varyings vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    g_Rng = asuint(i.normal.x);

    float3 rnd = normalize(float3(Rand(), Rand(), Rand()));
    float3 disp = DISP_AMP * sin(DISP_FREQ * time + dot(i.pos, DISP_DIR) * DISP_PHASE);

    float4 posW = mul(obj.model, float4(i.pos + disp * normalize(i.normal * rnd), 1.0));
    Varyings o;
    o.posW = posW.xyz;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.uv = i.uv;
    return o;
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