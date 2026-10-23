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
            error += difference(textureLod(sourceTexture, p, 0.0).rgb,
                                textureLod(destinationTexture, p + motion, 0.0).rgb);
        }
    }
    return error / 9.0;
}

void main()
{
    vec2 centers[10];
    centers[0] = vec2(0.0);
    int seedCount = 1;
    if (geometry.w < 1.5) {
        vec2 size = ceil(round(1.0 / geometry.xy) * 0.5);
        vec2 origin = floor(uv * size);
        for (int y = -1; y <= 1; ++y) {
            for (int x = -1; x <= 1; ++x) {
                vec2 pixel = clamp(origin + vec2(x, y), vec2(0.0), size - 1.0);
                vec2 candidate = textureLod(coarseFlow, (pixel + 0.5) / size, 0.0).xy;
                candidate = round(candidate / geometry.xy) * geometry.xy;
                bool duplicate = false;
                for (int previous = 0; previous < 10; ++previous) {
                    if (previous >= seedCount) break;
                    if (all(equal(candidate, centers[previous]))) duplicate = true;
                }
                if (!duplicate) centers[seedCount++] = candidate;
            }
        }
    }
    vec2 best = vec2(0.0);
    float bestError = 100.0;
    float secondError = 100.0;
    int radius = geometry.w > 0.5 ? 2 : 1;
    for (int seed = 0; seed < 10; ++seed) {
        if (seed >= seedCount) break;
        vec2 center = centers[seed];
        for (int y = -2; y <= 2; ++y) {
            for (int x = -2; x <= 2; ++x) {
                if (abs(x) > radius || abs(y) > radius)
                    continue;
                vec2 candidate = center + vec2(x, y) * geometry.xy;
                bool visited = false;
                for (int previous = 0; previous < 10; ++previous) {
                    if (previous >= seed) break;
                    if (all(lessThan(abs((candidate - centers[previous]) / geometry.xy), vec2(radius + 0.5))))
                        visited = true;
                }
                if (visited) continue;
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
