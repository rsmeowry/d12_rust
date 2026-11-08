#define PBR

cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float4x4 viewProjNj;
    float4x4 invViewProj;
    float4x4 prevViewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 cameraPos;
    float mipBias;
    float2 displayResolution;
    uint2 _pad2;
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
    float4x4 staticVp[4];
    float4 splits;
    float4 texel;
    float4 staticTexel;
    uint4 tex;
    uint4 staticTex;
    float3 camForward;
    uint enabled;
};

struct ObjectData {
    float4x4 model;
    float4x4 prevModel;
    uint materialId;
    uint3 _oPad;
};

Texture2D g_Tex2D[1024] : register(t0, space1);
TextureCube g_TexCube[1024] : register(t0, space2);
Texture3D g_Tex3D[1024] : register(t0, space3);
RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
RWByteAddressBuffer g_RWBuffers[1024] : register(u0, space2);
RWByteAddressBuffer g_RWScratch : register(u0);
StructuredBuffer<ObjectData> g_Objects : register(t1);

static const float PI = 3.14159265;

void BillboardTowards(float3 forward, out float3 right, out float3 up) {
    float3 rUp = abs(forward.y) < 0.99 ? float3(0, 1, 0) : float3(1, 0, 0);
    right = normalize(cross(forward, rUp));
    up = cross(right, forward);
}

static const float2 QUAD_CORNERS[4] = { float2(-1, -1), float2(-1, 1), float2(1, -1), float2(1, 1) };
static const float2 QUAD_UVS[4] = { float2(0., 1.), float2(0., 0.), float2(1, 1.), float2(1., 0.) };

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

float3 DecodeOctNormals(float2 v) {
    float2 vf = float2(v.x, v.y);
    float3 n = float3(vf.x, vf.y, 1.0 - abs(vf.x) - abs(vf.y));
    float t = saturate(-n.z);
    n.x += (n.x >= 0.0) ? -t : t;
    n.y += (n.y >= 0.0) ? -t : t;
    return normalize(n);
}

float4 TriplanarSample(Texture2D tex, SamplerState samplerState, float3 worldPos, float3 normal, float scale)
{
    float3 blendWeights = abs(normal);
    blendWeights = normalize(max(blendWeights, 0.0001f)); // div by zero :(

    blendWeights /= dot(blendWeights, float3(1, 1, 1));

    float4 xPlane = tex.Sample(samplerState, worldPos.yz * scale);
    float4 yPlane = tex.Sample(samplerState, worldPos.xz * scale);
    float4 zPlane = tex.Sample(samplerState, worldPos.xy * scale);

    // blending
    float4 triplanarColor = xPlane * blendWeights.x + yPlane * blendWeights.y + zPlane * blendWeights.z;
    return triplanarColor;
}

// https://bgolus.medium.com/normal-mapping-for-a-triplanar-shader-10bf39dca05a
float3 TriplanarNormal(Texture2D tex, SamplerState s, float3 pos, float3 n, float scale) {
    float3 w = pow(abs(n), 4);
    w /= dot(w, 1);

    float3 tnX = tex.Sample(s, pos.zy * scale).xyz * 2 - 1;
    float3 tnY = tex.Sample(s, pos.xz * scale).xyz * 2 - 1;
    float3 tnZ = tex.Sample(s, pos.xy * scale).xyz * 2 - 1;

    // whiteout blend
    tnX = float3(tnX.xy + n.zy, abs(tnX.z) * n.x);
    tnY = float3(tnY.xy + n.xz, abs(tnY.z) * n.y);
    tnZ = float3(tnZ.xy + n.xy, abs(tnZ.z) * n.z);

    // swizzle each plane tangent space back to world
    return normalize(tnX.zyx * w.x + tnY.xzy * w.y + tnZ.xyz * w.z);
}

uint PcgHash(uint input)
{
    uint state = input * 747796405u + 2891336453u;
    uint word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
