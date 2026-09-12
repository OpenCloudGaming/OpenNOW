#version 440
layout(location = 0) out vec2 uv;
layout(std140, binding = 0) uniform Parameters {
    vec4 geometry;
};
void main()
{
    const vec2 corners[3] = vec2[3](vec2(0, 0), vec2(2, 0), vec2(0, 2));
    uv = corners[gl_VertexIndex];
    gl_Position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);
    gl_Position.y *= geometry.z;
}
