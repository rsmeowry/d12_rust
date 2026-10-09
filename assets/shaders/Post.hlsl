#include <lib/Core.hlsl>

Texture2D g_Tex2D[1024] : register(t0, space1);
SamplerState PointClamp : register(s2);
SamplerState LinearClamp : register(s1);
cbuffer PostProperties : register(b2) {
    uint albedoTex;
    uint normalTex;
    uint materialTex;
    uint depthTex;
    uint sceneColorTex;
    uint enableSobel;
    uint enableVignette;
    float vignetteStrength;
    uint enablePosterization;
    uint posterizationLevels;
};

struct Varyings { float4 pos : SV_Position; float2 uv : TEXCOORD; };

// generating a fake tri mesh here
Varyings vert(uint id : SV_VertexID) {
    Varyings o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = uv;
    return o;
}

int2 ClampPx(int2 p) {
    return clamp(p, int2(0, 0), int2(resolution) - 1);
}

float3 NormalAt(int2 p) {
    return g_Tex2D[normalTex].Load(int3(ClampPx(p), 0)).xyz * 2 - 1;
}

float4 frag(Varyings i) : SV_Target {
    float2 t = float2(1.0, 1.0) / resolution;
    float2 pos = i.uv * resolution;
    Texture2D scene = g_Tex2D[sceneColorTex];
    Texture2D normals = g_Tex2D[normalTex];
    float4 source = scene.Sample(PointClamp, i.uv);

    // hdr to ldr
    source = source / (source + 1);

    // vignette
    if(enableVignette == 1) {
        source = source * pow(1 - distance(i.uv, float2(0.5, 0.5)), vignetteStrength);
    }

    // posterization
    if(enablePosterization == 1) {
        float level = posterizationLevels;
        source = floor(source * level) / level;
    }

    // sobel
    if(enableSobel == 1) {
        float3 tl = NormalAt(pos + int2(-1, -1));
        float3 tc = NormalAt(pos + int2(0, -1));
        float3 tr = NormalAt(pos + int2(1, -1));
        float3 ml = NormalAt(pos + int2(-1, 0));
        float3 mr = NormalAt(pos + int2(1, 0));
        float3 bl = NormalAt(pos + int2(-1, 1));
        float3 bc = NormalAt(pos + int2(0, 1));
        float3 br = NormalAt(pos + int2(1, 1));

        float3 gx = (tr + 2 * mr + br) - (tl + 2 * ml + bl);
        float3 gy = (bl + 2 * bc + br) - (tl + 2 * tc + tr);

        float edge = smoothstep(0.4, 1.0, sqrt(dot(gx, gx) + dot(gy, gy)));

        source = lerp(source, 0., edge);
    }
    return source;
}