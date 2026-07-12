#version 450

// 3D heightfield waterfall fragment shader: classic DSA palette by normalized height
// (deep blue -> green -> yellow -> red), opaque, depth-tested.

layout(location = 0) in float fragHeight;

layout(location = 0) out vec4 outColor;

vec3 dsa_colormap(float t) {
    // Piecewise-linear 4-stop gradient; t is clamped to [0, 1] by the vertex stage.
    const vec3 c0 = vec3(0.05, 0.08, 0.35); // deep blue
    const vec3 c1 = vec3(0.05, 0.55, 0.35); // green
    const vec3 c2 = vec3(0.90, 0.85, 0.15); // yellow
    const vec3 c3 = vec3(0.85, 0.15, 0.10); // red

    if (t < 1.0 / 3.0) {
        return mix(c0, c1, t * 3.0);
    }
    if (t < 2.0 / 3.0) {
        return mix(c1, c2, (t - 1.0 / 3.0) * 3.0);
    }
    return mix(c2, c3, (t - 2.0 / 3.0) * 3.0);
}

void main() {
    outColor = vec4(dsa_colormap(fragHeight), 1.0);
}
