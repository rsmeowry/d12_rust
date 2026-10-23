cbuffer SortPass : register(b2) {
    uint j;
    uint k;
    uint2 _pad;
};

RWStructuredBuffer<uint2> g_SortList : register(u2, space4);

[numthreads(256, 1, 1)]
void CS(uint i : SV_DispatchThreadID) {
    uint l = i ^ j;
    if(l <= i)
        return;
    uint2 a = g_SortList[i];
    uint2 b = g_SortList[l];
    bool ascending = (i & k) == 0;
    if ((asfloat(a.y) > asfloat(b.y)) == ascending) {
        g_SortList[i] = b;
        g_SortList[l] = a;
    }
}
