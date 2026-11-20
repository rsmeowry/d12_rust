#include "../../lib/Core.hlsl"
#include "../../lib/FFD.hlsl"

cbuffer PeelConstants : register(b2)
{
    uint previousLayer;
    uint hasPreviousLayer;
    float depthEpsilon;
    float padding;
};

float2 frag(Varyings i): SV_Target
{
    uint2 pixel = uint2(i.pos.xy);

    float currentDepth = i.pos.z;

    if (hasPreviousLayer != 0)
    {
        float previousDepth =
            g_Tex2D[previousLayer].Load(int3(pixel, 0)).r;

        clip(previousDepth - currentDepth - depthEpsilon);
    }

    return float2(
        currentDepth,
        distance(i.posW, cameraPos)
    );
}