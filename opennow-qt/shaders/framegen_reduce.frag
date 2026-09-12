#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Parameters {
    vec4 geometry;
};
layout(binding = 1) uniform sampler2D sourceTexture;
void main()
{
    vec2 offset = geometry.xy * 0.25;
    fragColor = (texture(sourceTexture, uv + vec2(-offset.x, -offset.y))
               + texture(sourceTexture, uv + vec2(offset.x, -offset.y))
               + texture(sourceTexture, uv + vec2(-offset.x, offset.y))
               + texture(sourceTexture, uv + vec2(offset.x, offset.y))) * 0.25;
}
