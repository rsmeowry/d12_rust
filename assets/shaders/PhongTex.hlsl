#include <lib/Core.hlsl>

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint _pad[3];
};

struct PhongMaterial {
    uint albedoTex;
    uint _texPad[7];

    float3 ambientColor;
    float3 diffuseColor;
    float3 specularColor;

    float shininess;
    float diffuseK;
    float specularK;

    float4 _pad;
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects : register(t1);
StructuredBuffer<PhongMaterial> g_Materials : register(t2);
Texture2D g_Tex2D[1024] : register(t0, space1);
SamplerState AnisoWrap : register(s3);

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
    PhongMaterial mat = g_Materials[obj.materialId];

    float3 N = normalize(i.normalW);
    float3 L = normalize(-lightDir);
    float3 V = normalize(cameraPos - i.posW);
    float3 R = reflect(-L, N);

    float4 albedo = g_Tex2D[mat.albedoTex].Sample(AnisoWrap, i.uv);
    float3 ambient = albedo.xyz * mat.ambientColor * lightCol;

    float diffuseFactor = max(dot(N, L), 0.0f);
    float3 diffuse = diffuseFactor * albedo * mat.diffuseColor * lightCol * mat.diffuseK;

    float specularFactor = pow(max(dot(R, V), 0.0f), mat.shininess);
    float3 specular = specularFactor * mat.specularColor * lightCol * mat.specularK;

    specular *= (diffuseFactor > 0.0f) ? 1.0f : 0.0f;

    float4 result = float4(ambient + diffuse + specular, 1.0);

    return result;
}