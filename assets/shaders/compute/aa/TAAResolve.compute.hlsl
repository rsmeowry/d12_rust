#include "../../lib/Core.hlsl"

cbuffer DownsampleProperties : register(b2) {
    uint prevTex;
    uint currentTex;
    uint depthTex;
    uint velocityTex;
    uint outTex;
}

SamplerState Linear : register(s1);

// https://www.elopezr.com/temporal-aa-and-the-quest-for-the-holy-trail/
float2 GetVelocity(int2 pix)
{
    float closestDepth = 0.0; // cus reversed depth
    int2 closestOffset = 0;
    for(int j = -1; j <= 1; ++j)
    {
        for(int i = -1; i <= 1; ++i)
        {
            int2 uvOffset = int2(i, j);
            float neighborDepth = g_Tex2D[depthTex].Load(int3(pix + uvOffset, 0)).r;
            if(neighborDepth > closestDepth)
            {
                closestOffset = uvOffset;
                closestDepth = neighborDepth;
            }
        }
    }
    return g_Tex2D[velocityTex].Load(int3(pix + closestOffset, 0)).rg;
}

void ClampPrevious(int2 pix, inout float4 previousColor) {
    float3 minCol = 9999.0;
    float3 maxCol = -9999.0;

    for(int x = -1; x <= 1; ++x)
    {
        for(int y = -1; y <= 1; ++y)
        {
            float3 color = g_Tex2D[currentTex].Load(int3(pix + int2(x, y), 0)).rgb;
            minCol = min(minCol, color);
            maxCol = max(maxCol, color);
        }
    }

    previousColor = clamp(previousColor, float4(minCol, 1.), float4(maxCol, 1.));
}

// https://gist.github.com/TheRealMJP/c83b8c0f46b63f3a88a5986f4fa982b1
float4 SampleTextureCatmullRom(in Texture2D<float4> tex, in SamplerState linearSampler, in float2 uv, in float2 texSize)
{
    // starting texel
    float2 samplePos = uv * texSize;
    float2 texPos1 = floor(samplePos - 0.5f) + 0.5f;

    // fractional offsett
    float2 f = samplePos - texPos1;

    // weights
    float2 w0 = f * (-0.5f + f * (1.0f - 0.5f * f));
    float2 w1 = 1.0f + f * f * (-2.5f + 1.5f * f);
    float2 w2 = f * (0.5f + f * (2.0f - 1.5f * f));
    float2 w3 = f * f * (-0.5f + 0.5f * f);

    // weighting factors
    float2 w12 = w1 + w2;
    float2 offset12 = w2 / (w1 + w2);

    // final uvs
    float2 texPos0 = texPos1 - 1;
    float2 texPos3 = texPos1 + 2;
    float2 texPos12 = texPos1 + offset12;

    texPos0 /= texSize;
    texPos3 /= texSize;
    texPos12 /= texSize;

    float4 result = 0.0f;
    result += tex.SampleLevel(linearSampler, float2(texPos0.x, texPos0.y), 0.0f) * w0.x * w0.y;
    result += tex.SampleLevel(linearSampler, float2(texPos12.x, texPos0.y), 0.0f) * w12.x * w0.y;
    result += tex.SampleLevel(linearSampler, float2(texPos3.x, texPos0.y), 0.0f) * w3.x * w0.y;

    result += tex.SampleLevel(linearSampler, float2(texPos0.x, texPos12.y), 0.0f) * w0.x * w12.y;
    result += tex.SampleLevel(linearSampler, float2(texPos12.x, texPos12.y), 0.0f) * w12.x * w12.y;
    result += tex.SampleLevel(linearSampler, float2(texPos3.x, texPos12.y), 0.0f) * w3.x * w12.y;

    result += tex.SampleLevel(linearSampler, float2(texPos0.x, texPos3.y), 0.0f) * w0.x * w3.y;
    result += tex.SampleLevel(linearSampler, float2(texPos12.x, texPos3.y), 0.0f) * w12.x * w3.y;
    result += tex.SampleLevel(linearSampler, float2(texPos3.x, texPos3.y), 0.0f) * w3.x * w3.y;

    return result;
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
    float2 currVelocity = GetVelocity(threadId.xy);

    #define MV_REP

    // backprojected
    float2 uv = (float2(threadId.xy) + 0.5) / resolution;

    #ifndef MV_REP
    float3 currWp = WorldPosFromDepth(uv, currDepth);
    // reprojected
    float4 rpClip = mul(prevViewProj, float4(currWp, 1.0));
    float2 rpNdc = (rpClip.xy / rpClip.w);
    float2 rpUv = float2(rpNdc.x * 0.5 + 0.5, 0.5 - rpNdc.y * 0.5);
    #else
    float2 rpUv = uv + currVelocity;
    #endif

    if(any(rpUv < 0.) || any(rpUv >= 1.)) {
        o[threadId.xy] = currCol;
        return;
    }
//    int2 rpPix = round(rpUv * resolution);
    float4 prevCol = SampleTextureCatmullRom(prev, Linear, rpUv, resolution);
//    float4 prevCol = prev.SampleLevel(Linear, rpUv, 0);
    prevCol = max(prevCol, 0);
    ClampPrevious(threadId.xy, prevCol);

    o[threadId.xy] = float4(prevCol.rgb * 0.9 + currCol.xyz * 0.1, 1);
}