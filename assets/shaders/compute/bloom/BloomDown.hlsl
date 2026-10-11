cbuffer DownsampleProperties : register(b2) {
    uint2 targetSize;
    float2 sourceTexel;
    float threshold;
    float knee;
    uint isFirstPass;
    float4 tint;

    uint sceneTex; // in
    uint outTex; // out
}

RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
Texture2D g_Tex2D[1024] : register(t0, space1);

SamplerState Linear : register(s1);

float3 Fetch(Texture2D src, float2 uv, float2 offset) {
    return src.SampleLevel(Linear, uv + offset * sourceTexel, 0).rgb;
}

float Luma(float3 c) {
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}
float KarisWeight(float3 c) {
    return 1.0 / (1.0 + Luma(c));
}

[numthreads(16, 16, 1)]
void CS(int3 threadId : SV_DispatchThreadID) {
    RWTexture2D<float4> o = g_RWTex2D[outTex];
    Texture2D src = g_Tex2D[sceneTex];

    if (any(threadId.xy >= targetSize)) {
        return;
    }
    float2 uv = (threadId.xy + 0.5) / targetSize;

    float3 A = Fetch(src, uv, float2(-2, 2));
    float3 B = Fetch(src, uv, float2(0, 2));
    float3 C = Fetch(src, uv, float2(2, 2));
    float3 D = Fetch(src, uv, float2(-1, 1));
    float3 E = Fetch(src, uv, float2(1, 1));
    float3 F = Fetch(src, uv, float2(-2, 0));
    float3 G = Fetch(src, uv, float2(0, 0));
    float3 H = Fetch(src, uv, float2(2, 0));
    float3 I = Fetch(src, uv, float2(-1,-1));
    float3 J = Fetch(src, uv, float2(1,-1));
    float3 K = Fetch(src, uv, float2(-2,-2));
    float3 L = Fetch(src, uv, float2(0,-2));
    float3 M = Fetch(src, uv, float2(2,-2));

    float3 res;

    if(isFirstPass == 1) {
        float3 g0 = (D + E + I + J) * 0.25;
        float3 g1 = (A + B + F + G) * 0.25;
        float3 g2 = (B + C + G + H) * 0.25;
        float3 g3 = (F + G + K + L) * 0.25;
        float3 g4 = (G + H + L + M) * 0.25;
        float w0 = KarisWeight(g0) * 0.5;
        float w1 = KarisWeight(g1) * 0.125;
        float w2 = KarisWeight(g2) * 0.125;
        float w3 = KarisWeight(g3) * 0.125;
        float w4 = KarisWeight(g4) * 0.125;
        res = (g0*w0 + g1*w1 + g2*w2 + g3*w3 + g4*w4) / (w0+w1+w2+w3+w4);

        // extract brightness basically
        float w = saturate((Luma(res) - threshold) / max(knee, 1e-4));
        res *= w * tint.rgb;
    } else {
        res = (D + E + I + J) * 0.125
                + (A + B + F + G) * 0.03125
                + (B + C + G + H) * 0.03125
                + (F + G + K + L) * 0.03125
                + (G + H + L + M) * 0.03125;
    }

    o[threadId.xy] = float4(res, 1);
}