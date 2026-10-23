#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Parameters {
    vec4 geometry;
};
layout(binding = 1) uniform sampler2D sourceTexture;
void main()
{
    vec4 sum = vec4(0.0);
    for (int y = 0; y < 4; ++y)
        for (int x = 0; x < 4; ++x)
            sum += texture(sourceTexture, uv + ((vec2(x, y) + 0.5) * 0.25 - 0.5) * geometry.xy);
    fragColor = sum * 0.0625;
}
