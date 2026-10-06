#include <Core.hlsl>

SamplerState LINEAR : register(s0);

struct ObjectData {
    float4x4 model;
    float4 ambientColor;
    float4 diffuseColor;
    float4 specularColor;

    float shininess;
    float diffuseK;
    float specularK;
    uint albedoTex;
};


cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects :  register(t1);
Texture2D g_Tex2D[1024] : register(t0, space1);

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
    ObjectData obj = g_Objects[drawIndex];

    float4 posW = mul(obj.model, float4(i.pos, 1.0));

    vout.pos = mul(viewProj, posW);
    vout.posW = posW.xyz;
    vout.normalW = mul((float3x3) obj.model, i.normal);
    vout.uv = i.uv;

    return vout;
}

float4 frag(Varyings i) : SV_Target {
    ObjectData obj = g_Objects[drawIndex];

    float3 N = normalize(i.normalW);
    float3 L = normalize(-lightDir);
    float3 V = normalize(cameraPos - i.posW);
    float3 R = reflect(-L, N);

    float4 albedo = g_Tex2D[obj.albedoTex].Sample(LINEAR, i.uv);
    float4 ambient = albedo * obj.ambientColor * lightCol;

    float diffuseFactor = max(dot(N, L), 0.0f);
    float4 diffuse = diffuseFactor * albedo * obj.diffuseColor * lightCol * obj.diffuseK;

    float specularFactor = pow(max(dot(R, V), 0.0f), obj.shininess);
    float4 specular = specularFactor * obj.specularColor * lightCol * obj.specularK;

    specular *= (diffuseFactor > 0.0f) ? 1.0f : 0.0f;

    float4 result = ambient + diffuse + specular;
    result.a = 1.0f;

    return result;
}