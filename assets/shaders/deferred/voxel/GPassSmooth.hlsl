#include "../../lib/Core.hlsl"
#include "../../lib/GBuffer.hlsl"

struct SmoothVoxelObjectData {
    float4x4 model;
    float4x4 prevModel;
    int3 chunkPos;
    uint _pad;
};

struct VoxelMaterial {
    uint albedoTex;
    uint normalTex;
    uint metalTex;
    uint aoTex;
    uint roughnessTex;
    uint3 _texPad;

    float metallic;
    float roughness;
    uint _pad[14];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<VoxelMaterial> g_Materials : register(t2);
SamplerState PointWrap : register(s5);

struct Attributes {
    uint4 posMaterial : POSITION_MATERIAL;
    float2 normalOct : NORMAL;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 posW : POSITION;
    float3 normalW : NORMAL;
    nointerpolation uint material : MATERIAL;
};

Varyings vert(Attributes i) {
    SmoothVoxelObjectData obj = SmoothVoxelObjectData(g_Objects[drawIndex]);
    float3 posPm = float3(i.posMaterial.xyz) / 1024.;
    float4 posW = mul(obj.model, float4(posPm, 1.0));
    Varyings o;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, DecodeOctNormals(i.normalOct));
    o.posW = mul((float3x3) obj.model, posPm);
    o.material = i.posMaterial.w;
    return o;
}

GBufferOut frag(Varyings i) {
    VoxelMaterial mat = g_Materials[i.material];
    float4 albedo = TriplanarSample(g_Tex2D[mat.albedoTex], PointWrap, i.posW, i.normalW, 1.);

//    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float3 texNormal = TriplanarNormal(g_Tex2D[mat.normalTex], PointWrap, i.posW, i.normalW, 1.);
//    float3 normal = normalize(mul(texNormal.xyz, tbnMat));

    float metallic = TriplanarSample(g_Tex2D[mat.metalTex], PointWrap, i.posW, i.normalW, 1.).r;
    float roughness = TriplanarSample(g_Tex2D[mat.roughnessTex], PointWrap, i.posW, i.normalW, 1.).r;
    float occlusion = TriplanarSample(g_Tex2D[mat.aoTex], PointWrap, i.posW, i.normalW, 1.).r;

    GBufferOut o;
    o.albedo = float4(albedo.rgb, 1);
    o.normal = float4(texNormal * 0.5 + 0.5, 0);
    o.material = float4(roughness * mat.roughness, metallic * mat.metallic, occlusion, 0);
    o.emissive = 0;
    o.velocity = 0;
    return o;
}