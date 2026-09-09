Texture2D<uint4> source_y410 : register(t0);
cbuffer Quantization : register(b0) { float4 scale_bias; };
float4 vertex_main(uint id : SV_VertexID) : SV_Position {
    float2 corner = float2((id << 1) & 2, id & 2);
    return float4(corner * float2(2, -2) + float2(-1, 1), 0, 1);
}
float4 pixel_main(float4 position : SV_Position) : SV_Target {
    float3 encoded = float3(source_y410.Load(int3(position.xy, 0)).xyz);
    float y = encoded.y * scale_bias.x + scale_bias.y;
    float u = encoded.x * scale_bias.z + scale_bias.w;
    float v = encoded.z * scale_bias.z + scale_bias.w;
    return float4(saturate(float3(y + 1.5748 * v,
        y - 0.1873242729 * u - 0.4681242729 * v, y + 1.8556 * u)), 1);
}
