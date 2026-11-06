#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
// geometry.xy: texel size; geometry.w: t, the time of the generated frame between the
// previous (0) and the current (1) source frame. t = 0.5 is the classic midpoint.
layout(std140, binding = 0) uniform Parameters {
    vec4 geometry;
};
layout(binding = 1) uniform sampler2D previousTexture;
layout(binding = 2) uniform sampler2D currentTexture;
layout(binding = 3) uniform sampler2D forwardFlow;
layout(binding = 4) uniform sampler2D backwardFlow;
layout(binding = 5) uniform sampler2D sceneCut;

bool inside(vec2 p)
{
    return all(greaterThanEqual(p, vec2(0.0))) && all(lessThanEqual(p, vec2(1.0)));
}

void main()
{
    vec4 actual = texture(currentTexture, uv);
    if (texture(sceneCut, vec2(0.5)).r > 0.5) {
        fragColor = actual;
        return;
    }
    // The previous frame travels t of its forward flow and the current frame (1 - t) of
    // its backward flow to meet at the generated frame.
    float t = clamp(geometry.w, 0.0, 1.0);
    float s = 1.0 - t;
    vec2 p = uv - texture(forwardFlow, uv).xy * t;
    vec2 q = uv - texture(backwardFlow, uv).xy * s;
    p = uv - texture(forwardFlow, p).xy * t;
    q = uv - texture(backwardFlow, q).xy * s;
    vec4 forward = texture(forwardFlow, p);
    vec4 backward = texture(backwardFlow, q);
    vec2 disagreement = (forward.xy + texture(backwardFlow, p + forward.xy).xy) / geometry.xy;
    vec2 reverseDisagreement = (backward.xy + texture(forwardFlow, q + backward.xy).xy) / geometry.xy;
    vec4 a = texture(previousTexture, p);
    vec4 b = texture(currentTexture, q);
    float error = dot(abs(a.rgb - b.rgb), vec3(0.25, 0.5, 0.25));
    float forwardError = dot(abs(a.rgb - texture(currentTexture, p + forward.xy).rgb), vec3(0.25, 0.5, 0.25));
    float backwardError = dot(abs(b.rgb - texture(previousTexture, q + backward.xy).rgb), vec3(0.25, 0.5, 0.25));
    bool forwardReliable = inside(p) && inside(p + forward.xy)
        && length(disagreement) < 1.5 && forward.z < 0.10 && forwardError < 0.12
        && (forward.w > 0.015 || length(forward.xy / geometry.xy) < 0.25);
    bool backwardReliable = inside(q) && inside(q + backward.xy)
        && length(reverseDisagreement) < 1.5 && backward.z < 0.10 && backwardError < 0.12
        && (backward.w > 0.015 || length(backward.xy / geometry.xy) < 0.25);
    if (forwardReliable && backwardReliable)
        fragColor = error < 0.12 ? mix(a, b, t) : actual;
    else if (forwardReliable)
        fragColor = a;
    else if (backwardReliable)
        fragColor = b;
    else
        fragColor = actual;
}
