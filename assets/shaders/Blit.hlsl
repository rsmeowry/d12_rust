#include "lib/Core.hlsl"

SamplerState PointClamp : register(s2);
cbuffer BlitProperties : register(b2) {
    uint sourceTex;
    uint transformHdr;
}

struct Varyings { float4 pos : SV_Position; float2 uv : TEXCOORD; };

// generating a fake tri mesh here
Varyings vert(uint id : SV_VertexID) {
    Varyings o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = uv;
    return o;
}

#define DITHER_STRENGTH 0.01

float Bayer4x4(float2 p){
    int x = int(p.x % 4.0);
    int y = int(p.y % 4.0);
    int m[16] = {
        0, 8, 2, 10,
        12, 4, 14, 6,
        3, 11, 1, 9,
        15, 7, 13, 5
    };
    return (float(m[y*4 + x]) + 0.5) / 16.0;
}

float4 frag(Varyings i) : SV_Target {
    float2 pos = i.uv * resolution;
    float dither = Bayer4x4(pos);
    float4 scene = g_Tex2D[sourceTex].Sample(PointClamp, i.uv);
    if(transformHdr == 1) {
        scene = scene / (scene + 1.0);
    }
    return scene + (dither - 0.5) * DITHER_STRENGTH;
}

