cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float4x4 invViewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 cameraPos;
    float _pad2;
};

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

struct Cascades {
    float4x4 vp[4];
    float4 splits;
    float4 texel;
    uint4 tex;
    float3 camForward;
    uint enabled;
};

//#define PBR

Texture2D g_Tex2D[1024] : register(t0, space1);
TextureCube g_TexCube[1024] : register(t0, space2);
Texture3D g_Tex3D[1024] : register(t0, space3);
RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
RWByteAddressBuffer g_RWBuffers[1024] : register(u0, space2);
RWByteAddressBuffer g_RWScratch : register(u0);

static const float PI = 3.14159265;

float3 WorldPosFromDepth(float2 uv, float depth) {
    float4 clip = float4(uv.x * 2 - 1, 1 - uv.y * 2, depth, 1);
    float4 w = mul(invViewProj, clip);
    return w.xyz / w.w;
}

// https://github.com/patriciogonzalezvivo/lygia/blob/main/color/dither/interleavedGradientNoise.hlsl
float InterleavedGradientNoise(const in float2 n) {
    return frac(52.982919 * frac(dot(float2(0.06711, 0.00584), n)));
}

float DitherInterleavedGradientNoise(float b, float2 fragcoord, const in float t) {
    fragcoord += 1337.0 * frac(t);
    float noise = InterleavedGradientNoise(fragcoord);
    // remap from [0..1[ to [-1..1[
    noise = (noise * 2.0) - 1.0;
    return b + noise / 255.0;
}

