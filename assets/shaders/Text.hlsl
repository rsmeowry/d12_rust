#include <./lib/Core.hlsl>

cbuffer DrawConstants : register(b0) { uint drawIndex; } // font texture idx
SamplerState PointClamp : register(s2);

struct Glyph {
    float2 pos;
    float scale;
    uint code;
    float4 col;
};

StructuredBuffer<Glyph> g_Glyphs : register(t1);

static const float GLYPH_COUNT = 128.0;
static const float2 CORNERS[6] = {
    float2(0, 0), float2(1, 0), float2(0, 1),
    float2(0, 1), float2(1, 0), float2(1, 1)
};

struct Varyings { float4 pos : SV_Position; float2 uv : TEXCOORD; float4 color: COLOR; };

// generating a fake quad mesh here
Varyings vert(uint vid : SV_VertexID, uint iid: SV_InstanceID) {
    Glyph g = g_Glyphs[iid];
    float2 c = CORNERS[vid];
    float2 px = g.pos + c * 8.0 * g.scale;
    Varyings o;
    o.pos = float4(px / resolution * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = float2((g.code + c.x) / GLYPH_COUNT, c.y);
    o.color = g.col;
    return o;
}

float4 frag(Varyings i) : SV_Target {
    float4 texCol = g_Tex2D[drawIndex].Sample(PointClamp, i.uv);
    clip(texCol.r - 0.5);
    return texCol * i.color;
}
