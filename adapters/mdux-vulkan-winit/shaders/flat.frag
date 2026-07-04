#version 450

// Solid-color rectangle (Panel underlay) fragment shader: the interpolated vertex color, as is.

layout(location = 0) in vec4 fragColor;

layout(location = 0) out vec4 outColor;

void main() {
    outColor = fragColor;
}
