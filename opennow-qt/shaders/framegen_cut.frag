#version 440
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 fragColor;
layout(binding = 1) uniform sampler2D previousTexture;
layout(binding = 2) uniform sampler2D currentTexture;
layout(binding = 3) uniform sampler2D forwardFlow;
layout(binding = 4) uniform sampler2D backwardFlow;
void main()
{
    float bad = 0.0;
    float error = 0.0;
    float changed = 0.0;
    for (int y = 0; y < 8; ++y) {
        for (int x = 0; x < 8; ++x) {
            vec2 p = (vec2(x, y) + 0.5) / 8.0;
            float e = (texture(forwardFlow, p).z + texture(backwardFlow, p).z) * 0.5;
            error += e;
            bad += step(0.12, e);
            changed += dot(abs(texture(previousTexture, p).rgb
                             - texture(currentTexture, p).rgb), vec3(0.25, 0.5, 0.25));
        }
    }
    float cut = float(error / 64.0 > 0.16
                     || (bad / 64.0 > 0.65 && changed / 64.0 > 0.18));
    fragColor = vec4(cut, 0.0, 0.0, 1.0);
}
