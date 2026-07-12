#version 450

layout(set = 0, binding = 0) uniform sampler2D textAtlas;

layout(location = 0) in vec2 fragTexCoord;
layout(location = 0) out vec4 outColor;

layout(push_constant) uniform TextPushConstants {
    mat4 transform;
    vec4 textColor;
} pushConstants;

void main() {
    float alpha = texture(textAtlas, fragTexCoord).r;
    outColor = vec4(pushConstants.textColor.rgb, pushConstants.textColor.a * alpha);
}
