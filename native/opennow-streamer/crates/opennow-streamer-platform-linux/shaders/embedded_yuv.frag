#version 450

layout(set = 0, binding = 0) uniform sampler2D luma_texture;
layout(set = 0, binding = 1) uniform sampler2D chroma_texture;

layout(push_constant) uniform Conversion {
    vec2 texture_scale;
    uint color_matrix;
    uint full_range;
    uint sample_bits;
    float chroma_offset_x;
} conversion;

layout(location = 0) in vec2 texture_coordinates;
layout(location = 0) out vec4 output_color;

void main() {
    vec2 source = (texture_coordinates - vec2(0.5)) * conversion.texture_scale + vec2(0.5);
    float y = texture(luma_texture, source).r;
    vec2 uv = texture(chroma_texture, source + vec2(conversion.chroma_offset_x, 0.0)).rg;
    bool ten_bit = conversion.sample_bits != 8;
    if (conversion.sample_bits == 16) {
        y *= 65535.0 / 65472.0;
        uv *= 65535.0 / 65472.0;
    }
    float maximum = ten_bit ? 1023.0 : 255.0;
    float multiplier = ten_bit ? 4.0 : 1.0;
    float midpoint = 128.0 * multiplier / maximum;
    float u = uv.x - midpoint;
    float v = uv.y - midpoint;
    if (conversion.full_range == 0) {
        y = max(0.0, y - 16.0 * multiplier / maximum) * maximum / (219.0 * multiplier);
        u *= maximum / (224.0 * multiplier);
        v *= maximum / (224.0 * multiplier);
    }
    vec3 rgb;
    if (conversion.color_matrix == 0) {
        rgb = vec3(y + 1.402 * v, y - 0.344136 * u - 0.714136 * v, y + 1.772 * u);
    } else if (conversion.color_matrix == 2) {
        rgb = vec3(y + 1.4746 * v, y - 0.164553 * u - 0.571353 * v, y + 1.8814 * u);
    } else {
        rgb = vec3(y + 1.5748 * v, y - 0.187324 * u - 0.468124 * v, y + 1.8556 * u);
    }
    output_color = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}
