#include "../lib/Core.hlsl"

cbuffer TransparentPass : register(b2) {
    uint depthTex;
    uint sceneTex;
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
    float3 normalW : NORMAL;
    float2 uv : TEXCOORD;
};

Varyings vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    Varyings o;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.uv = i.uv;
    return o;
}

float4 frag(Varyings i): SV_Target {
    Texture2D scene = g_Tex2D[sceneTex];

    uint2 pos = uint2(i.pos.xy);
    float3 rd = normalize(WorldPosFromDepth(i.pos.xy, 1.0) - cameraPos);
    float depth = g_Tex2D[depthTex].Load(int3(pos, 0)).r;
    return float4(depth, 0, 0, 1);

    float transmittance = 1.0;
    uint idx = 0;
    while(idx <= 100) {

        idx += 1;
    }

    return float4(rd.xyz, 1);
}