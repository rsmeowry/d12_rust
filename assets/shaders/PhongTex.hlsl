#include <Core.hlsl>

Texture2D tex : register(t0);
SamplerState LINEAR : register(s0);

cbuffer PhongObj : register(b0) {
    float4x4 model;

    float3 cameraPos;
    float shininess;
    float4 ambientColor;
    float4 diffuseColor;
    float4 specularColor;
}

struct Attributes {
    float3 pos: POSITION;
    float3 normal: NORMAL;
    float2 uv: TEXCOORD;
};

struct Varyings {
    float4 pos: SV_Position;
    float3 posW: POSITION;
    float3 normalW: NORMAL;
    float2 uv: TEXCOORD;
};

Varyings vert(Attributes i) {
    Varyings vout;

    float4 posW = mul(model, float4(i.pos, 1.0));

    vout.pos = mul(viewProj, posW);
    vout.posW = posW.xyz;
    vout.normalW = mul((float3x3) model, i.normal);
    vout.uv = i.uv;

    return vout;
}

float4 frag(Varyings i) : SV_Target {
    float3 N = normalize(i.normalW);
    float3 L = normalize(-lightDir);
    float3 V = normalize(cameraPos - i.posW);
    float3 R = reflect(-L, N);

    float4 ambient = ambientColor * lightCol;

    float diffuseFactor = max(dot(N, L), 0.0f);
    float4 diffuse = diffuseFactor * tex.Sample(LINEAR, i.uv) * diffuseColor * lightCol;

    float specularFactor = pow(max(dot(R, V), 0.0f), shininess);
    float4 specular = specularFactor * specularColor * lightCol;

    specular *= (diffuseFactor > 0.0f) ? 1.0f : 0.0f;

    float4 result = ambient + diffuse + specular;
    result.a = 1.0f;

    return result;
}