#include <lib/Core.hlsl>

cbuffer DrawConstants : register(b0) { uint drawIndex; } // texture index actually
Texture2D g_Tex2D[1024] : register(t0, space1);
SamplerState PointClamp : register(s2);

struct Varyings { float4 pos : SV_Position; float2 uv : TEXCOORD; };

// generating a fake quad mesh here
Varyings vert(uint id : SV_VertexID) {
    Varyings o;
    float2 uv = float2((id << 1) & 2, id & 2);   // (0,0) (2,0) (0,2)
    o.pos = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = uv;
    return o;
}

float4 frag(Varyings i) : SV_Target {
    return g_Tex2D[drawIndex].Sample(PointClamp, i.uv);
}