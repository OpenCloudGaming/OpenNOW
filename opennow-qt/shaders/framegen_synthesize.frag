#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
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
    vec2 p = uv - texture(forwardFlow, uv).xy * 0.5;
    vec2 q = uv - texture(backwardFlow, uv).xy * 0.5;
    p = uv - texture(forwardFlow, p).xy * 0.5;
    q = uv - texture(backwardFlow, q).xy * 0.5;
    vec4 forward = texture(forwardFlow, p);
    vec4 backward = texture(backwardFlow, q);
    vec2 disagreement = (forward.xy + texture(backwardFlow, p + forward.xy).xy) / geometry.xy;
    vec2 reverseDisagreement = (backward.xy + texture(forwardFlow, q + backward.xy).xy) / geometry.xy;
    vec4 a = texture(previousTexture, p);
    vec4 b = texture(currentTexture, q);
    float error = dot(abs(a.rgb - b.rgb), vec3(0.25, 0.5, 0.25));
    bool reliable = inside(p) && inside(q) && inside(p + forward.xy) && inside(q + backward.xy)
        && length(disagreement) < 1.5 && length(reverseDisagreement) < 1.5
        && forward.z < 0.10 && backward.z < 0.10 && error < 0.12;
    bool textured = min(forward.w, backward.w) > 0.015;
    bool stationary = length(forward.xy / geometry.xy) < 0.25
                   && length(backward.xy / geometry.xy) < 0.25;
    fragColor = reliable && (textured || stationary) ? (a + b) * 0.5 : actual;
}
