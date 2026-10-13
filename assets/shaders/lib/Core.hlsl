cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float4x4 invViewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 cameraPos;
    float _pad2;
};

TextureCube g_TexCube[] : register(t0, space2);
RWTexture2D<float4> g_RWTex2D[] : register(u0, space1);
RWByteAddressBuffer g_RWBuffers[] : register(u0, space2);
RWByteAddressBuffer g_RWScratch : register(u0);

static const float PI = 3.14159265;