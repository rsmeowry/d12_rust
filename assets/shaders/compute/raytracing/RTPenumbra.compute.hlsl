#include "../../lib/Core.hlsl"

cbuffer RTPenumbra : register(b2) {
    float2 passRes;
    uint rawTex;
    uint outTex;
    float tanSun; // tan sun rad
    float projScale; // proj[1][1]
    float maxRadius; // 16 maybe idk
    uint _pad;
}

float PenumbraPx(float hitT, float viewDepth) {
    return hitT * tanSun * projScale * passRes.y / (2 * viewDepth);
}

static const int2 DIRS[4] = {
    int2(1, 0), int2(-1, 0),
    int2(0, 1), int2(0, -1)
};

[numthreads(8, 8, 1)]
void CS(uint3 id : SV_DispatchThreadID) {
    float4 c = g_Tex2D[rawTex].Load(int3(id.xy, 0));
    float radius = 0;

    if (c.z > 0) { // not sky
        if (c.x < 0.5 && c.y > 0) { // ... and in shadow
            radius = PenumbraPx(c.y, c.z);
        } else if (c.x >= 0.5) { // ... and lit, maybe penumbra?
            float best = maxRadius + 1;
            for (uint d = 0; d < 4; d++) {
                for (int s = 1; s <= (int) maxRadius; s++) {
                    int2 p = int2(id.xy) + DIRS[d] * s;
                    if (any(p < 0) || any(p >= int2(passRes))) break;
                    float4 n = g_Tex2D[rawTex].Load(int3(p, 0));
                    if (n.x < 0.5 && n.y > 0) {
                        float r = PenumbraPx(n.y, c.z);
                        if (s <= r && s < best) {
                            best = s;
                            radius = r;
                        } // got penumbra!
                        break; // dont care abt the rest
                    }
                }
            }
        }
    }
    g_RWTex2D[outTex][id.xy] = min(radius, maxRadius);
}