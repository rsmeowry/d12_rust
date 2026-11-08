#include "../lib/Core.hlsl"
#include "../lib/GBuffer.hlsl"
#include "../lib/GPassPBRCore.hlsl"

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

[outputtopology("triangle")]
[numthreads(128, 1, 1)]
void ms(uint gtid : SV_GroupThreadID, uint gid : SV_GroupID, out vertices GPBRVaryings verts[64], out indices uint3 tris[124]) {
    uint4 hdr = g_Meshlets.Load4(0);
    Meshlet m = g_Meshlets.Load<Meshlet>(16 + gid * 48);
    SetMeshOutputCounts(m.vertexCount, m.triangleCount);

    if (gtid < m.vertexCount) {
        uint vi = g_Meshlets.Load(hdr.y + (m.vertexOffset + gtid) * 4);
        verts[gtid] = vert(g_Vertices[vi]);
        verts[gtid].meshletId = gid;
    }
    if (gtid < m.triangleCount) {
        uint p = g_Meshlets.Load(hdr.z + (m.triangleOffset + gtid) * 4);
        tris[gtid] = uint3(p & 0xFF, (p >> 8) & 0xFF, (p >> 16) & 0xFF);
    }
}