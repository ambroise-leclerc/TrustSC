#version 450

// 3D heightfield waterfall (Density Spectral Array) vertex shader.
//
// The mesh is a fixed rows x cols grid whose ONLY vertex attribute is the height sample; grid
// coordinates are derived from gl_VertexIndex (vertex id = row * cols + col), so the
// persistently mapped vertex buffer stays a bare float array the ring buffer writes into.
//
// Push-constant contract (must match `HeightfieldPushConstants` in the renderer, std430-like
// ordering, 80 bytes total: a 64-byte mat4 followed by four 4-byte floats):
//   mvp          - column-major model-view-projection matrix (fixed perspective camera)
//   rows, cols   - grid dimensions as floats
//   row_offset   - ring-buffer cursor: logical row = (physical row + rows - row_offset) % rows,
//                  so scrolling costs zero copies of the grid
//   height_scale - vertical exaggeration applied to the raw 0..1 samples

layout(location = 0) in float inHeight;

layout(location = 0) out float fragHeight;

layout(push_constant) uniform HeightfieldPushConstants {
    mat4 mvp;
    float rows;
    float cols;
    float row_offset;
    float height_scale;
} pc;

void main() {
    float cols = pc.cols;
    float rows = pc.rows;
    float physical_row = floor(float(gl_VertexIndex) / cols);
    float col = mod(float(gl_VertexIndex), cols);

    // Remap the physical (storage) row to its logical scroll position: the most recently
    // written ring row renders nearest the viewer, older rows recede.
    float logical_row = mod(physical_row + rows - pc.row_offset, rows);

    // Normalized grid space: x in [-1, 1] across bins, z in [0, 1] front-to-back.
    float x = (col / (cols - 1.0)) * 2.0 - 1.0;
    float z = logical_row / (rows - 1.0);
    float y = clamp(inHeight, 0.0, 1.0) * pc.height_scale;

    gl_Position = pc.mvp * vec4(x, y, z, 1.0);
    fragHeight = clamp(inHeight, 0.0, 1.0);
}
