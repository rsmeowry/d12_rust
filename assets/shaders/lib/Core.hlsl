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

Texture2D g_Tex2D[1024] : register(t0, space1);
TextureCube g_TexCube[1024] : register(t0, space2);
RWTexture2D<float4> g_RWTex2D[] : register(u0, space1);
RWByteAddressBuffer g_RWBuffers[] : register(u0, space2);
RWByteAddressBuffer g_RWScratch : register(u0);

static const float PI = 3.14159265;
