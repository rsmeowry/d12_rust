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
