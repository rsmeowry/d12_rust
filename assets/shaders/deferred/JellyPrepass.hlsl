#include "../lib/Core.hlsl"
#define MAX_DISTANCE 15.

cbuffer DrawConstants : register(b0) { uint drawIndex; }
SamplerState AnisoWrap : register(s3);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 worldPos : TEXCOORD0;
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

Varyings vert(Attributes v) {
    ObjectData obj = g_Objects[drawIndex];
    Varyings o;
    g_Rng = asuint(v.normal.x);

    float3 rnd = normalize(float3(Rand(), Rand(), Rand()));
    float3 disp = DISP_AMP * sin(DISP_FREQ * time + dot(v.pos, DISP_DIR) * DISP_PHASE);

    float4 posW = mul(obj.model, float4(v.pos + disp * normalize(v.normal * rnd), 1.0));
    o.worldPos = posW.xyz;
    o.pos = mul(viewProj, posW);
    return o;
}

float frag(Varyings i) : SV_Target {
    return distance(i.worldPos, cameraPos);
}