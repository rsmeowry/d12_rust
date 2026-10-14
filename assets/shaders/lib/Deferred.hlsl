#define NORMAL_OFFSET 1.5
#define PCF_RADIUS 1

cbuffer LightingPass : register(b2) {
    uint albedoTex;
    uint normalTex;
    uint materialTex;
    uint depthTex;
    uint dirCount;
    uint debugView;
    uint skyTex;
    float _pad;
    float3 ambientColor;
    float _alsoPad;
    float4x4 cascadeVp[4];
    float4 splits;
    float4 cascadeTexel;
    uint4 shadowTex;
    float3 camForward;
    uint useShadows;
};

float3 WorldPosFromDepth(float2 uv, float depth) {
    float4 clip = float4(uv.x * 2 - 1, 1 - uv.y * 2, depth, 1);
    float4 w = mul(invViewProj, clip);
    return w.xyz / w.w;
}

bool LoadSurface(float2 pixel, out Surface s) {
    s = (Surface) 0;
    int3 px = int3(pixel, 0);
    float depth = g_Tex2D[depthTex].Load(px).r;
    if (depth == 0.0)
        return false;
    s.albedo = g_Tex2D[albedoTex].Load(px).rgb;
    s.normal = normalize(g_Tex2D[normalTex].Load(px).xyz * 2 - 1);
    float4 m = g_Tex2D[materialTex].Load(px);
    s.specular = m.r;
    s.shininess = m.g * 255.0;
    s.pos = WorldPosFromDepth(pixel / resolution, depth);
    return true;
}

float ShadowInCascade(uint cascade, float3 pos, float3 offset) {
    float3 posOff = pos + offset * cascadeTexel[cascade] * NORMAL_OFFSET;
    float4 p = mul(cascadeVp[cascade], float4(posOff, 1));
    p.xyz /= p.w;
    float2 uv = p.xy * float2(0.5, -0.5) + 0.5;
    if (any(uv < 0) || any(uv > 1) || p.z < 0 || p.z > 1)
        return 1;

    Texture2D map = g_Tex2D[NonUniformResourceIndex(shadowTex[cascade])];
    return PCF(map, uv, p.z);
}