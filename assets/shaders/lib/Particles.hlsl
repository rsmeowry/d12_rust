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
