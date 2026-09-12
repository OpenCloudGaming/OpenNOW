#version 440
#extension GL_GOOGLE_include_directive : require

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Parameters {
    vec4 geometry;
    vec4 dimensions;
};
layout(binding = 1) uniform sampler2D sourceTexture;

#define A_GPU 1
#define A_GLSL 1
#define FSR_RCAS_F 1
#include "fsr1/ffx_a.h"
#include "fsr1/ffx_fsr1.h"

AF4 FsrRcasLoadF(ASU2 p)
{
    return texelFetch(sourceTexture, clamp(p, ivec2(0), ivec2(dimensions.zw) - 1), 0);
}
void FsrRcasInputF(inout AF1 r, inout AF1 g, inout AF1 b) {}

void main()
{
    uvec4 con;
    FsrRcasCon(con, geometry.w);
    vec3 color;
    FsrRcasF(color.r, color.g, color.b, uvec2(uv * dimensions.zw), con);
    if (any(isnan(color)) || any(isinf(color)))
        color = FsrRcasLoadF(ivec2(uv * dimensions.zw)).rgb;
    fragColor = vec4(color, 1.0);
}
