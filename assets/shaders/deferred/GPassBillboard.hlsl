#include "../lib/Core.hlsl"

struct ObjectData {
    float4x4 model;
    uint materialId;
    uint3 _pad;
};

struct BillboardMaterial {
    uint albedoTex;
    uint normalTex;
    uint materialTex;
    uint _texPad[5];

    float _pad[16];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
StructuredBuffer<ObjectData> g_Objects : register(t1);
StructuredBuffer<BillboardMaterial> g_Materials : register(t2);
SamplerState LinearClamp : register(s1);

struct Varyings {
    float3 center : POSITION;
    float radius : RADIUS;
    nointerpolation uint material : MATERIAL;
};

struct GSOut {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD;
    float3 right : RIGHT;
    float3 up : UP;
    float3 forward : FORWARD;
    nointerpolation uint material : MATERIAL;
};

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

Varyings vert(uint vid : SV_VertexID) {
    ObjectData obj = g_Objects[drawIndex + vid];
    Varyings o;
    o.center = mul(obj.model, float4(0, 0, 0, 1)).xyz;
    o.radius = length(mul(obj.model, float4(1, 0, 0, 0)).xyz);
    o.material = obj.materialId;
    return o;
}

[maxvertexcount(4)]
void geom(point Varyings i[1], inout TriangleStream<GSOut> stream) {
    float3 fwd = normalize(cameraPos - i[0].center);
    float3 right;
    float3 up;
    BillboardTowards(fwd, right, up);
    [unroll]
    for (int k = 0; k < 4; k++) {
        GSOut o;
        float3 w = i[0].center + (right * QUAD_CORNERS[k].x + up * QUAD_CORNERS[k].y) * i[0].radius;
        o.pos = mul(viewProj, float4(w, 1));
        o.uv = QUAD_UVS[k];
        o.right = right;
        o.up = up;
        o.forward = fwd;
        o.material = i[0].material;
        stream.Append(o);
    }
}

GBufferOut frag(GSOut i) {
    BillboardMaterial mat = g_Materials[i.material];
    float4 tex = g_Tex2D[mat.albedoTex].Sample(LinearClamp, i.uv);
    clip(tex.a - 0.5);
    float3 nv = g_Tex2D[mat.normalTex].Sample(LinearClamp, i.uv).xyz * 2 - 1;
    float3 n = normalize(i.right * nv.x + i.up * nv.y - i.forward * nv.z);
    float4 texMat = g_Tex2D[mat.materialTex].Sample(LinearClamp, i.uv);

    GBufferOut o;
    o.albedo = float4(tex.rgb, 1);
    o.normal = float4(n * 0.5 + 0.5, 0);
    o.material = texMat;
    o.emissive = 0;
    return o;
}
