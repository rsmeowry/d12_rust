#include "../lib/Core.hlsl"
#include "../lib/GBuffer.hlsl"

struct PbrMaterial {
    uint albedoTex;
    uint normalTex;
    uint metalRoughnessTex;
    uint occlusionTex;
    uint _texPad[4];

    float3 baseColor;
    float metallic;
    float roughness;
    uint _pad[11];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<PbrMaterial> g_Materials : register(t2);
SamplerState AnisoWrap : register(s3);

#include "../lib/GPassPBRCore.hlsl"

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float3 tangent : TANGENT;
    float3 binormal : BINORMAL;
    float2 uv : TEXCOORD;
};

GPBRVaryings vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    GPBRVaryings o;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.currClip = mul(viewProjNj, posW);
    o.prevClip = mul(prevViewProj, mul(obj.prevModel, float4(i.pos, 1.0)));
    o.tangentW = mul((float3x3) obj.model, i.tangent);
    o.binormalW = mul((float3x3) obj.model, i.binormal);
    o.uv = i.uv;
    return o;
}
