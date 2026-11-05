#include "../../lib/Core.hlsl"

cbuffer ShadowPass : register(b2) {
    float4x4 lightViewProj;
}

cbuffer DrawConstants : register(b0) { uint drawIndex; }

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 uv : TEXCOORD;
    uint2 materialAo : MATERIAL_AO;
};

float4 vert(Attributes i) : SV_Position {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    return mul(lightViewProj, posW);
}
