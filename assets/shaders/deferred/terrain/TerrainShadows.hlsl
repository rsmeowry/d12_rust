#include "../../lib/Core.hlsl"

#define MAX_LODS 8

cbuffer Pass : register(b2) {
    uint heightMapTex;
    uint albedoTex;
    uint normalTex;
    uint roughnessTex;
    uint aoTex;
    uint worldTexels;
    float texelSize;
    float heightScale;
    float4 morphs[MAX_LODS];
    float4x4 lightViewProj;
};

struct Patch {
    float2 origin;
    uint lod;
};

StructuredBuffer<Patch> g_Patches : register(t1);
SamplerState AnisoWrap : register(s3);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD;
};

float LoadHeight(int2 t) {
    t = clamp(t, 0, int(worldTexels) - 1);
    return g_Tex2D[heightMapTex].Load(int3(t, 0)).r * heightScale;
}

float4 vert(Attributes i, uint id : SV_InstanceID) : SV_Position {
    Patch patch = g_Patches[id];
    int2 grid = int2(i.pos.xz);
    int stp = 1 << patch.lod; // grid size in texels
    int2 texl = int2(patch.origin) + grid * stp; // vertex texel
    int2 odd = (grid & 1) * stp; // odd vertex fallback to parent grid

    float height = LoadHeight(texl);
    float dist = distance(cameraPos, float3(texl.x * texelSize, height, texl.y * texelSize));
    float k = saturate((dist - morphs[patch.lod].x) / (morphs[patch.lod].y - morphs[patch.lod].x));

    float2 xz = lerp(float2(texl), float2(texl - odd), k); // morphing to parent
    float y = lerp(height, LoadHeight(texl - odd), k); // morphing height

    float4 pos = mul(lightViewProj, float4(xz.x * texelSize, y, xz.y * texelSize, 1.0));
    return pos;
}
