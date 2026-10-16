#define LIGHT_DIRECTIONAL 0
#define LIGHT_POINT 1
#define LIGHT_SPOT 2

#define PBR

SamplerComparisonState ShadowCmp : register(s4);

struct Surface {
    float3 pos;
    float3 normal;
    float3 albedo;
    #ifdef PBR
    float roughness;
    float metallic;
    #else
    float specular;
    float shininess;
    #endif
};

float DistanceAttenuation(float dist, float range) {
    float r = dist / range;
    float window = saturate(1.0 - r * r * r * r);
    return window * window / (dist * dist + 1.0);
}

#ifndef PBR
float3 ShadeLightPhong(Light l, Surface s, float3 V) {
    float3 L;
    float atten = 1.0;
    if (l.type == LIGHT_DIRECTIONAL) {
        L = -l.direction;
    } else {
        float3 toLight = l.position - s.pos;
        float dist = length(toLight);
        L = toLight / max(dist, 1e-4);
        atten = DistanceAttenuation(dist, l.range);
        if (l.type == LIGHT_SPOT) {
            atten *= smoothstep(l.spotOuterCos, l.spotInnerCos, dot(-L, l.direction));
        }
    }

    float NdotL = saturate(dot(s.normal, L));
    if (NdotL <= 0.0 || atten <= 0.0) return 0;

    float3 R = reflect(-L, s.normal);
    float RdotV = max(dot(R, V), 1e-5);
    float spec = pow(RdotV, max(s.shininess, 1.0)) * s.specular;

    return (s.albedo + spec) * NdotL * l.color * l.intensity * atten;
}
#endif

#ifdef PBR
float3 ShadeLightPBR(Light l, Surface s, float3 V) {
    return float3(s.normal.xy, s.roughness);
}
#endif

float3 ShadeLight(Light l, Surface s, float3 V) {
    #ifndef PBR
    return ShadeLightPhong(l, s, V);
    #else
    return ShadeLightPBR(l, s, V);
    #endif
}
