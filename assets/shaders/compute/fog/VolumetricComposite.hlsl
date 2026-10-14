#include <../../lib/Core.hlsl>

cbuffer FogComposite : register(b2) {
    uint fogTex;
    uint depthTex;
    uint downscale;
    uint _pad;
    uint2 fogSize;
}

SamplerState LinearClamp : register(s1);

#define EDGE_THRESHOLD 0.1

float LinearDepth(float d) {
    return 1.0 / max(d, 1e-7);
}

float4 vert(uint id : SV_VertexID) : SV_Position {
    float2 uv = float2((id << 1) & 2, id & 2);
    return float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
}

float4 frag(float4 pos : SV_Position) : SV_Target {
    float center = LinearDepth(g_Tex2D[depthTex].Load(int3(pos.xy, 0)).r);

    float2 f = (pos.xy - 0.5) / downscale;
    int2 base = int2(floor(f));
    int2 best = clamp(base, 0, int2(fogSize) - 1);
    float bestDiff = 1e30;
    float maxDiff = 0;

    [unroll]
    for (int i = 0; i < 4; i++) {
        int2 texel = clamp(base + int2(i & 1, i >> 1), 0, int2(fogSize) - 1);
        float d = LinearDepth(g_Tex2D[depthTex].Load(int3(texel * downscale, 0)).r);
        float diff = abs(d - center) / center;
        maxDiff = max(maxDiff, diff);
        if (diff < bestDiff) {
            bestDiff = diff;
            best = texel;
        }
    }

    float4 fog;
    if (maxDiff < EDGE_THRESHOLD) {
        fog = g_Tex2D[fogTex].SampleLevel(LinearClamp, (f + 0.5) / fogSize, 0);
    } else {
        fog = g_Tex2D[fogTex].Load(int3(best, 0));
    }
    return float4(fog.rgb, 1.0 - fog.a);
}
