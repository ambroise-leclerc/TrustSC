#version 450

// Governed-image quad (ADR-014) vertex shader: one textured quad per image at its intrinsic
// size, NDC precomputed per swapchain extent — no transforms, no push constants.

layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec2 inUv;

layout(location = 0) out vec2 fragUv;

void main() {
    gl_Position = vec4(inPosition, 0.0, 1.0);
    fragUv = inUv;
}
