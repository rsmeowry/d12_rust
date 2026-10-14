#include <../lib/Core.hlsl>
#define MAX_TF 16.
#define FAR 15.0
#define NEAR 0.1
#define DISP 0.2
#define OPENGL_NORMAL 0

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint3 _pad;
};

struct PhongTessMaterial {
    uint albedoTex;
    uint normalTex;
    uint displacementMap;
    uint _texPad[6];

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
StructuredBuffer<PhongTessMaterial> g_Materials : register(t2);
SamplerState AnisoWrap : register(s3);

struct Attributes {
    float3 pos : POSITION;
    float3 normal : NORMAL;
    float3 tangent : TANGENT;
    float3 binormal  : BINORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
};

struct VertOut {
    float4 pos : SV_Position;
    float3 posW : POSITION;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
};

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

VertOut vert(Attributes i) {
    ObjectData obj = g_Objects[drawIndex];
    float4 posW = mul(obj.model, float4(i.pos, 1.0));
    VertOut o;
    o.pos = mul(viewProj, posW);
    o.posW = posW.xyz;
    o.normalW = normalize(mul((float3x3) obj.model, i.normal));
    o.tangentW = mul((float3x3) obj.model, i.tangent);
    o.binormalW = mul((float3x3) obj.model, i.binormal);
    o.uv = i.uv;
    return o;
}

struct HullOut {
    float3 posW : POSITION;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
};

// once per control point
[domain("tri")]
[partitioning("fractional_odd")]
[outputtopology("triangle_cw")]
[outputcontrolpoints(3)]
[patchconstantfunc("Constants")]
[maxtessfactor(MAX_TF)]
HullOut hull(InputPatch<VertOut, 3> patch, uint uCPID : SV_OutputControlPointID) {
    HullOut o;
    o.posW = patch[uCPID].posW;
    o.normalW = patch[uCPID].normalW;
    o.tangentW = patch[uCPID].tangentW;
    o.binormalW = patch[uCPID].binormalW;
    o.uv = patch[uCPID].uv;

    return o;
}

struct HullConsts {
    float edges[3] : SV_TessFactor;
    float inside : SV_InsideTessFactor;
};

float EdgeFac(float3 a, float3 b) {
    float d = distance(cameraPos, 0.5 * (a + b));
    float t = saturate((d - NEAR) / (FAR - NEAR));
    float tf = lerp(MAX_TF, 1.0, t);
    return tf;
}

// once per patch
HullConsts Constants(InputPatch<VertOut, 3> p, uint patchID : SV_PrimitiveID) {
    HullConsts o;

    o.edges[0] = EdgeFac(p[1].posW, p[2].posW);
    o.edges[1] = EdgeFac(p[2].posW, p[0].posW);
    o.edges[2] = EdgeFac(p[0].posW, p[1].posW);
    o.inside = (o.edges[0] + o.edges[1] + o.edges[2]) / 3.0;
    return o;
}

[domain("tri")]
Varyings domain(HullConsts input, float3 bary: SV_DomainLocation, const OutputPatch<HullOut, 3> patch) {
    Varyings o;

    float3 pos = bary.x * patch[0].posW + bary.y * patch[1].posW + bary.z * patch[2].posW;
    float3 normalW = bary.x * patch[0].normalW + bary.y * patch[1].normalW + bary.z * patch[2].normalW;
    float3 tangentW = bary.x * patch[0].tangentW + bary.y * patch[1].tangentW + bary.z * patch[2].tangentW;
    float3 binormalW = bary.x * patch[0].binormalW + bary.y * patch[1].binormalW + bary.z * patch[2].binormalW;
    float2 uv = bary.x * patch[0].uv + bary.y * patch[1].uv + bary.z * patch[2].uv;

    normalW = normalize(normalW);

    ObjectData obj = g_Objects[drawIndex];
    PhongTessMaterial mat = g_Materials[obj.materialId];

    float displacement = DISP * (g_Tex2D[mat.displacementMap].SampleLevel(AnisoWrap, uv, 0).x - 0.5);

    pos += displacement * normalW;
    o.pos = mul(viewProj, float4(pos, 1.0));
    o.normalW = normalW;
    o.tangentW = tangentW;
    o.binormalW = binormalW;
    o.uv = uv;

    return o;
}


GBufferOut frag(Varyings i) {
    PhongTessMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    float4 tex = g_Tex2D[mat.albedoTex].Sample(AnisoWrap, i.uv);
    clip(tex.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float3 normalTex = g_Tex2D[mat.normalTex].Sample(AnisoWrap, i.uv).xyz * 2.0 - 1.0;

    #if OPENGL_NORMAL
    normalTex.y = -normalTex.y;
    #endif

    float3 normal = normalize(mul(normalTex, tbnMat));

    GBufferOut o;
    o.albedo = float4(tex.rgb * mat.diffuseColor * mat.diffuseK, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(mat.specularK, saturate(mat.shininess / 255.0), 0, 0);
    o.emissive = 0;
    return o;
}
