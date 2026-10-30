#define VIEW_STEPS 16
#define SUN_STEPS 8
#define R_EARTH 6360.
#define H_R 8
#define SUN_INTENSITY 19.5
#define ATMOSPHERE_EXIT 6420
#define WORLD_TO_KM 0.001
#define GROUND_ALBEDO float3(0.2, 0.3, 0.6)

// ground dist or -1 if miss
float RayGroundHit(float3 ro, float3 rd) {
    float b = dot(ro, rd);
    float c = dot(ro, ro) - R_EARTH * R_EARTH;
    float disc = b * b - c;
    if (b > 0 || disc < 0)
        return -1;
    return -b - sqrt(disc);
}

float MiePhase(float mu, float g) {
    float g2 = g * g;
    float denom = 1.0 + g2 - 2.0 * g * mu;
    return (1.0 - g2) / (4.0 * PI * denom * sqrt(denom));
}

float RayleighPhase(float mu) {
    return (3 / (16 * PI)) * (1 + mu * mu);
}

float RaySphereExit(float3 ro, float3 rd, float r) {
    float b = dot(ro, rd);
    float c = dot(ro, ro) - r * r;
    return -b + sqrt(max(b * b - c, 0));
}

float2 Density(float3 p) {
    float h = max(length(p) - R_EARTH, 0);
    return exp(-h / float2(H_R, hMie));
}

float3 Extinction(float2 od) {
    return betaRayleigh * od.x + betaMie * 1.1 * od.y;
}

float2 OpticalDepthToSun(float3 origin, float3 dirToSun) {
    float2 od = 0;
    float dt = RaySphereExit(origin, dirToSun, ATMOSPHERE_EXIT) / SUN_STEPS;
    for (int i = 0; i < SUN_STEPS; i++)
        od += Density(origin + dirToSun * (i + 0.5) * dt) * dt;
    return od;
}

float3 AtmosphereOrigin() {
    return float3(0, R_EARTH + max(cameraPos.y * WORLD_TO_KM, 0.001), 0);
}

float3 Atmosphere(float3 ro, float3 rd, float tMax, float3 betaR, float3 sunDir, out float3 T) {
    float3 toSun = -sunDir;
    float mu = dot(rd, toSun);
    float phaseR = RayleighPhase(mu);
    float phaseM = MiePhase(mu, mieG);

    float dt = tMax / VIEW_STEPS;
    float3 inscatter = 0;
    T = 1;

    for (int i = 0; i < VIEW_STEPS; i++) {
        float3 pos = ro + rd * (i + 0.5) * dt;
        float2 dens = Density(pos);
        float3 sigmaR = betaR * dens.x;
        float3 sigmaM = betaMie * dens.y;
        float3 sigmaT = sigmaR + sigmaM * 1.1;

        float3 sunT = exp(-Extinction(OpticalDepthToSun(pos, toSun)));
        float3 S = sunT * (sigmaR * phaseR + sigmaM * phaseM);

        float3 stepT = exp(-sigmaT * dt);
        inscatter += T * S * (1 - stepT) / max(sigmaT, 1e-7);
        T *= stepT;
    }
    return SUN_INTENSITY * inscatter;
}