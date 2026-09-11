#version 440
layout(location = 0) in vec4 position;
layout(location = 1) in vec2 texCoord;
layout(location = 0) out vec2 uv;
layout(std140, binding = 0) uniform Params {
    mat4 matrix;
    float opacity;
    float outputMode;
    float whiteNits;
    float padding;
};
void main()
{
    uv = texCoord;
    gl_Position = matrix * position;
}
