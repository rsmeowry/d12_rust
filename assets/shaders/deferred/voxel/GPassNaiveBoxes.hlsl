#include "../../lib/Core.hlsl"
#include "../../lib/GBuffer.hlsl"

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
SamplerState PointClamp : register(s2);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : TEXCOORD;
    uint2 materialAo : MATERIAL_AO;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 posW : POSITION;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
    float ao : AO;
    float4 currClip : CURR_CLIP;
    float4 prevClip : PREV_CLIP;
    nointerpolation uint material : MATERIAL;
};

Varyings vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    Varyings o;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.posW = mul((float3x3) obj.model, i.pos);
    o.tangentW = mul((float3x3) obj.model, i.tangent);
    // cube UVs have v=0 at the bottom, D3D samples v=0 from the top row: flip v, and the binormal with it
    float3 binormal = -cross(i.normal, i.tangent.xyz) * i.tangent.w;
    o.binormalW = mul((float3x3) obj.model, binormal);
    o.uv = float2(i.uv.x, 1.0 - i.uv.y);
    o.material = i.materialAo.x;
    o.ao = i.materialAo.y / 3.;
    o.currClip = mul(viewProjNj, posW);
    o.prevClip = mul(prevViewProj, mul(obj.prevModel, float4(i.pos, 1.0)));
    return o;
}

GBufferOut frag(Varyings i) {
    VoxelMaterial mat = g_Materials[i.material];
    float4 albedo = g_Tex2D[mat.albedoTex].SampleBias(PointClamp, i.uv, mipBias);
    clip(albedo.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float4 texNormal = g_Tex2D[mat.normalTex].SampleBias(PointClamp, i.uv, mipBias) * 2.0 - 1.0;
    float3 normal = normalize(mul(texNormal.xyz, tbnMat));

    float metallic = g_Tex2D[mat.metalTex].SampleBias(PointClamp, i.uv, mipBias).r;
    float roughness = g_Tex2D[mat.roughnessTex].SampleBias(PointClamp, i.uv, mipBias).r;
    float occlusion = g_Tex2D[mat.aoTex].SampleBias(PointClamp, i.uv, mipBias).r;
    float aoVal = lerp(0.35, 1.0, i.ao);

    GBufferOut o;
    o.albedo = float4(albedo.rgb * aoVal, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(roughness * mat.roughness, metallic * mat.metallic, occlusion * aoVal, 0);
    o.emissive = 0;
    o.velocity = GetMotionVector(i.currClip, i.prevClip);
    return o;
}