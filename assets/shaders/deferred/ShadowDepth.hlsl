#include "../lib/Core.hlsl"
cbuffer DrawConstants : register(b0) {
    uint drawIndex;
}
cbuffer ShadowPass : register(b2) {
    float4x4 lightViewProj;
}
struct ObjectData {
    float4x4 model;
    uint materialId; uint3 _pad;
};
StructuredBuffer<ObjectData> g_Objects : register(t1);

float4 vert(float3 p : POSITION, float3 n : NORMAL, float2 uv : TEXCOORD) : SV_Position {
    return mul(lightViewProj, mul(g_Objects[drawIndex].model, float4(p, 1)));
}