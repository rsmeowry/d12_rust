#include "../../lib/Core.hlsl"

struct ObjectData {
    float4x4 model;
    int3 chunkPos;
    uint _pad;
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects : register(t1);

struct Attributes {
    uint4 posMaterial : POSITION_MATERIAL;
    float2 normalOct : NORMAL;
};

float4 vert(Attributes i): SV_Position {
    ObjectData obj = g_Objects[drawIndex];
    float3 posPm = float3(i.posMaterial.xyz) / 1024.;
    float4 posW = mul(obj.model, float4(posPm, 1.0));
    return mul(viewProj, posW);
}
