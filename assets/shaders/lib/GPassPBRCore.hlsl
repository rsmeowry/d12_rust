struct PbrMaterial {
    uint albedoTex;
    uint normalTex;
    uint metalRoughnessTex;
    uint occlusionTex;
    uint _texPad[4];

    float3 baseColor;
    float metallic;
    float roughness;
    uint _pad[11];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
cbuffer DrawFlags : register(b2) { uint meshletDebug; }
StructuredBuffer<PbrMaterial> g_Materials : register(t2);
SamplerState AnisoWrap : register(s3);

struct GPBRVaryings {
    float4 pos : SV_Position;
    float4 currClip : CURR_CLIP;
    float4 prevClip : PREV_CLIP;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
    nointerpolation uint meshletId : MESHLET_ID;
};

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float3 tangent : TANGENT;
    float3 binormal : BINORMAL;
    float2 uv : TEXCOORD;
};

GPBRVaryings vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    GPBRVaryings o;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, i.normal);
    o.currClip = mul(viewProjNj, posW);
    o.prevClip = mul(prevViewProj, mul(obj.prevModel, float4(i.pos, 1.0)));
    o.tangentW = mul((float3x3) obj.model, i.tangent);
    o.binormalW = mul((float3x3) obj.model, i.binormal);
    o.uv = i.uv;
    o.meshletId = 0;
    return o;
}

GBufferOut frag(GPBRVaryings i) {
    ObjectData obj = g_Objects[drawIndex];
    PbrMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    float4 albedo = g_Tex2D[mat.albedoTex].SampleBias(AnisoWrap, i.uv, mipBias);
    clip(albedo.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float4 texNormal = g_Tex2D[mat.normalTex].SampleBias(AnisoWrap, i.uv, mipBias) * 2.0 - 1.0;
    float3 normal = normalize(mul(texNormal.xyz, tbnMat));

    float4 roughnessMetallic = g_Tex2D[mat.metalRoughnessTex].SampleBias(AnisoWrap, i.uv, mipBias);
    float4 occlusion = g_Tex2D[mat.occlusionTex].SampleBias(AnisoWrap, i.uv, mipBias);

    GBufferOut o;
    o.albedo = float4(albedo.rgb * mat.baseColor, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(roughnessMetallic.g * mat.roughness, roughnessMetallic.b * mat.metallic, occlusion.r, 0);
    o.emissive = 0;
    o.velocity = GetMotionVector(i.currClip, i.prevClip);
    if (meshletDebug == 1) {
        uint h = PcgHash(i.meshletId);
        o.albedo.rgb = float3(h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF) / 255.0;
    }
    return o;
}