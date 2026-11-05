cbuffer LightingPass : register(b2) {
    uint albedoTex;
    uint normalTex;
    uint materialTex;
    uint depthTex;
    uint velocityTex;
    uint dirCount;
    uint debugView;
    uint skyTex;
    float3 ambientColor;
    float _alsoPad;
    Cascades csm;
    uint irradianceTex;
    uint specularTex;
    uint brdfLutTex;
    float specularMips;
    uint useAtmosphere;
    float3 betaRayleigh;
    float hMie;
    float3 betaMie;
    float mieG;
    uint3 _lPad;
};

struct Surface {
    float3 pos;
    float3 normal;
    float3 albedo;
    #ifdef PBR
    float roughness;
    float metallic;
    float ao;
    float2 velocity;
    #else
    float specular;
    float shininess;
    #endif
};

bool LoadSurface(float2 pixel, out Surface s) {
    s = (Surface) 0;
    int3 px = int3(pixel, 0);
    float depth = g_Tex2D[depthTex].Load(px).r;
    if (depth == 0.0)
        return false;
    s.albedo = g_Tex2D[albedoTex].Load(px).rgb;
    s.normal = normalize(g_Tex2D[normalTex].Load(px).xyz * 2 - 1);
    float4 m = g_Tex2D[materialTex].Load(px);
    #ifndef PBR
    s.specular = m.r;
    s.shininess = m.g * 255.0;
    #else
    s.roughness = m.r;
    s.metallic = m.g;
    s.ao = m.b;
    s.velocity = g_Tex2D[velocityTex].Load(px).xy;
    #endif
    s.pos = WorldPosFromDepth(pixel / resolution, depth);
    return true;
}