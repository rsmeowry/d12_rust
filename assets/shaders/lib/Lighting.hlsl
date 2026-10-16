#define LIGHT_DIRECTIONAL 0
#define LIGHT_POINT 1
#define LIGHT_SPOT 2

SamplerComparisonState ShadowCmp : register(s4);

float DistanceAttenuation(float dist, float range) {
    float r = dist / range;
    float window = saturate(1.0 - r * r * r * r);
    return window * window / (dist * dist + 1.0);
}

#ifndef COMPUTE
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
SamplerState LinearWrap : register(s0);
SamplerState LinearClamp : register(s1);

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
float3 BaseReflectivity(Surface s) {
    return lerp(0.04, s.albedo, s.metallic);
}

float3 FresnelSchlick(float cosTheta, float3 f0) {
    return f0 + (1 - f0) * pow(1 - cosTheta, 5);
}

float3 FresnelSchlickRoughness(float cosTheta, float3 f0, float roughness) {
    return f0 + (max(1 - roughness, f0) - f0) * pow(1 - cosTheta, 5);
}

float3 AmbientIBL(Surface s, float3 V) {
    float nDotV = saturate(dot(s.normal, V));
    float3 F = FresnelSchlickRoughness(nDotV, BaseReflectivity(s), s.roughness);
    float3 kD = (1 - F) * (1 - s.metallic);

    float3 irradiance = g_TexCube[irradianceTex].SampleLevel(LinearClamp, s.normal, 0).rgb;
    float3 R = reflect(-V, s.normal);
    float3 prefiltered = g_TexCube[specularTex].SampleLevel(LinearClamp, R, s.roughness * (specularMips - 1)).rgb;
    float2 brdf = g_Tex2D[brdfLutTex].SampleLevel(LinearClamp, float2(nDotV, s.roughness), 0).rg;

    return (kD * irradiance * s.albedo + prefiltered * (F * brdf.x + brdf.y)) * s.ao;
}

float3 ShadeLightPBR(Light l, Surface s, float3 V) {
    float3 L = -l.direction;
    float atten = 1.0;
    if (l.type != LIGHT_DIRECTIONAL) {
        float3 toLight = l.position - s.pos;
        float dist = length(toLight);
        L = toLight / max(dist, 1e-4);
        atten = DistanceAttenuation(dist, l.range);
        if (l.type == LIGHT_SPOT) {
            atten *= smoothstep(l.spotOuterCos, l.spotInnerCos, dot(-L, l.direction));
        }
    }

    float nDotL = saturate(dot(s.normal, L));
    if (nDotL <= 0.0 || atten <= 0.0) return 0;

    float3 H = normalize(V + L);
    float nDotV = max(dot(s.normal, V), 1e-3);
    float r = max(s.roughness, 0.045);
    float a2 = r * r * r * r;
    float nDotH = saturate(dot(s.normal, H));
    float d = nDotH * nDotH * (a2 - 1) + 1;
    float D = a2 / (PI * d * d);
    float G = GeometrySchlick(nDotV, r) * GeometrySchlick(nDotL, r);
    float3 F = FresnelSchlick(saturate(dot(H, V)), BaseReflectivity(s));

    float3 spec = D * G * F / (4 * nDotV * nDotL);
    float3 kD = (1 - F) * (1 - s.metallic);
    return (kD * s.albedo / PI + spec) * l.color * l.intensity * atten * nDotL;
}
#endif

float3 ShadeLight(Light l, Surface s, float3 V) {
    #ifndef PBR
    return ShadeLightPhong(l, s, V);
    #else
    return ShadeLightPBR(l, s, V);
    #endif
}

#endif