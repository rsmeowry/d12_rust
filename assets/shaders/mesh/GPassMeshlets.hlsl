#include "../lib/Core.hlsl"
#include "../lib/GBuffer.hlsl"
#include "../lib/GPassPBRCore.hlsl"
#define AS_GROUP 32
struct Meshlet {
    uint vertexOffset;
    uint triangleOffset;
    uint vertexCount;
    uint triangleCount;
    float3 center;
    float radius;
    float3 coneAxis;
    float coneCutoff;
};

StructuredBuffer<Attributes> g_Vertices : register(t3);
ByteAddressBuffer g_Meshlets : register(t4);

struct Payload {
    uint ids[AS_GROUP];
};
groupshared Payload s_payload;

bool IsVisible(Meshlet m) {
    ObjectData obj = g_Objects[drawIndex];
    float3x3 m3 = (float3x3)obj.model;
    float s = sqrt(max(max(dot(m3._11_21_31, m3._11_21_31),
                           dot(m3._12_22_32, m3._12_22_32)),
                           dot(m3._13_23_33, m3._13_23_33)));
    float3 c = mul(obj.model, float4(m.center, 1)).xyz;
    float r = m.radius * s;

    float4x4 vp = viewProjNj;
    float4 planes[5] = {
        vp[3] + vp[0],
        vp[3] - vp[0],
        vp[3] + vp[1],
        vp[3] - vp[1],
        vp[3] - vp[2]
    };
    [unroll]
     for (uint i = 0; i < 5; i++)
        if (dot(planes[i].xyz, c) + planes[i].w < -r * length(planes[i].xyz))
            return false;

    float3 axis = normalize(mul(m3, m.coneAxis));
    float3 v = c - cameraPos;
    return dot(v, axis) < m.coneCutoff * length(v) + r;
}

[numthreads(AS_GROUP, 1, 1)]
void amp(uint dtid : SV_DispatchThreadID) {
    uint count = g_Meshlets.Load(0);
    bool visible = false;
    if (dtid < count)
        visible = IsVisible(g_Meshlets.Load<Meshlet>(16 + dtid * 48));
    if (visible)
        s_payload.ids[WavePrefixCountBits(visible)] = dtid;
    uint visibleCount = WaveActiveCountBits(visible);
    if (WaveIsFirstLane())
        g_RWScratch.InterlockedAdd(0, visibleCount);
    DispatchMesh(WaveActiveCountBits(visible), 1, 1, s_payload);
}

[outputtopology("triangle")]
[numthreads(128, 1, 1)]
void mesh(uint gtid : SV_GroupThreadID, uint gid : SV_GroupID, out vertices GPBRVaryings verts[64], out indices uint3 tris[124], in payload Payload p) {
    uint mi = p.ids[gid];
    uint4 hdr = g_Meshlets.Load4(0);
    Meshlet m = g_Meshlets.Load<Meshlet>(16 + mi * 48);
    SetMeshOutputCounts(m.vertexCount, m.triangleCount);

    if (gtid < m.vertexCount) {
        uint vi = g_Meshlets.Load(hdr.y + (m.vertexOffset + gtid) * 4);
        verts[gtid] = vert(g_Vertices[vi]);
        verts[gtid].meshletId = mi;
    }
    if (gtid < m.triangleCount) {
        uint pos = g_Meshlets.Load(hdr.z + (m.triangleOffset + gtid) * 4);
        tris[gtid] = uint3(pos & 0xFF, (pos >> 8) & 0xFF, (pos >> 16) & 0xFF);
    }
}