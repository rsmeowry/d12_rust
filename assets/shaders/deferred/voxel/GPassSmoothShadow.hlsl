#include "../../lib/Core.hlsl"

struct SmoothVoxelObjectData {
    float4x4 model;
    float4x4 prevModel;
    int3 chunkPos;
    uint _pad;
};

cbuffer ShadowPass : register(b2) {
    float4x4 lightViewProj;
}

cbuffer DrawConstants : register(b0) { uint drawIndex; }

struct Attributes {
    uint4 posMaterial : POSITION_MATERIAL;
    float2 normalOct : NORMAL;
};

float4 vert(Attributes i): SV_Position {
    SmoothVoxelObjectData obj = SmoothVoxelObjectData(g_Objects[drawIndex]);
    float3 posPm = float3(i.posMaterial.xyz) / 1024.;
    float4 posW = mul(obj.model, float4(posPm, 1.0));
    return mul(lightViewProj, posW);
}
