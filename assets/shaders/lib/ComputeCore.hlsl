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

Texture2D g_Tex2D[1024] : register(t0, space1);
TextureCube g_TexCube[1024] : register(t0, space2);
RWTexture2D<float4> g_RWTex2D[1024] : register(u0, space1);
RWByteAddressBuffer g_RWBuffers[1024] : register(u0, space2);
RWByteAddressBuffer g_RWScratch : register(u0);

static const float PI = 3.14159265;

float Luma(float3 c) {
    return dot(c, float3(0.2126, 0.7152, 0.0722));
}
