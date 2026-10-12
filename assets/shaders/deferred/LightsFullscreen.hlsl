#include <lib/Core.hlsl>
#include <lib/Lighting.hlsl>
#include <lib/Deferred.hlsl>

StructuredBuffer<Light> g_Lights : register(t3);
SamplerState AnisoWrap : register(s3);
SamplerComparisonState ShadowCmp : register(s4);

#define NORMAL_OFFSET 1.5
#define PCF_RADIUS 1
#define SKY_INTENSITY 1.5

float3 SampleSky(float2 pixel) {
    float3 dir = normalize(WorldPosFromDepth(pixel / resolution, 1.) - cameraPos);
    float2 uv = float2(atan2(dir.z, dir.x) / (2 * PI) + 0.5, acos(dir.y) / PI);
    return g_Tex2D[skyTex].Sample(AnisoWrap, uv, 0).rgb * SKY_INTENSITY;
}

float SampleShadow(Surface s) {
    if (useShadows == 0)
        return 1;

    float3 posOff = s.pos + s.normal * cascadeTexel.x * NORMAL_OFFSET;

    float4 p = mul(cascadeVp, float4(posOff, 1));
    p.xyz /= p.w;

    float2 uv = p.xy * float2(0.5, -0.5) + 0.5;

    if (any(uv < 0) || any(uv > 1) || p.z < 0 || p.z > 1)
        return 1;

    Texture2D map = g_Tex2D[shadowsTex];
    float w, h;
    map.GetDimensions(w, h);
    float2 t = 1.0 / float2(w, h);

    float sum = 0;
    [unroll] for (int y = -PCF_RADIUS; y <= PCF_RADIUS; y++)
    [unroll] for (int x = -PCF_RADIUS; x <= PCF_RADIUS; x++)
        sum += map.SampleCmpLevelZero(ShadowCmp, uv + float2(x, y) * t, p.z);

    const float n = 2 * PCF_RADIUS + 1;
    return sum / (n * n);
}

float4 vert(uint id : SV_VertexID) : SV_Position {
    float2 uv = float2((id << 1) & 2, id & 2);
    return float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
}

float4 frag(float4 pos : SV_Position) : SV_Target {
    Surface s;
    if (!LoadSurface(pos.xy, s))
        return float4(SampleSky(pos.xy), 0);

    if (debugView == 1)
        return float4(s.albedo, 0);
    if (debugView == 2)
        return float4(s.normal, 0);
    if (debugView == 3)
        return float4(frac(s.pos), 0);

    float shadow = SampleShadow(s);
    float3 V = normalize(cameraPos - s.pos);
    float3 color = s.albedo * ambientColor;
    for (uint i = 0; i < dirCount; i++)
        color += ShadeLight(g_Lights[i], s, V) * (i == 0 ? shadow : 1.0);
    return float4(color, 0);
}