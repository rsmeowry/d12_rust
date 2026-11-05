cbuffer DownsampleProperties : register(b2) {
    uint prevTex;
    uint currentTex;
    uint outTex;
}

RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
Texture2D g_Tex2D[1024] : register(t0, space1);

SamplerState Linear : register(s1);

[numthreads(32, 32, 1)]
void CS(int3 threadId : SV_DispatchThreadID) {
    RWTexture2D<float4> o = g_RWTex2D[outTex];
    Texture2D prev = g_Tex2D[prevTex];
    Texture2D current = g_Tex2D[currentTex];

    float4 prevCol = prev.Load(threadId);
    float4 currCol = current.Load(threadId);

    o[threadId.xy] = float4(prevCol.rgb * 0.9 + currCol.xyz * 0.1, 1);
}