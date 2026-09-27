cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float2 resolution;
    float time;
    float deltaTime;
    float3 lightDir;
    float _pad;
    float4 lightCol;
};
