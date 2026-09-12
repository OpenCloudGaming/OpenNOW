#version 440
#extension GL_GOOGLE_include_directive : enable
#include "hdrcolor.glsl"
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(binding = 1) uniform sampler2D linearScene;
void main()
{
    fragColor = vec4(outputColor(texture(linearScene, uv).rgb * 80.0, 2.0, 203.0), 1.0);
}
