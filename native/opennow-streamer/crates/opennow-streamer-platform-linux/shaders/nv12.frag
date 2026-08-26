#version 450

layout(set = 0, binding = 0) uniform sampler2D luma_texture;
layout(set = 0, binding = 1) uniform sampler2D chroma_texture;

layout(push_constant) uniform Conversion {
    vec2 texture_scale;
    uint color_matrix;
    uint full_range;
} conversion;

layout(location = 0) in vec2 texture_coordinates;
layout(location = 0) out vec4 output_color;

void main() {
    vec2 source = (texture_coordinates - vec2(0.5)) * conversion.texture_scale + vec2(0.5);
    if (any(lessThan(source, vec2(0.0))) || any(greaterThan(source, vec2(1.0)))) {
        output_color = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }

    float y = texture(luma_texture, source).r;
    vec2 uv = texture(chroma_texture, source).rg;
    float u;
    float v;
    if (conversion.full_range != 0) {
        u = uv.x - 0.5;
        v = uv.y - 0.5;
    } else {
        y = max(0.0, y - (16.0 / 255.0)) * (255.0 / 219.0);
        u = (uv.x - (128.0 / 255.0)) * (255.0 / 224.0);
        v = (uv.y - (128.0 / 255.0)) * (255.0 / 224.0);
    }

    vec3 rgb;
    if (conversion.color_matrix == 0) {
        rgb = vec3(
            y + 1.402 * v,
            y - 0.344136 * u - 0.714136 * v,
            y + 1.772 * u
        );
    } else if (conversion.color_matrix == 2) {
        rgb = vec3(
            y + 1.4746 * v,
            y - 0.164553 * u - 0.571353 * v,
            y + 1.8814 * u
        );
    } else {
        rgb = vec3(
            y + 1.5748 * v,
            y - 0.187324 * u - 0.468124 * v,
            y + 1.8556 * u
        );
    }
    output_color = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}
