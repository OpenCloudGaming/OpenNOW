#version 440
#extension GL_GOOGLE_include_directive : enable
#include "../shaders/hdrcolor.glsl"
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Params {
    vec4 encoded;
    vec4 mode;
};
void main()
{
    fragColor = vec4(videoColor(encoded.rgb, mode.x, mode.y, mode.z, mode.w) * encoded.a, encoded.a);
}
