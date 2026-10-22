struct Particle {
    float3 pos;
    float lifetimeSeconds;
    float3 velocity;
    float size;
    float3 color;
    float maxLifetime;
};

#define DEAD_COUNTER 0
#define SORT_COUNTER 4096
#define SNAPSHOT 8192

static const float2 QUAD_CORNERS[4] = { float2(-1, -1), float2(-1, 1), float2(1, -1), float2(1, 1) };
static const float2 QUAD_UVS[4] = { float2(0., 1.), float2(0., 0.), float2(1, 1.), float2(1., 0.) };

void BillboardTowards(float3 forward, out float3 right, out float3 up) {
    float3 rUp = abs(forward.y) < 0.99 ? float3(0, 1, 0) : float3(1, 0, 0);
    right = normalize(cross(forward, rUp));
    up = cross(right, forward);
}

// https://en.wikipedia.org/wiki/Permuted_congruential_generator
uint Hash(uint x) {
    x = x * 747796405u + 2891336453u;
    uint w = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (w >> 22u) ^ w;
}

float Rand(inout uint seed) {
    seed = Hash(seed);
    // accounting for mantissa
    return (seed >> 8) * (1.0 / 16777216.0);
}
