#include "../lib/Core.hlsl"
#include "../lib/Particles.hlsl"

cbuffer MeshParticlePass : register(b2) {
    uint albedoTex;
    uint normalTex;
    uint metalRoughnessTex;
    uint occlusionTex;
    float3 baseColor;
    float metallic;
    float3 meshCenter;
    float meshScale;
    float roughness;
    uint3 _pad;
};

StructuredBuffer<Particle> g_Pool : register(t1);
StructuredBuffer<uint2>    g_Sort : register(t2);
SamplerState AnisoWrap : register(s3);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float3 tangent : TANGENT;
    float3 binormal : BINORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
    float3 color : COLOR;
};

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

static const float TAU = PI * 2;

float3x3 AxisAngle(float3 a, float ang) {
    float s, c;
    sincos(ang, s, c);
    float t = 1 - c;
    return float3x3(
        t * a.x * a.x + c,       /* */ t * a.x * a.y - s * a.z, /* */ t * a.x * a.z + s * a.y,
        t * a.x * a.y + s * a.z, /* */ t * a.y * a.y + c,       /* */ t * a.y * a.z - s * a.x,
        t * a.x * a.z - s * a.y, /* */ t * a.y * a.z + s * a.x, /* */ t * a.z * a.z + c
    );
}

float3x3 TumbleRotation(uint idx, Particle p) {
    uint s = Hash(idx ^ asuint(p.maxLifetime));
    float z = Rand(s) * 2 - 1;
    float phi = Rand(s) * TAU;
    float3 axis = float3(sqrt(1 - z * z) * float2(cos(phi), sin(phi)), z);
    float startAngle = Rand(s) * TAU;
    float speed = lerp(1.0, 6.0, Rand(s));
    float age = p.maxLifetime - p.lifetimeSeconds;
    return AxisAngle(axis, startAngle + speed * age);
}

Varyings vert(Attributes v, uint iid : SV_InstanceID) {
    uint idx = g_Sort[iid].x;
    Particle p = g_Pool[idx];
    float scale = p.size * meshScale * saturate(p.lifetimeSeconds / p.maxLifetime * 4);
    float3x3 R = TumbleRotation(idx, p);

    float3 world = p.pos + mul(R, (v.pos - meshCenter) * scale);
    Varyings o;
    o.pos = mul(viewProj, float4(world, 1));
    o.normalW = mul(R, v.normal);
    o.tangentW = mul(R, v.tangent);
    o.binormalW = mul(R, v.binormal);
    o.uv = v.uv;
    o.color = p.color;
    return o;
}

GBufferOut frag(Varyings i) {
    float4 albedo = g_Tex2D[albedoTex].Sample(AnisoWrap, i.uv);
    float3x3 tbn = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float3 n = normalize(mul(g_Tex2D[normalTex].Sample(AnisoWrap, i.uv).xyz * 2 - 1, tbn));
    float4 mr = g_Tex2D[metalRoughnessTex].Sample(AnisoWrap, i.uv);
    float ao = g_Tex2D[occlusionTex].Sample(AnisoWrap, i.uv).r;

    GBufferOut o;
    o.albedo = float4(albedo.rgb * baseColor * i.color, 1);
    o.normal = float4(n * 0.5 + 0.5, 0);
    o.material = float4(mr.g * roughness, mr.b * metallic, ao, 0);
    o.emissive = 0;
    return o;
}