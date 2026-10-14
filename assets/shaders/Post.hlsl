#include <lib/Core.hlsl>

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
    uint enableFlyEye;
    uint bloomTex;
    float bloomIntensity;
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

float HexagonSDF(float2 p, float r) {
    float x = abs(p.x);
    float y = abs(p.y);
    return max((x * 0.5 + y * 0.866025),x) - r;
}

#define EYES 8
#define FISHEYE_FAC 0.8
float4 frag(Varyings i) : SV_Target {
    float2 t = float2(1.0, 1.0) / resolution;
    float2 pos = i.uv * resolution;
    Texture2D scene = g_Tex2D[sceneColorTex];
    Texture2D normals = g_Tex2D[normalTex];

    float4 source = float4(0, 0, 0, 0);

    if(enableFlyEye == 1) {
        float2 xy = 2.0 * i.uv - 1.0;
        float dist = length(xy);
        float2 uv = i.uv;
        float resultingDist = 0;
        float2 newUv = uv;

        if(dist < (2 - FISHEYE_FAC)) {
            dist = length(xy * FISHEYE_FAC);
            float z = sqrt(1.0 - dist * dist);
            float r = atan2(dist, z) / PI;
            float phi = atan2(xy.y, xy.x);

            uv.x = r * cos(phi) + 0.5;
            uv.y = r * sin(phi) + 0.5;
            newUv = frac(uv * EYES);
            float resultingHex = HexagonSDF(newUv * 2 - 1.0, 0.6);
            resultingDist = 1.0 - saturate(resultingHex * 4.0);
        } else {
            uv = i.uv;
            resultingDist = 0.0;
        }
        source = scene.Sample(PointClamp, newUv);
        source = lerp(float4(0.0, 0.01, 0.02, 1.0), source, resultingDist);
    } else {
        source = scene.Sample(PointClamp, i.uv);
    }

    // bloom, added in hdr before tonemapping
    if(bloomIntensity > 0) {
        source.rgb += g_Tex2D[bloomTex].SampleLevel(LinearClamp, i.uv, 0).rgb * bloomIntensity;
    }

//    source.rgb = g_Tex2D[bloomTex].SampleLevel(LinearClamp, i.uv, 0).rgb;
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