#include "../../lib/Core.hlsl"

cbuffer RTBlur : register(b2) {
    int2 axis;
    uint inTex;
    uint radiusTex;
    uint rawTex;
    uint outTex;
    float depthTol;
    uint _pad;
}

#define MAX_TAPS 8

// https://blog.imaginationtech.com/implementing-fast-ray-traced-soft-shadows-in-a-game-engine/
[numthreads(8, 8, 1)]
void CS(uint3 id : SV_DispatchThreadID) {
    int2 px = int2(id.xy);
    float center = g_Tex2D[inTex].Load(int3(px, 0)).x;
    float r = g_Tex2D[radiusTex].Load(int3(px, 0)).x;
    if (r < 0.5) {
        g_RWTex2D[outTex][px] = center;
        return;
    }

    float d0 = g_Tex2D[rawTex].Load(int3(px, 0)).z;
    float dP = g_Tex2D[rawTex].Load(int3(px + axis, 0)).z;
    float dM = g_Tex2D[rawTex].Load(int3(px - axis, 0)).z;
    float dd = abs(dP - d0) < abs(d0 - dM) ? dP - d0 : d0 - dM;

    int n = min(MAX_TAPS, (int) ceil(r));
    float stride = r / n;
    float sigma = r * 0.5;
    float sum = center, wsum = 1;

    for (int i = -n; i <= n; i++) {
        if (i == 0) continue;
        int k = (int) round(i * stride);
        int2 p = px + axis * k;
        float di = g_Tex2D[rawTex].Load(int3(p, 0)).z;
        if (di <= 0 || abs(di - (d0 + dd * k)) > depthTol * d0)
            continue;
        float w = exp(-(k * k) / (2 * sigma * sigma));
        sum += g_Tex2D[inTex].Load(int3(p, 0)).x * w;
        wsum += w;
    }
    g_RWTex2D[outTex][px] = sum / wsum;
}
