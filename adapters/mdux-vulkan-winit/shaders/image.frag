#version 450

// Governed-image fragment shader: straight-alpha RGBA sampled as baked — no tinting, no
// filtering surprises (nearest sampling, intrinsic size).

layout(location = 0) in vec2 fragUv;

layout(location = 0) out vec4 outColor;

layout(set = 0, binding = 0) uniform sampler2D imageTexture;

void main() {
    outColor = texture(imageTexture, fragUv);
}
