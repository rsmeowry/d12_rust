cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 lightDir;
    float3 cameraPos;
    float4 lightCol;
    float2 _pad;
};
