cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 lightDir;
    float _pad;
    float3 cameraPos;
    float _pad2;
    float4 lightCol;
};

RWTexture2D<float4> g_RWTex2D[] : register(u0, space1);
RWByteAddressBuffer g_RWBuffers[] : register(u0, space2);
RWByteAddressBuffer g_RWScratch : register(u0);
