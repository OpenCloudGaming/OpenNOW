#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(binding = 1) uniform sampler2D sourceTexture;
void main()
{
    fragColor = texture(sourceTexture, uv);
}
