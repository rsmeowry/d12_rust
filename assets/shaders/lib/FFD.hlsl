cbuffer DrawConstants : register(b0) { uint drawIndex; }

cbuffer FFDPoints : register(b3) {
    float4 points[64]; // idx is x + 4 * (y + 4 * z)
}

struct VertexFFD {
    float3 pos : POSITION; // stu
    float3 normal : NORMAL;
    float2 uv : TEXCOORD;
};

struct Varyings {
    float4 pos : SV_Position;
    float3 posW : WORLD_POS;
    float3 normalW : NORMAL;
    float2 uv : TEXCOORD;
};

float4 Bern(float t) {
    float m = 1 - t;
    return float4(m*m*m, 3*t*m*m, 3*t*t*m, t*t*t);
}

float4 dBern(float t) {
    float m = 1 - t;
    return float4(-3*m*m, 3*m*m - 6*t*m, 6*t*m - 3*t*t, 3*t*t);
}

static uint g_Rng;

float Rand() {
    g_Rng = PcgHash(g_Rng);
    return (g_Rng >> 8) * (1.0 / 16777216.0);
}

#define DISP_AMP 0.03
#define DISP_K 5
#define DISP_FREQ 1.2
#define DISP_PHASE 6.
#define DISP_DIR float3(0, 1, 0)

Varyings vert(VertexFFD v) {
    ObjectData obj = g_Objects[drawIndex];

    float4 bs = Bern(v.pos.x);
    float4 bt = Bern(v.pos.y);
    float4 bu = Bern(v.pos.z);

    float4 ds = dBern(v.pos.x);
    float4 dt = dBern(v.pos.y);
    float4 du = dBern(v.pos.z);

    float3 pos = 0, a = 0, b = 0, c = 0;
    [unroll]
    for (int k = 0; k < 4; k++) {
        [unroll]
        for (int j = 0; j < 4; j++) {
            [unroll]
            for (int i = 0; i < 4; i++)
            {
                float3 p = points[i + 4*(j + 4*k)].xyz;
                pos += bs[i]*bt[j]*bu[k] * p;
                a += ds[i]*bt[j]*bu[k] * p;
                b += bs[i]*dt[j]*bu[k] * p;
                c += bs[i]*bt[j]*du[k] * p;
            }
        }
    }
    float3 nrm = normalize(v.normal.x * cross(b, c) + v.normal.y * cross(c, a) + v.normal.z * cross(a, b));
    float4 posW = mul(obj.model, float4(pos, 1.0));

    Varyings o;
    o.posW = posW.xyz;
    o.pos = mul(viewProj, posW);
    o.normalW = mul((float3x3) obj.model, nrm);
    o.uv = v.uv;
    return o;
}