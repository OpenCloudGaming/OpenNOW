#version 440
layout(location = 0) out vec2 itemPosition;
layout(std140, binding = 0) uniform Composition {
    mat4 matrix;
    vec4 bounds;
    vec4 videoRect;
    vec4 parameters;
    vec4 colorParameters;
};
void main()
{
    const vec2 corners[6] = vec2[6](vec2(0, 0), vec2(1, 0), vec2(0, 1),
                                  vec2(0, 1), vec2(1, 0), vec2(1, 1));
    itemPosition = bounds.xy + corners[gl_VertexIndex] * bounds.zw;
    gl_Position = matrix * vec4(itemPosition, 0.0, 1.0);
}
