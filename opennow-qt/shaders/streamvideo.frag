#version 440
#extension GL_GOOGLE_include_directive : enable
#include "hdrcolor.glsl"
layout(location = 0) in vec2 itemPosition;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Composition {
    mat4 matrix;
    vec4 bounds;
    vec4 videoRect;
    vec4 parameters;
    vec4 colorParameters;
};
layout(binding = 1) uniform sampler2D videoTexture;
void main()
{
    vec2 uv = (itemPosition - videoRect.xy) / max(videoRect.zw, vec2(1.0));
    vec3 color = vec3(0.0);
    if (all(greaterThanEqual(uv, vec2(0.0))) && all(lessThanEqual(uv, vec2(1.0)))) {
        color = videoColor(texture(videoTexture, uv).rgb, colorParameters.x,
                           colorParameters.y, colorParameters.z, colorParameters.w);
        if (colorParameters.y < 0.5 && parameters.y > 0.0) {
            uvec2 pixel = uvec2(gl_FragCoord.xy) & uvec2(7u);
            uint rank = ((pixel.x ^ pixel.y) & 1u) * 32u + (pixel.y & 1u) * 16u
                      + ((pixel.x ^ pixel.y) & 2u) * 4u + (pixel.y & 2u) * 2u
                      + ((pixel.x ^ pixel.y) & 4u) / 2u + (pixel.y & 4u) / 4u;
            float dither = ((float(rank) + 0.5) / 64.0 - 0.5) * parameters.y;
            color = floor(clamp(color + vec3(dither), 0.0, 1.0) * 255.0 + 0.5) / 255.0;
        }
    }
    fragColor = vec4(color * parameters.x, parameters.x);
}
