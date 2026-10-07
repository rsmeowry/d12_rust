#include <lib/Core.hlsl>

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint _pad[3];
};

struct Material {
    uint albedoTex;
    uint _texPad[7];

    float4 color;
    float scrollSpeed;

    float _pad[11];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects : register(t1);
StructuredBuffer<Material> g_Materials : register(t2);
SamplerState AnisoWrap : register(s3);
Texture2D g_Tex2D[1024] : register(t0, space1);

struct Attributes {
    float3 pos: POSITION;
    float2 uv: TEXCOORD;
};

struct Varyings {
    float4 pos: SV_Position;
    float2 uv: TEXCOORD;
};

Varyings vert(Attributes i) {
    Varyings vout;
    ObjectData obj = g_Objects[drawIndex];

    float4 posW = mul(obj.model, float4(i.pos, 1.0));

    vout.pos = mul(viewProj, posW);
    vout.uv = i.uv;

    return vout;
}

float4 frag(Varyings i) : SV_Target {
    ObjectData obj = g_Objects[drawIndex];
    Material mat = g_Materials[obj.materialId];

    float4 albedo = g_Tex2D[mat.albedoTex].Sample(AnisoWrap, i.uv + float2(0., time * mat.scrollSpeed));
    float4 result = albedo * mat.color;

    return result;
}