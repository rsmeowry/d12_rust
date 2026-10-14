cbuffer UpsampleProperties : register(b2) {
    uint2 targetSize;
    float2 lowerTexel;
    float radius;

    uint lowerTex; // in
    uint sameTex; // in
    uint outTex; // out
}

RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
Texture2D g_Tex2D[1024] : register(t0, space1);

SamplerState Linear : register(s1);

[numthreads(16, 16, 1)]
void CS(int3 threadId : SV_DispatchThreadID) {
    if (any(threadId.xy >= targetSize)) {
        return;
    }
    float2 uv = (threadId.xy + 0.5) / targetSize;
    float2 d  = lowerTexel * radius;

    Texture2D lower = g_Tex2D[lowerTex];
    Texture2D same = g_Tex2D[sameTex];
    RWTexture2D<float4> o = g_RWTex2D[outTex];

    float3 a = lower.SampleLevel(Linear, uv + d * float2(-1, 1), 0).rgb;
    float3 b = lower.SampleLevel(Linear, uv + d * float2(0, 1), 0).rgb;
    float3 c = lower.SampleLevel(Linear, uv + d * float2(1, 1), 0).rgb;
    float3 e = lower.SampleLevel(Linear, uv + d * float2(-1, 0), 0).rgb;
    float3 f = lower.SampleLevel(Linear, uv, 0).rgb;
    float3 g = lower.SampleLevel(Linear, uv + d * float2(1, 0), 0).rgb;
    float3 h = lower.SampleLevel(Linear, uv + d * float2(-1, -1), 0).rgb;
    float3 i = lower.SampleLevel(Linear, uv + d * float2(0, -1), 0).rgb;
    float3 j = lower.SampleLevel(Linear, uv + d * float2(1, -1), 0).rgb;

    float3 up = (f * 4 + (b + e + g + i) * 2 + (a + c + h + j)) / 16.0;
    float3 cur = same.Load(int3(threadId.xy, 0)).rgb;

    o[threadId.xy] = float4(cur + up, 1);
}