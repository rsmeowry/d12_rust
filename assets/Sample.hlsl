cbuffer Obj : register(b0) {
    float4x4 mvp;
};

struct Attributes {
    float3 pos: POSITION;
    float4 col: COLOR;
};

struct Varyings {
    float4 pos: SV_Position;
    float4 col: COLOR;
};

Varyings vert(Attributes attr) {
    Varyings v;
    v.pos = mul(float4(attr.pos, 1.), mvp);
    v.col = attr.col;
    return v;
}

float4 frag(Varyings v) : SV_Target {
    return v.col;
}
