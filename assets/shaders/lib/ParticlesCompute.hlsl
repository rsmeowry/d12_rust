cbuffer ParticlePass : register(b2) {
    uint depthTex;
    uint normalTex;
    uint countersBuf;

    uint maxParticles;
    float3 emitterPos;
    uint emitCount;

    float3 gravity;
    float dt;
    float3 colorA;
    float drag;
    float3 colorB;
    float sortOrder; // +z or -z
    float2 lifeRange;
    float2 sizeRange;
    float3 velBase;

    float spread;
    float restitution;
    float thickness;

    uint2 _pad;
};

Particle SpawnParticle(uint pId) {
    uint s = Hash(pId + Hash(asuint(time)));
    float3 r = float3(Rand(s), Rand(s), Rand(s)) * 2 - 1;
    Particle p;
    p.pos = emitterPos;
    p.velocity = velBase + r * spread;
    p.maxLifetime = lerp(lifeRange.x, lifeRange.y, Rand(s));
    p.lifetimeSeconds = p.maxLifetime;
    p.size = lerp(sizeRange.x, sizeRange.y, Rand(s));
    p.color = lerp(colorA, colorB, Rand(s));
    return p;
}
