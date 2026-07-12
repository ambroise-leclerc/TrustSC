#version 450

// Solid-color rectangle (Panel underlay, ADR-014) vertex shader. All panels live in ONE static
// vertex buffer (6 vertices per panel, NDC precomputed per swapchain extent) drawn with a
// single call before everything else — no descriptors, no push constants.

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec4 inColor;

layout(location = 0) out vec4 fragColor;

void main() {
    gl_Position = vec4(inPosition, 0.0, 1.0);
    fragColor = inColor;
}
