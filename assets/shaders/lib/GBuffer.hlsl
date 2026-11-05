struct GBufferOut {
    float4 albedo : SV_Target0;
    float4 normal : SV_Target1;
    float4 material : SV_Target2;
    float4 emissive : SV_Target3;
    float2 velocity : SV_Target4;
};

float2 GetMotionVector(float4 currentClip, float4 prevClip) {
    return (prevClip.xy / prevClip.w - currentClip.xy / currentClip.w) * float2(0.5, -0.5);
}