#include "../lib/Core.hlsl"

cbuffer BakePass : register(b2) {
    float4x4 bakeViewProj;
}

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint3 _pad;
};

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
StructuredBuffer<ObjectData> g_Objects : register(t1);
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

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

Varyings vertBake(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    Varyings o;
    o.pos = mul(bakeViewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.uv = i.uv;
    return o;
}

GBufferOut frag(Varyings i) {
    PhongMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    float4 tex = g_Tex2D[mat.albedoTex].Sample(AnisoWrap, i.uv);
    clip(tex.a - 0.5);

    GBufferOut o;
    o.albedo = float4(tex.rgb * mat.diffuseColor * mat.diffuseK, 1);
    o.normal = float4(normalize(i.normalW) * 0.5 + 0.5, 0);
    o.material = float4(mat.specularK, saturate(mat.shininess / 255.0), 0, 0);
    o.emissive = 0;
    return o;
}