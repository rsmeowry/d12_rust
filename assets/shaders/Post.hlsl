#include <lib/Core.hlsl>


struct PostProperties {
    uint albedo;
    uint normals;
    uint materials;
    uint depth;
    uint sceneColor;
    uint enableSobel;
    uint enableVignette;
    float vignetteStrength;
    uint enablePosterization;
    uint posterizationLevels;
    uint _objPad[10];
};

cbuffer DrawConstants : register(b0) { uint drawIndex; }
Texture2D g_Tex2D[1024] : register(t0, space1);
SamplerState PointClamp : register(s2);
SamplerState Linear : register(s0);
StructuredBuffer<PostProperties> g_Props : register(t1);

struct Varyings { float4 pos : SV_Position; float2 uv : TEXCOORD; };

// generating a fake tri mesh here
Varyings vert(uint id : SV_VertexID) {
    Varyings o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = uv;
    return o;
}

float Luma(float3 c)
{
    return dot(c, float3(0.299, 0.587, 0.114));
}

float4 frag(Varyings i) : SV_Target {
    PostProperties props = g_Props[drawIndex];
    float2 t = float2(1.0, 1.0) / resolution;
    float2 pos = i.uv * resolution;
    Texture2D scene = g_Tex2D[props.sceneColor];
    Texture2D normals = g_Tex2D[props.normals];
    float4 source = scene.Sample(Linear, i.uv);

    // hdr to ldr
    source = source / (source + 1);

    // vignette
    if(props.enableVignette == 1) {
        source = source * pow(1 - distance(i.uv, float2(0.5, 0.5)), props.vignetteStrength);
    }

    // posterization
    if(props.enablePosterization == 1) {
        float level = props.posterizationLevels;
        source = floor(source * level) / level;
    }

    // sobel
    if(props.enableSobel == 1) {
        float tl = Luma(normals.Sample(Linear, i.uv + float2(-t.x, -t.y)).rgb);
        float t0 = Luma(normals.Sample(Linear, i.uv + float2(0.0, -t.y)).rgb);
        float tr = Luma(normals.Sample(Linear, i.uv + float2(t.x, -t.y)).rgb);

        float ml = Luma(normals.Sample(Linear, i.uv + float2(-t.x, 0.0)).rgb);
        float mr = Luma(normals.Sample(Linear, i.uv + float2(t.x, 0.0)).rgb);

        float bl = Luma(normals.Sample(Linear, i.uv + float2(-t.x, t.y)).rgb);
        float b0 = Luma(normals.Sample(Linear, i.uv + float2(0.0, t.y)).rgb);
        float br = Luma(normals.Sample(Linear, i.uv + float2(t.x, t.y)).rgb);

        float gx = -tl - 2.0 * ml - bl + tr + 2.0 * mr + br;
        float gy = -tl - 2.0 * t0 - tr + bl + 2.0 * b0 + br;

        float edge = sqrt(gx * gx + gy * gy);

        if (edge < 0.00) {
            edge = 0.0;
        }

        source = lerp(source, 0., edge);
    }
    return source;
}