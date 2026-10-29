#include "../../lib/Core.hlsl"

SamplerState LinearWrap : register(s0);

cbuffer Pass : register(b2) {
    uint cloudTexture;
}

struct Varyings {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD;
};

Varyings vert(uint id : SV_VertexID) {
    Varyings o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = uv;
    return o;
}

#define STEP_SIZE 0.1
#define MAX_ITERS 100

float Smin(float a, float b, float k)
{
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * (1.0 / 4.0);
}
float InvLerp(float mn, float mx, float val) {
    return (val - mn) / (mx - mn);
}

float SdfSphere(float3 pos, float3 center, float rad) {
    return length(pos - center) - rad;
}

float SdfRoundedBox(float3 pos, float3 center, float3 bounds, float rad)
{
    float3 q = abs(pos - center) - bounds;
    return length(max(q, float3(0.0, 0.0, 0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - rad;
}

float Scene(float3 pos) {
    float sphere = SdfSphere(pos, float3(0., 0., -3.), 0.6);
    float box = SdfRoundedBox(pos, float3(0., 3. * sin(time), -3.), float3(0.6, 0.2, 0.5), 0.3);
    return Smin(sphere, box, 0.8);
}

float Raymarch(float3 pos, float3 rd, inout float3 sceneColor) {
    uint i = 0;
    float depth = MAX_ITERS * STEP_SIZE;
    Texture3D clouds = g_Tex3D[cloudTexture];
    while(i < MAX_ITERS) {
        depth -= STEP_SIZE;
        float3 wp = pos + i * STEP_SIZE * rd;
        sceneColor += clouds.SampleLevel(LinearWrap, wp / 10., 0) * 0.001;
        i += 1;
    }
    return 0.;
}

float4 frag(Varyings i) : SV_Target {
    float2 pixel = i.uv * resolution;

    float3 rd = normalize(WorldPosFromDepth(i.uv, 1.0) - cameraPos);
    float3 sceneColor = float3(0, 0, 0);
    float depth = Raymarch(cameraPos, rd, sceneColor);

    return float4(sceneColor, 1.0);
}

