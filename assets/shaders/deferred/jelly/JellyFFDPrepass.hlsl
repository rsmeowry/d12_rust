#include "../../lib/Core.hlsl"
#include "../../lib/FFD.hlsl"

float frag(Varyings i) : SV_Target {
    return distance(i.posW, cameraPos);
}