Texture2D<uint4> source_y410 : register(t0);
Texture2D<float> source_luma : register(t1);
Texture2D<float2> source_chroma : register(t2);
cbuffer Quantization : register(b0) { float4 scale_bias; float4 yuv_coefficients; };
float4 vertex_main(uint id : SV_VertexID) : SV_Position {
    float2 corner = float2((id << 1) & 2, id & 2);
    return float4(corner * float2(2, -2) + float2(-1, 1), 0, 1);
}
float4 convert_yuv(float3 encoded) {
    float y = encoded.y * scale_bias.x + scale_bias.y;
    float u = encoded.x * scale_bias.z + scale_bias.w;
    float v = encoded.z * scale_bias.z + scale_bias.w;
    return float4(saturate(float3(y + yuv_coefficients.x * v,
        y + yuv_coefficients.y * u + yuv_coefficients.z * v, y + yuv_coefficients.w * u)), 1);
}
float4 pixel_main(float4 position : SV_Position) : SV_Target {
    return convert_yuv(float3(source_y410.Load(int3(position.xy, 0)).xyz));
}
float4 centered_main(float4 position : SV_Position) : SV_Target {
    uint width, height;
    source_chroma.GetDimensions(width, height);
    float2 chroma_position = position.xy * 0.5 - 0.5;
    int2 first = int2(floor(chroma_position));
    int2 last = int2(width, height) - 1;
    float2 weight = frac(chroma_position);
    float2 upper = lerp(source_chroma.Load(int3(clamp(first, 0, last), 0)),
        source_chroma.Load(int3(clamp(first + int2(1, 0), 0, last), 0)), weight.x);
    float2 lower = lerp(source_chroma.Load(int3(clamp(first + int2(0, 1), 0, last), 0)),
        source_chroma.Load(int3(clamp(first + int2(1, 1), 0, last), 0)), weight.x);
    float2 chroma = lerp(upper, lower, weight.y);
    return convert_yuv(float3(chroma.x, source_luma.Load(int3(position.xy, 0)), chroma.y));
}
