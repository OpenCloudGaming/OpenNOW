#version 440
#extension GL_GOOGLE_include_directive : enable
#include "hdrcolor.glsl"
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Params {
    mat4 matrix;
    float opacity;
    float outputMode;
    float whiteNits;
    float padding;
};
layout(binding = 1) uniform sampler2D sourceTexture;
void main()
{
    vec4 color = texture(sourceTexture, uv);
    if (outputMode > 0.5 && color.a > 0.00001)
        color.rgb = outputColor(sdrToLinear(color.rgb / color.a) * whiteNits, outputMode, whiteNits) * color.a;
    fragColor = color * opacity;
}
