#include <lib/Core.hlsl>

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint3 _pad;
};

struct PhongTessMaterial {
    uint albedoTex;
    uint normalTex;
    uint _texPad[6];

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
StructuredBuffer<PhongTessMaterial> g_Materials : register(t2);
Texture2D g_Tex2D[1024] : register(t0, space1);
SamplerState AnisoWrap : register(s3);
SamplerState Linear : register(s2);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float3 tangent : TANGENT;
    float3 binormal  : BINORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
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
    o.tangentW = mul((float3x3) obj.model, i.tangent);
    o.binormalW = mul((float3x3) obj.model, i.binormal);
    o.uv = i.uv;
    return o;
}

GBufferOut frag(Varyings i) {
    PhongTessMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    float4 tex = g_Tex2D[mat.albedoTex].Sample(AnisoWrap, i.uv);
    clip(tex.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float3 normalTex = g_Tex2D[mat.normalTex].Sample(Linear, i.uv).xyz * 2.0 - 1.0;
    float3 normal = normalize(mul(normalTex, tbnMat));

    GBufferOut o;
    o.albedo = float4(tex.rgb * mat.diffuseColor * mat.diffuseK, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(mat.specularK, saturate(mat.shininess / 255.0), 0, 0);
    o.emissive = 0;
    return o;
}

void hull() {

}

void domain() {

}