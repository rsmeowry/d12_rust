#define VIEW_STEPS 16
#define SUN_STEPS 8
#define R_EARTH 6360.
#define H_R 8
#define SUN_INTENSITY 19.5
#define ATMOSPHERE_EXIT 6420
#define WORLD_TO_KM 0.001

float RayleighPhase(float mu) {
    return (3 / (16 * PI)) * (1 + mu * mu);
}

float RaySphereExit(float3 ro, float3 rd, float r) {
    float b = dot(ro, rd);
    float c = dot(ro, ro) - r * r;
    return -b + sqrt(max(b * b - c, 0));
}

float Density(float3 p) {
    return exp(-max(length(p) - R_EARTH, 0) / H_R);
}

float OpticalDepthToSun(float3 origin, float3 dirToSun) {
    float accumDepth = 0;
    float dt = RaySphereExit(origin, dirToSun, ATMOSPHERE_EXIT) / SUN_STEPS;
    for(int i = 0; i < SUN_STEPS; i++) {
        accumDepth += Density(origin + dirToSun * (i + 0.5) * dt) * dt;
    }
    return accumDepth;
}

float3 AtmosphereOrigin() {
    return float3(0, R_EARTH + max(cameraPos.y * WORLD_TO_KM, 0.001), 0);
}

float3 Atmosphere(float3 roAtmS, float3 rd, float tMax, float3 betaR, float3 sunDir, out float3 T) {
    float dt = tMax / VIEW_STEPS;
    float opticalDepth = 0;
    float3 sumR = 0;

    for(int i = 0; i < VIEW_STEPS; i++) {
        float3 pos = roAtmS + rd * (i + 0.5) * dt;

        // optical depth
        float d = Density(pos) * dt;
        opticalDepth += d;
        float odSun = OpticalDepthToSun(pos, -sunDir);
        float3 tau = betaR * (opticalDepth + odSun);
        float3 att = exp(-tau);
        sumR += d * att;
    }

    T = exp(-(betaR * opticalDepth));
    float mu = dot(rd, -sunDir);
    return SUN_INTENSITY * (sumR * betaR * RayleighPhase(mu));
}
