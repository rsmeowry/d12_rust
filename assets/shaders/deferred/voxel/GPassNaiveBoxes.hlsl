#include "../../lib/Core.hlsl"

struct ObjectData {
    float4x4 model;
    uint4 _pad;
};

struct VoxelMaterial {
    uint albedoTex;
    uint normalTex;
    uint metalTex;
    uint aoTex;
    uint _texPad[4];

    float metallic;
    float roughness;
    uint _pad[14];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects : register(t1);
StructuredBuffer<VoxelMaterial> g_Materials : register(t2);
SamplerState PointClamp : register(s2);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : TEXCOORD;
    uint material : MATERIAL;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 posW : POSITION;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
    nointerpolation uint material : MATERIAL;
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
    float3 binormal = cross(i.normal, i.tangent.xyz) * i.tangent.w;
    o.binormalW = mul((float3x3) obj.model, binormal);
    o.uv = i.uv;
    o.material = i.material;
    return o;
}

GBufferOut frag(Varyings i) {
    VoxelMaterial mat = g_Materials[i.material];
    float4 albedo = g_Tex2D[mat.albedoTex].Sample(PointClamp, i.uv);
    clip(albedo.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float4 texNormal = g_Tex2D[mat.normalTex].Sample(PointClamp, i.uv) * 2.0 - 1.0;
    float3 normal = normalize(mul(texNormal.xyz, tbnMat));

    float4 roughnessMetallic = g_Tex2D[mat.metalTex].Sample(PointClamp, i.uv);
    float4 occlusion = g_Tex2D[mat.aoTex].Sample(PointClamp, i.uv);

    GBufferOut o;
    o.albedo = float4(albedo.rgb, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(mat.roughness, roughnessMetallic.r * mat.metallic, 1., 0);
    o.emissive = 0;
    return o;
}