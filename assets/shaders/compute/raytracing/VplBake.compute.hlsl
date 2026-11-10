// we gotta bake ALL the lights, one light at a time
// casting rays from each light, reading them back on cpu
// then we can reuse generated flux

#include "../../lib/Core.hlsl"
#include "../../lib/Raytracing.hlsl"

#define VPL_DIR 0
#define VPL_POINT 1
#define VPL_SPOT 2
#define VPL_STRIDE 48

cbuffer RTBakePass : register(b2) {
    float3 lightPos;
    uint lightType;
    float3 lightDir;
    float spotOuterCos;
    float3 baseFlux; // lumen
    float3 sceneCenter; // just for directional
    float sceneRadius;
    uint maxVpl;
    uint pathIndex;
    uint vplBuf;
    uint maxLevels;
    uint seed;
    float albedoMip;
    float3 _pad;
}

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

StructuredBuffer<PbrMaterial> g_Materials : register(t2);
SamplerState LinearWrap : register(s0);

// for writing vplBuf
void WriteVpl(uint path, uint level, float3 pos, float3 norm, float3 intensity) {
    RWByteAddressBuffer o = g_RWBuffers[vplBuf];
    uint off = (path * maxLevels + level) * VPL_STRIDE;
    o.Store4(off, uint4(asuint(pos), level));
    o.Store4(off + 16, uint4(asuint(norm), 0));
    o.Store4(off + 32, uint4(asuint(intensity), 0));
}

// random
static uint g_Rng;
float Rand() {
    g_Rng = PcgHash(g_Rng);
    return (g_Rng >> 8) * (1.0 / 16777216.0);
}

// hitting
struct SceneHit {
    bool didHit;
    uint instanceId;
    uint primitiveId;
    float3 bary;
    float depth;
};

bool TraceScene(float3 ro, float3 rd, out SceneHit hit) {
    RayDesc desc;
    desc.Origin = ro;
    desc.TMin = 1e-3;
    desc.TMax = 100.;
    desc.Direction = rd;

    RayQuery<RAY_FLAG_FORCE_OPAQUE> q;
    q.TraceRayInline(g_Tlas, RAY_FLAG_NONE, 0xFF, desc);
    while(q.Proceed()) {
        // bwomp bwomp
    }
    hit = (SceneHit) 0;
    hit.didHit = q.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    if(!hit.didHit) {
        return false;
    }

    hit.depth = q.CommittedRayT();
    float2 barycentrics = q.CommittedTriangleBarycentrics();
    hit.bary = float3(1.0 - barycentrics.x - barycentrics.y, barycentrics.x, barycentrics.y);
    hit.primitiveId = q.CommittedPrimitiveIndex();
    hit.instanceId = q.CommittedInstanceIndex();
    return true;
}

// sampling light sources
float3 SamplePointSphere(float2 rand) {
    float z = 1 - 2 * rand.x;
    float r = sqrt(max(0, 1 - z * z));
    float phi = 2 * PI * rand.y;
    return float3(r * cos(phi), r * sin(phi), z);
}

float3 SampleSpotCone(float2 rand, float cosMax) {
    float z = lerp(1, cosMax, rand.x);
    float r = sqrt(max(0, 1 - z * z));
    float phi = 2 * PI * rand.y;
    return float3(r * cos(phi), r * sin(phi), z);
}

float3 SampleCosine(float2 u) {
    float r = sqrt(u.x);
    float phi = 2 * PI * u.y;
    return float3(r * cos(phi), r * sin(phi), sqrt(max(0, 1 - u.x)));
}

// other utilities

// https://www.jcgt.org/published/0006/01/01/paper-lowres.pdf
void OrthonormedBasis(float3 n, out float3 tangent, out float3 binormal) {
    if(n.z<0.){
        const float a = 1.0f / (1.0f - n.z);
        const float b = n.x * n.y * a;
        tangent = float3(1.0f - n.x * n.x * a, -b, n.x);
        binormal = float3(b, n.y * n.y*a - 1.0f, -n.y);
    }
    else{
        const float a = 1.0f / (1.0f + n.z);
        const float b = -n.x * n.y * a;
        tangent = float3(1.0f - n.x * n.x * a, b, -n.x);
        binormal = float3(b, 1.0f - n.y * n.y * a, -n.y);
    }
}

float3 ConvertToWorld(float3 vec, float3 norm) {
    float3 tangent;
    float3 binormal;
    OrthonormedBasis(norm, tangent, binormal);
    return vec.x * tangent + vec.y * binormal + vec.z * norm;
}

[numthreads(8, 8, 1)]
void CS(int3 threadId: SV_DispatchThreadID) {
    uint pathCount = maxVpl / maxLevels;
    if(threadId.x >= pathCount)
        return; // STOP !!!!

    g_Rng = PcgHash(threadId.x ^ PcgHash(seed));
    float3 origin;
    float3 dir;
    float2 rnd = float2(Rand(), Rand());
    if (lightType == VPL_POINT) {
        origin = lightPos;
        dir = SamplePointSphere(rnd);
    } else if(lightType == VPL_SPOT) {
        origin = lightPos;
        dir = ConvertToWorld(SampleSpotCone(rnd, spotOuterCos), lightDir);
    } else {
        // for directional light (sun)
        float3 tangent;
        float3 binormal;
        OrthonormedBasis(lightDir, tangent, binormal);
        float rad = sceneRadius * sqrt(rnd.x);
        float phi = PI * 2 * rnd.y;
        dir = lightDir;
        origin = sceneCenter - dir * (2 * sceneRadius) + (tangent * cos(phi) + binormal * sin(phi)) * rad;
    }

    float3 flux = baseFlux;

    // cant unroll bwa
    for (uint level = 0; level < maxLevels; level += 1) {
        SceneHit hit;
        if(!TraceScene(origin, dir, hit))
            break;

        ObjectData hitObj = g_Objects[hit.instanceId];
        PbrMaterial hitMat = g_Materials[hitObj.materialId];

        float3 norm;
        float2 uv;
        LoadNormalUv(hit.primitiveId, hit.bary, hitObj, norm, uv);
        norm = normalize(mul((float3x3) hitObj.model, norm)); // probably should also sample normals tex
        if (dot(norm, dir) > 0)
            norm = -norm;  // two-sided geometry

        float3 pos = origin + dir * hit.depth;
        float3 albedo = g_Tex2D[NonUniformResourceIndex(hitMat.albedoTex)].SampleLevel(LinearWrap, uv, albedoMip).rgb * hitMat.baseColor;
        float metallic = g_Tex2D[NonUniformResourceIndex(hitMat.metalRoughnessTex)].SampleLevel(LinearWrap, uv, albedoMip).g * hitMat.metallic;
        float3 rho = albedo * (1 - metallic);

        flux *= rho;

        WriteVpl(pathIndex, level, pos, norm, flux / PI); // lambertian diffuse uses I = phi/PI

        // LETS GO GAMBLING
        float p = clamp((rho.r + rho.g + rho.b) / 3., 0.05, 0.95);
        if (Rand() >= p)
            break;
        flux /= p; // extinction kinda

        origin = pos + norm * 1e-3; // slightly offset not to get lost in geometry
        dir = ConvertToWorld(SampleCosine(float2(Rand(), Rand())), norm);
    }
}