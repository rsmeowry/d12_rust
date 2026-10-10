#include <lib/Core.hlsl>
#include <lib/Lighting.hlsl>
#include <lib/Deferred.hlsl>

StructuredBuffer<Light> g_Lights : register(t3);
SamplerState AnisoWrap : register(s3);

#define SKY_INTENSITY 1.5

float3 SampleSky(float2 pixel) {
    float3 dir = normalize(WorldPosFromDepth(pixel / resolution, 1.) - cameraPos);
    float2 uv = float2(atan2(dir.z, dir.x) / (2 * PI) + 0.5, acos(dir.y) / PI);
    return g_Tex2D[skyTex].Sample(AnisoWrap, uv, 0).rgb * SKY_INTENSITY;
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

    float3 V = normalize(cameraPos - s.pos);
    float3 color = s.albedo * ambientColor;
    for (uint i = 0; i < dirCount; i++)
        color += ShadeLight(g_Lights[i], s, V);
    return float4(color, 0);
}