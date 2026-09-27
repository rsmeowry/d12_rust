cbuffer Obj : register(b0) {
    float4x4 model;
};

cbuffer Globals : register(b1) {
    float4x4 viewProj;
    float2 resolution;
    float time;
    float deltaTime;
};