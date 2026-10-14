#define LIGHT_DIRECTIONAL 0
#define LIGHT_POINT 1
#define LIGHT_SPOT 2
#define PCF_RADIUS 1
#define LOCAL_NORMAL_OFFSET 1.5

struct Light {
    float3 position;
    float range;
    float3 direction;
    uint type;
    float3 color;
    float intensity;
    float spotInnerCos;
    float spotOuterCos;
    float spotRadius;
    uint shadowTex;
    float4x4 shadowVp;
    float4 shadowParams;
};

struct Surface {
    float3 pos;
    float3 normal;
    float3 albedo;
    float specular;
    float shininess;
};

float DistanceAttenuation(float dist, float range) {
    float r = dist / range;
    float window = saturate(1.0 - r * r * r * r);
    return window * window / (dist * dist + 1.0);
}

float PCF(Texture2D shadowMap, float2 uv, float z) {
    float w;
    float h;

    shadowMap.GetDimensions(w, h);
    float2 t = 1.0 / float2(w, h);

    float sum = 0;
    [unroll] for (int y = -PCF_RADIUS; y <= PCF_RADIUS; y++)
    [unroll] for (int x = -PCF_RADIUS; x <= PCF_RADIUS; x++)
        sum += shadowMap.SampleCmpLevelZero(ShadowCmp, uv + float2(x, y) * t, z);
    const float n = 2 * PCF_RADIUS + 1;
    return sum / (n * n);
}

float PCFCube(TextureCube shadowMap, float3 uvw, float z, float depth, float texel) {
    float w;
    float h;

    float3 n = normalize(uvw);
    float3 t= normalize(cross(abs(n.y) < 0.99 ? float3(0, 1, 0) : float3(1, 0, 0), n));
    float3 b = cross(n, t);
    float stp = texel * z;
    float sum = 0;

    [unroll] for (int y = -PCF_RADIUS; y <= PCF_RADIUS; y++)
    [unroll] for (int x = -PCF_RADIUS; x <= PCF_RADIUS; x++)
        sum += shadowMap.SampleCmpLevelZero(ShadowCmp, uvw + (t * x + b * y) * stp, depth);
    const float k = 2 * PCF_RADIUS + 1;
    return sum / (k * k);
}

float SpotShadow(Light l, float3 pos, float3 offset) {
    float dist = distance(pos, l.position);
    float3 p = pos + offset * (l.shadowParams.z * dist * LOCAL_NORMAL_OFFSET);
    float4 c = mul(l.shadowVp, float4(p, 1));
    c.xyz /= c.w;
    float2 uv = c.xy * float2(0.5, -0.5) + 0.5;
    return PCF(g_Tex2D[NonUniformResourceIndex(l.shadowTex)], uv, c.z);
}

float PointShadow(Light l, float3 pos, float3 offset) {
    float3 v = pos - l.position;
    v += offset * (l.shadowParams.z * length(v) * LOCAL_NORMAL_OFFSET);
    float3 a = abs(v);
    float z = max(a.x, max(a.y, a.z));
    float depth = l.shadowParams.x + l.shadowParams.y / z;
    return PCFCube(g_TexCube[NonUniformResourceIndex(l.shadowTex)], v, z, depth, l.shadowParams.z);
}

float3 ShadeLight(Light l, Surface s, float3 V) {
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