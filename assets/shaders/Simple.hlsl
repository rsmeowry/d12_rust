#include <Core.hlsl>

cbuffer SimpleObj : register(b0) {
    float4x4 model;
}

struct Attributes {
    float3 pos: POSITION;
    float4 col: COLOR;
};

struct Varyings {
    float4 pos: SV_Position;
    float4 col: COLOR;
};

Varyings vert(Attributes attr) {
    Varyings v;

    v.pos = mul(viewProj, mul(model, float4(attr.pos, 1.)));
    v.col = attr.col;
    return v;
}

float4 frag(Varyings v) : SV_Target {
    return v.col;
}
