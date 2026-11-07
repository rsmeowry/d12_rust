
RaytracingAccelerationStructure g_Tlas : register(t4);

bool TraceShadow(float3 ro, float3 dir, float tMax, out float hitT) {
    RayDesc ray;
    ray.Origin = ro;
    ray.Direction = dir;
    ray.TMax = tMax;
    ray.TMin = 0.001;
    RayQuery<RAY_FLAG_FORCE_OPAQUE> q;
    q.TraceRayInline(g_Tlas, RAY_FLAG_NONE, 0xFF, ray);
    while(q.Proceed()) {
        // bwomp bwomp
    }
    bool hit = q.CommittedStatus() == COMMITTED_TRIANGLE_HIT;
    hitT = hit ? q.CommittedRayT() : -1;
    return hit;
}
