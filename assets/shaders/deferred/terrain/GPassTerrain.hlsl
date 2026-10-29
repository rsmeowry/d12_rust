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

struct Varyings {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD;
};

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

float LoadHeight(int2 t) {
    t = clamp(t, 0, int(worldTexels) - 1);
    return g_Tex2D[heightMapTex].Load(int3(t, 0)).r * heightScale;
}

Varyings vert(Attributes i, uint id : SV_InstanceID) {
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

    Varyings o;
    o.pos = mul(viewProj, float4(xz.x * texelSize, y, xz.y * texelSize, 1.0));
    o.uv = (xz + 0.5) / worldTexels;
    return o;
}

GBufferOut frag(Varyings i) {
    float3 albedo = g_Tex2D[albedoTex].Sample(AnisoWrap, i.uv);
    float3 norm = (g_Tex2D[normalTex].Sample(AnisoWrap, i.uv).rgb * 2 - 1.).xzy;
    float roughness = g_Tex2D[roughnessTex].Sample(AnisoWrap, i.uv).r;
    float ao = g_Tex2D[aoTex].Sample(AnisoWrap, i.uv).r;
    norm = normalize(norm);


    GBufferOut o;
    o.albedo = float4(albedo, 1);
    o.normal = float4(norm * 0.5 + 0.5, 0);
    o.material = float4(roughness, 0., ao, 0);
    o.emissive = 0;
    return o;
}