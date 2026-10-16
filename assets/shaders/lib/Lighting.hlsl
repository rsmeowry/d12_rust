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
// Throwbridge-Reitz GGX NDF
float NormalDistributionFunction(Light l, Surface s, float3 V) {
    float alpha = s.roughness * s.roughness;
    float alpha2 = alpha * alpha;
    float3 L = l.direction;
    float3 H = normalize(V - L);
    float nDotH = max(dot(s.normal, H), 0.001);
    float nDotH2 = nDotH * nDotH;
    float comp = (nDotH2 * (alpha2 - 1) + 1);
    return alpha2 / (PI * (comp * comp));
}

// Schlick-GGX Geometry Function
float GeometrySchlick(float nDotV, float roughness) {
    float k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    return nDotV / (nDotV * (1 - k) + k);
}

// Smith-Schlick-GGX GF
float GeometryFunction(Light l, Surface s, float3 V) {
    float nDotV = max(dot(s.normal, V), 0.001);
    float nDotL = max(dot(s.normal, -l.direction), 0.001);
    float schlick1 = GeometrySchlick(nDotV, s.roughness);
    float schlick2 = GeometrySchlick(nDotL, s.roughness);
    return schlick1 * schlick2;
}

// Fresnel-Schlick
float3 FresnelSchlick(Light l, Surface s, float3 V) {
    float3 f0 = float3(0.04, 0.04, 0.04); // taking an approx value here
    f0 = lerp(f0, s.albedo, s.metallic);
    float3 L = l.direction;
    float3 H = normalize(V - L);
    float hDotV = max(dot(H, V), 0.001);
    float twoPowBullshit = exp2(((-5.55473 * hDotV) - 6.98316) * hDotV);
    return f0 + (1 - f0) * twoPowBullshit;
}

float3 ShadeLightPBR(Light l, Surface s, float3 V) {
    float nDotV = max(dot(s.normal, V), 0.001);
    float nDotL = max(dot(s.normal, -l.direction), 0.001);

    if(nDotL <= 0.0) {
        return 0.0;
    }

    float D = NormalDistributionFunction(l, s, V);
    float3 F = FresnelSchlick(l, s, V);
    float G = GeometryFunction(l, s, V);

    // specular (DFG) / 4(nDotV*nDotL)
    float3 specular = (D * F * G) / (4 * (nDotV * nDotL));

    return specular;
}
#endif

float3 ShadeLight(Light l, Surface s, float3 V) {
    #ifndef PBR
    return ShadeLightPhong(l, s, V);
    #else
    return ShadeLightPBR(l, s, V);
    #endif
}
