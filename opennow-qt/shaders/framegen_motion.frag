#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform Parameters {
    vec4 geometry;
};
layout(binding = 1) uniform sampler2D sourceTexture;
layout(binding = 2) uniform sampler2D destinationTexture;
layout(binding = 3) uniform sampler2D coarseFlow;

float difference(vec3 a, vec3 b)
{
    vec3 delta = a - b;
    return dot(delta * delta, vec3(0.25, 0.5, 0.25));
}

float cost(vec2 motion)
{
    if (any(lessThan(uv + motion, vec2(0.0)))
        || any(greaterThan(uv + motion, vec2(1.0))))
        return 100.0;
    float error = 0.0;
    for (int y = -1; y <= 1; ++y) {
        for (int x = -1; x <= 1; ++x) {
            vec2 p = uv + vec2(x, y) * geometry.xy;
            error += difference(texture(sourceTexture, p).rgb,
                                texture(destinationTexture, p + motion).rgb);
        }
    }
    return error / 9.0;
}

void main()
{
    vec2 prediction = geometry.w > 1.5 ? vec2(0.0) : texture(coarseFlow, uv).xy;
    prediction = round(prediction / geometry.xy) * geometry.xy;
    if (cost(vec2(0.0)) < cost(prediction))
        prediction = vec2(0.0);
    vec2 best = prediction;
    float bestError = cost(best);
    float secondError = 100.0;
    int radius = geometry.w > 0.5 ? 2 : 1;
    for (int seed = 0; seed < 2; ++seed) {
        if (seed == 1 && (geometry.w != 1.0 || length(prediction / geometry.xy) < 0.5))
            continue;
        vec2 center = seed == 0 ? prediction : vec2(0.0);
        for (int y = -2; y <= 2; ++y) {
            for (int x = -2; x <= 2; ++x) {
                if (abs(x) > radius || abs(y) > radius || (seed == 0 && x == 0 && y == 0))
                    continue;
                vec2 candidate = center + vec2(x, y) * geometry.xy;
                if (seed == 1 && all(lessThanEqual(abs((candidate - prediction) / geometry.xy), vec2(radius))))
                    continue;
                float error = cost(candidate);
                if (error < bestError) {
                    secondError = bestError;
                    bestError = error;
                    best = candidate;
                } else {
                    secondError = min(secondError, error);
                }
            }
        }
    }
    float left = cost(best - vec2(geometry.x, 0.0));
    float right = cost(best + vec2(geometry.x, 0.0));
    float above = cost(best - vec2(0.0, geometry.y));
    float below = cost(best + vec2(0.0, geometry.y));
    vec2 curvature = vec2(left + right, above + below) - 2.0 * bestError;
    vec2 adjustment = clamp(0.5 * vec2(left - right, above - below)
                            / max(curvature, vec2(0.000001)), vec2(-0.5), vec2(0.5));
    vec2 refined = best + adjustment * geometry.xy;
    float refinedError = cost(refined);
    if (refinedError < bestError) {
        best = refined;
        bestError = refinedError;
    }
    float stillError = cost(vec2(0.0));
    if (stillError <= bestError + 0.000004) {
        best = vec2(0.0);
        bestError = stillError;
    }
    fragColor = vec4(best, sqrt(bestError),
                     clamp((sqrt(secondError) - sqrt(bestError)) * 20.0, 0.0, 1.0));
}
