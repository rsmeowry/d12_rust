#define PCF_RADIUS 1
#define LOCAL_NORMAL_OFFSET 1.5
#define NORMAL_OFFSET 1.5
#define PCF_RADIUS 1

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

float SpotShadow(Light l, float3 pos, float3 offset, bool usePcf) {
    float dist = distance(pos, l.position);
    float3 p = pos + offset * (l.shadowParams.z * dist * LOCAL_NORMAL_OFFSET);
    float4 c = mul(l.shadowVp, float4(p, 1));
    c.xyz /= c.w;
    float2 uv = c.xy * float2(0.5, -0.5) + 0.5;
    return usePcf ? PCF(g_Tex2D[NonUniformResourceIndex(l.shadowTex)], uv, c.z) : g_Tex2D[NonUniformResourceIndex(l.shadowTex)].SampleCmpLevelZero(ShadowCmp, uv, c.z);
}

float PointShadow(Light l, float3 pos, float3 offset, bool usePcf) {
    float3 v = pos - l.position;
    v += offset * (l.shadowParams.z * length(v) * LOCAL_NORMAL_OFFSET);
    float3 a = abs(v);
    float z = max(a.x, max(a.y, a.z));
    float depth = l.shadowParams.x + l.shadowParams.y / z;
    return usePcf ? PCFCube(g_TexCube[NonUniformResourceIndex(l.shadowTex)], v, z, depth, l.shadowParams.z) : g_TexCube[NonUniformResourceIndex(l.shadowTex)].SampleCmpLevelZero(ShadowCmp, v, depth);
}

float ShadowInCascade(uint cascade, float3 pos, float3 offset, bool usePcf) {
    float3 posOff = pos + offset * cascadeTexel[cascade] * NORMAL_OFFSET;
    float4 p = mul(cascadeVp[cascade], float4(posOff, 1));
    p.xyz /= p.w;
    float2 uv = p.xy * float2(0.5, -0.5) + 0.5;
    if (any(uv < 0) || any(uv > 1) || p.z < 0 || p.z > 1)
        return 1;

    Texture2D map = g_Tex2D[NonUniformResourceIndex(shadowTex[cascade])];
    return usePcf ? PCF(map, uv, p.z) : map.SampleCmpLevelZero(ShadowCmp, uv, p.z);
}