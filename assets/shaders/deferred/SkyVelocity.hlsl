#include "../lib/Core.hlsl"
#include "../lib/GBuffer.hlsl"

cbuffer Pass : register(b2) {
    uint depthTex;
}

struct Varyings {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD;
};

Varyings vert(uint id : SV_VertexID) {
    Varyings o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = uv;
    return o;
}

float2 frag(Varyings i) : SV_Target {
    if (g_Tex2D[depthTex].Load(int3(i.pos.xy, 0)).r != 0)
        discard;
    float4 w = float4(WorldPosFromDepth(i.uv, 0), 1);
    // unjittered
    return GetMotionVector(mul(viewProjNj, w), mul(prevViewProj, w));
}