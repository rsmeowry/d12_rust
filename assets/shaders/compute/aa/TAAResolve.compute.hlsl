cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float4x4 viewProjNj;
    float4x4 invViewProj;
    float4x4 prevViewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 cameraPos;
    float _pad2;
};

cbuffer DownsampleProperties : register(b2) {
    uint prevTex;
    uint currentTex;
    uint depthTex;
    uint velocityTex;
    uint outTex;
}

RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
Texture2D g_Tex2D[1024] : register(t0, space1);

SamplerState Linear : register(s1);

float3 WorldPosFromDepth(float2 uv, float depth) {
    float4 clip = float4(uv.x * 2 - 1, 1 - uv.y * 2, depth, 1);
    float4 w = mul(invViewProj, clip);
    return w.xyz / w.w;
}

[numthreads(32, 32, 1)]
void CS(int3 threadId : SV_DispatchThreadID) {
    RWTexture2D<float4> o = g_RWTex2D[outTex];
    Texture2D prev = g_Tex2D[prevTex];
    Texture2D current = g_Tex2D[currentTex];
    Texture2D depth = g_Tex2D[depthTex];
    Texture2D velocity = g_Tex2D[velocityTex];

    float4 currCol = current.Load(threadId);
    float currDepth = depth.Load(threadId).r;
    float2 currVelocity = velocity.Load(threadId).rg;

    #define MV_REP

    // backprojected
    float2 uv = float2(threadId.xy) / resolution;
    #ifndef MV_REP
    float3 currWp = WorldPosFromDepth(uv, currDepth);
    // reprojected
    float4 rpClip = mul(prevViewProj, float4(currWp, 1.0));
    float2 rpNdc = (rpClip.xy / rpClip.w);
    float2 rpUv = float2(rpNdc.x * 0.5 + 0.5, 0.5 - rpNdc.y * 0.5);
    #else
    float2 rpUv = uv + currVelocity;
    if (currDepth == 0) { // sky still needs reprojection
        float4 c = mul(prevViewProj, float4(WorldPosFromDepth(uv, 0), 1));
        rpUv = c.xy / c.w * float2(0.5, -0.5) + 0.5;
    }
    #endif

    if(any(rpUv < 0.) || any(rpUv >= 1.)) {
        o[threadId.xy] = currCol;
        return;
    }
    int2 rpPix = round(rpUv * resolution);
    float4 prevCol = prev.Load(int3(rpPix, 0));

    o[threadId.xy] = float4(prevCol.rgb * 0.9 + currCol.xyz * 0.1, 1);
}