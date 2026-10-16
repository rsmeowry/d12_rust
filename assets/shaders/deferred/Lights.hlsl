#include <../lib/Core.hlsl>
#include <../lib/Deferred.hlsl>
#include <../lib/Lighting.hlsl>
#include <../lib/Shadows.hlsl>

#define NO_SHADOW 0xFFFFFFFF

cbuffer DrawConstants : register(b0) { uint lightOffset; }
StructuredBuffer<Light> g_Lights : register(t3);

static const float SPHERE_SCALE = 1.2;
static const float CONE_SCALE = 1.1;

struct VolumeOut {
    float4 pos : SV_Position;
    nointerpolation uint light : LIGHT;
};

VolumeOut vert(float3 p : POSITION, uint iid : SV_InstanceID) {
    uint li = lightOffset + iid;
    Light l = g_Lights[li];

    float3 world;
    if (l.type == LIGHT_SPOT) {
        float3 d = l.direction;
        float3 up = abs(d.y) < 0.99 ? float3(0, 1, 0) : float3(1, 0, 0);
        float3 right = normalize(cross(up, d));
        up = cross(d, right);
        float r = l.spotRadius * CONE_SCALE;
        world = l.position + right * (p.x * r) + up * (p.y * r) + d * (p.z * l.range * CONE_SCALE);
    } else {
        world = l.position + p * (l.range * SPHERE_SCALE);
    }

    VolumeOut o;
    o.pos = mul(viewProj, float4(world, 1));
    o.light = li;
    return o;
}

float4 frag(VolumeOut i) : SV_Target {
    Surface s;
    if (!LoadSurface(i.pos.xy, s))
        return 0;
    Light l = g_Lights[i.light];
    float3 color = ShadeLight(l, s, normalize(cameraPos - s.pos));
    if (l.shadowTex != NO_SHADOW && any(color > 0)) {
        color *= l.type == LIGHT_SPOT ? SpotShadow(l, s.pos, s.normal, true) : l.type == LIGHT_POINT ? PointShadow(l, s.pos, s.normal, true) : 1.0;
    }
    return float4(color, 0);
}