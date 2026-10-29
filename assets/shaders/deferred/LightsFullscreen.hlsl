#include "../lib/Core.hlsl"
#include "../lib/Deferred.hlsl"
#include "../lib/Lighting.hlsl"
#include "../lib/Shadows.hlsl"
#include "../lib/Atmosphere.hlsl"

StructuredBuffer<Light> g_Lights : register(t3);
//SamplerState LinearWrap : register(s0);

#define SKY_INTENSITY 1.2
#define SKY_CUBEMAP 0

#define SUN_ANGULAR_RADIUS 0.0293
#define SUN_DISC_INTENSITY 100.0

float3 SunDisc(float3 rd, float3 toSun, float3 T) {
    float theta = acos(saturate(dot(rd, toSun)));
    float mask = 1 - smoothstep(SUN_ANGULAR_RADIUS * 0.9, SUN_ANGULAR_RADIUS, theta);
    return mask * SUN_DISC_INTENSITY * T;
}

float2 SkyUV(float2 pixel) {
    float3 dir = normalize(WorldPosFromDepth(pixel / resolution, 1.) - cameraPos);
    return float2(atan2(dir.z, dir.x) / (2 * PI) + 0.5, acos(dir.y) / PI);
}

float3 SampleSky(float2 pixel) {
    if(useAtmosphere == 1) {
        float3 ro = AtmosphereOrigin();
        float3 rd = normalize(WorldPosFromDepth(pixel / resolution, 1.0) - cameraPos);
        float3 t;
        float3 att = Atmosphere(ro, rd, RaySphereExit(ro, rd, ATMOSPHERE_EXIT), betaRayleigh, g_Lights[0].direction, t);
        return att + SunDisc(rd, -g_Lights[0].direction, t);
    }
#if SKY_CUBEMAP
    float3 dir = normalize(WorldPosFromDepth(pixel / resolution, 1.) - cameraPos);
    return g_TexCube[skyTex].Sample(LinearWrap, dir, 0).rgb * SKY_INTENSITY;
#else
    return g_Tex2D[skyTex].SampleLevel(LinearWrap, SkyUV(pixel), 0).rgb * SKY_INTENSITY;
#endif
}

float4 vert(uint id : SV_VertexID) : SV_Position {
    float2 uv = float2((id << 1) & 2, id & 2);
    return float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
}

float4 frag(float4 pos : SV_Position) : SV_Target {
    Surface s;
    if (!LoadSurface(pos.xy, s)) {
        return float4(SampleSky(pos.xy), 0);
    }

    if (debugView == 1)
        return float4(s.normal, 0);
    #ifdef PBR
    if (debugView == 2)
        return float4(s.metallic.xxx, 0);
    if (debugView == 3)
        return float4(s.roughness.xxx, 0);
    if (debugView == 4)
        return float4(s.ao.xxx, 0);
    #endif

    float3 V = normalize(cameraPos - s.pos);
    float3 color = s.albedo * ambientColor;
    #ifdef PBR
    color = AmbientIBL(s, V) * ambientColor;
    #endif
    for (uint i = 0; i < dirCount; i++) {
        float3 litVal = ShadeLight(g_Lights[i], s, V);
        if(i == 0 && any(litVal > 0)) {
            litVal *= SampleCascades(s.pos, s.normal, true);
        }
        color += litVal;
    }

    if(useAtmosphere == 1) {
        float3 ro = AtmosphereOrigin();
        float3 rd = normalize(WorldPosFromDepth(pos.xy / resolution, 1.0) - cameraPos);
        float3 t;
        float3 att = Atmosphere(ro, -V, distance(cameraPos, s.pos) * WORLD_TO_KM, betaRayleigh, g_Lights[0].direction, t);
        return float4(color * t + att, 0);
    }

    return float4(color, 0);
}