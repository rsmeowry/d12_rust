#include <../lib/Core.hlsl>

cbuffer DrawConstants : register(b0) { uint drawIndex; }
SamplerState LinearClamp : register(s1);

struct V {
    float2 pos;
    float2 uv;
    uint col;
};
StructuredBuffer<V> g_Verts : register(t1);

struct Varyings {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD;
    float4 color : COLOR;
};

float3 SrgbToLinear(float3 c) {
    if(any(c <= 0.04045)) {
        return c / 12.92;
    } else {
        return pow((c + 0.055) / 1.055, 2.4);
    }
}

Varyings vert(uint vid : SV_VertexID) {
    V v = g_Verts[vid];

    Varyings o;
    o.pos = float4(v.pos / resolution * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = v.uv;
    float4 c = float4(v.col & 0xFF, (v.col >> 8) & 0xFF, (v.col >> 16) & 0xFF, v.col >> 24) / 255.0;
    o.color = float4(SrgbToLinear(c.rgb), c.a);
    return o;
}

float4 frag(Varyings i) : SV_Target {
    return g_Tex2D[drawIndex].Sample(LinearClamp, i.uv) * i.color;
}