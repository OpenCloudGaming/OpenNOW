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
#define FSR_EASU_F 1
#include "fsr1/ffx_a.h"
#include "fsr1/ffx_fsr1.h"

AF4 FsrEasuRF(AF2 p) { return textureGather(sourceTexture, p, 0); }
AF4 FsrEasuGF(AF2 p) { return textureGather(sourceTexture, p, 1); }
AF4 FsrEasuBF(AF2 p) { return textureGather(sourceTexture, p, 2); }

void main()
{
    uvec4 con0, con1, con2, con3;
    FsrEasuCon(con0, con1, con2, con3, dimensions.x, dimensions.y,
               dimensions.x, dimensions.y, dimensions.z, dimensions.w);
    vec3 color;
    FsrEasuF(color, uvec2(uv * dimensions.zw), con0, con1, con2, con3);
    if (any(isnan(color)) || any(isinf(color))) color = texture(sourceTexture, uv).rgb;
    fragColor = vec4(color, 1.0);
}
