struct GPBRVaryings {
    float4 pos : SV_Position;
    float4 currClip : CURR_CLIP;
    float4 prevClip : PREV_CLIP;
    float3 normalW : NORMAL;
    float3 tangentW : TANGENT;
    float3 binormalW : BINORMAL;
    float2 uv : TEXCOORD;
};

GBufferOut frag(GPBRVaryings i) {
    ObjectData obj = g_Objects[drawIndex];
    PbrMaterial mat = g_Materials[g_Objects[drawIndex].materialId];
    float4 albedo = g_Tex2D[mat.albedoTex].SampleBias(AnisoWrap, i.uv, mipBias);
    clip(albedo.a - 0.5);

    float3x3 tbnMat = float3x3(normalize(i.tangentW), normalize(i.binormalW), normalize(i.normalW));
    float4 texNormal = g_Tex2D[mat.normalTex].SampleBias(AnisoWrap, i.uv, mipBias) * 2.0 - 1.0;
    float3 normal = normalize(mul(texNormal.xyz, tbnMat));

    float4 roughnessMetallic = g_Tex2D[mat.metalRoughnessTex].SampleBias(AnisoWrap, i.uv, mipBias);
    float4 occlusion = g_Tex2D[mat.occlusionTex].SampleBias(AnisoWrap, i.uv, mipBias);

    GBufferOut o;
    o.albedo = float4(albedo.rgb * mat.baseColor, 1);
    o.normal = float4(normal * 0.5 + 0.5, 0);
    o.material = float4(roughnessMetallic.g * mat.roughness, roughnessMetallic.b * mat.metallic, occlusion.r, 0);
    o.emissive = 0;
    o.velocity = GetMotionVector(i.currClip, i.prevClip);
    return o;
}