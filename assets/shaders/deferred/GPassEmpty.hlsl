#include "../lib/Core.hlsl"
#include "../lib/GBuffer.hlsl"

struct PhongMaterial {
    uint albedoTex;
    uint _texPad[7];

    float3 ambientColor;
    float3 diffuseColor;
    float3 specularColor;
    float shininess;
    float diffuseK;
    float specularK;
    float4 _pad;
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<PhongMaterial> g_Materials : register(t2);
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

GBufferOut frag(Varyings i) {
    clip(-1);

    GBufferOut o;
    return o;
}