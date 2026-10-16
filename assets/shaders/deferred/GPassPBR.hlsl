#include "../lib/Core.hlsl"

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint3 _pad;
};

struct PbrMaterial {
    uint albedoTex;
    uint normalTex;
    uint metalRoughnessTex;
    uint _texPad[5];

    float3 baseColor;
    float metallic;
    float roughness;
    uint _pad[11];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects : register(t1);
StructuredBuffer<PbrMaterial> g_Materials : register(t2);
SamplerState AnisoWrap : register(s3);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float3 tangent : TANGENT;
    float3 binormal : BINORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 posW : POSITION;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
};

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

Varyings vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    Varyings o;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.posW = mul((float3x3) obj.model, i.pos);
    o.tangentW = mul((float3x3) obj.model, i.tangent);
    o.binormalW = mul((float3x3) obj.model, i.binormal);
    o.uv = i.uv;
    return o;
}

GBufferOut frag(Varyings i) {
    PbrMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    float4 albedo = g_Tex2D[mat.albedoTex].Sample(AnisoWrap, i.uv);
    clip(albedo.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float4 texNormal = g_Tex2D[mat.normalTex].Sample(AnisoWrap, i.uv) * 2.0 - 1.0;
    float3 normal = normalize(mul(texNormal.xyz, tbnMat));

    float4 roughness = g_Tex2D[mat.metalRoughnessTex].Sample(AnisoWrap, i.uv);

    GBufferOut o;
    o.albedo = float4(albedo.rgb * mat.baseColor, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(roughness.x * mat.roughness, mat.metallic, 0, 0);
    o.emissive = 0;
    return o;
}