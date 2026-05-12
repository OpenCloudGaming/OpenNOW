#include <metal_stdlib>
using namespace metal;

struct OPNBackdropVertex {
    float2 position;
    float2 texCoord;
};

struct OPNBackdropUniforms {
    float2 viewportSize;
    float2 focusCenter;
    float focusRadius;
    float focusStrength;
    float time;
    float3 accentColor;
    float bloomIntensity;
};

struct OPNBackdropVarying {
    float4 position [[position]];
    float2 texCoord;
    float2 screenPosition;
};

vertex OPNBackdropVarying opnMeshWarpVertex(uint vertexID [[vertex_id]],
                                            constant OPNBackdropVertex *vertices [[buffer(0)]],
                                            constant OPNBackdropUniforms &uniforms [[buffer(1)]]) {
    OPNBackdropVertex input = vertices[vertexID];
    float2 pixelPosition = (input.position * 0.5 + 0.5) * uniforms.viewportSize;
    float2 delta = pixelPosition - uniforms.focusCenter;
    float distanceToFocus = length(delta);
    float normalized = clamp(1.0 - distanceToFocus / max(uniforms.focusRadius, 1.0), 0.0, 1.0);
    float falloff = normalized * normalized * (3.0 - 2.0 * normalized);
    float wave = sin(delta.x * 0.018 + delta.y * 0.012 + uniforms.time * 1.35) * 0.006;
    float pull = falloff * uniforms.focusStrength * 0.026;
    float2 direction = distanceToFocus > 0.001 ? normalize(delta) : float2(0.0);
    float2 warpedPosition = input.position - direction * pull + direction.yx * wave * falloff;

    OPNBackdropVarying output;
    output.position = float4(warpedPosition, 0.0, 1.0);
    output.texCoord = input.texCoord;
    output.screenPosition = pixelPosition;
    return output;
}

fragment float4 opnVariableBloomFragment(OPNBackdropVarying input [[stage_in]],
                                          texture2d<float> sourceTexture [[texture(0)]],
                                          sampler linearSampler [[sampler(0)]],
                                          constant OPNBackdropUniforms &uniforms [[buffer(0)]]) {
    float2 delta = input.screenPosition - uniforms.focusCenter;
    float distanceToFocus = length(delta);
    float focus = clamp(1.0 - distanceToFocus / max(uniforms.focusRadius, 1.0), 0.0, 1.0);
    focus = focus * focus * (3.0 - 2.0 * focus);

    float2 pixel = 1.0 / max(uniforms.viewportSize, float2(1.0));
    float blurRadius = mix(1.0, 16.0, focus * uniforms.focusStrength);

    constexpr int sampleCount = 12;
    float2 offsets[sampleCount] = {
        float2(0.000, 0.000),
        float2(0.866, 0.500),
        float2(-0.866, 0.500),
        float2(0.000, -1.000),
        float2(1.414, 1.414),
        float2(-1.414, 1.414),
        float2(1.414, -1.414),
        float2(-1.414, -1.414),
        float2(2.500, 0.000),
        float2(-2.500, 0.000),
        float2(0.000, 2.500),
        float2(0.000, -2.500)
    };
    float weights[sampleCount] = {
        0.180, 0.090, 0.090, 0.090,
        0.070, 0.070, 0.070, 0.070,
        0.055, 0.055, 0.055, 0.055
    };

    float4 color = float4(0.0);
    float totalWeight = 0.0;
    for (int i = 0; i < sampleCount; i++) {
        float2 uv = input.texCoord + offsets[i] * pixel * blurRadius;
        float weight = weights[i];
        color += sourceTexture.sample(linearSampler, uv) * weight;
        totalWeight += weight;
    }
    color /= max(totalWeight, 0.001);

    float luminance = dot(color.rgb, float3(0.2126, 0.7152, 0.0722));
    float bloomMask = smoothstep(0.42, 0.95, luminance) * focus;
    float3 bloom = uniforms.accentColor * bloomMask * uniforms.bloomIntensity;
    float vignette = smoothstep(1.15, 0.15, distanceToFocus / max(uniforms.focusRadius, 1.0));

    color.rgb = mix(color.rgb, color.rgb + bloom, uniforms.focusStrength);
    color.rgb += uniforms.accentColor * vignette * 0.055 * uniforms.focusStrength;
    return float4(color.rgb, color.a);
}
