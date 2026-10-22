#include "../lib/Core.hlsl"
#include "../lib/Particles.hlsl"

cbuffer ParticlesData : register(b2) {
    uint particleTex;
    uint3 _pad;
};

SamplerState LinearClamp : register(s1);
StructuredBuffer<Particle> g_Pool : register(t1);
StructuredBuffer<uint2> g_Sort : register(t2);

struct VSOut {
    float3 pos : POSITION;
    float size : SIZE;
    float3 color : COLOR;
};

struct GSOut {
    float4 pos : SV_Position;
    float2 uv : TEXCOORD;
    float3 color : COLOR;
    float3 right : RIGHT;
    float3 up : UP;
    float3 forward : FORWARD;
};

struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
};

VSOut vert(uint vid : SV_VertexID) {
    Particle p = g_Pool[g_Sort[vid].x];
    VSOut o;
    o.pos = p.pos;
    o.size = p.size * saturate(p.lifetimeSeconds / p.maxLifetime * 4);
    o.color = p.color;
    return o;
}

[maxvertexcount(4)]
void geom(point VSOut i[1], inout TriangleStream<GSOut> stream) {
    float3 fwd = normalize(cameraPos - i[0].pos);
    float3 right;
    float3 up;
    BillboardTowards(fwd, right, up);
    [unroll]
    for (int k = 0; k < 4; k++) {
        GSOut o;
        float3 w = i[0].pos + (right * QUAD_CORNERS[k].x + up * QUAD_CORNERS[k].y) * i[0].size;
        o.pos = mul(viewProj, float4(w, 1));
        o.uv = QUAD_UVS[k];
        o.color = i[0].color;
        o.right = right;
        o.up = up;
        o.forward = fwd;
        stream.Append(o);
    }
}

GBufferOut frag(GSOut i) {
    float4 col = g_Tex2D[particleTex].Sample(LinearClamp, i.uv);

    clip(col.a - 0.5);
    GBufferOut o;
    o.albedo = float4(col.rgb * i.color, 1);
    o.normal = float4(i.forward * 0.5 + 0.5, 0);
    o.material = float4(0.4, 0.0, 1.0, 0);
    o.emissive = float4(i.color * 0.3, 0);
    return o;
}