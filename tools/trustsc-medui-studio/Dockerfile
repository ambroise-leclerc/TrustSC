# trustsc-medui-studio container image (ADR-022 wave S10). Build from the repository root, since
# this crate is a Cargo workspace member and needs the whole workspace as build context:
#
#   docker build -f tools/trustsc-medui-studio/Dockerfile -t trustsc-medui-studio .
#
# Runtime uses lavapipe (Mesa's software Vulkan rasterizer) — the same one CI uses
# (.github/workflows/ci.yml) — so the container renders correctly with no GPU passthrough on the
# host at all.

# ---- build stage -----------------------------------------------------------------------------
FROM rust:1-bookworm AS builder
WORKDIR /workspace
COPY . .
RUN cargo build --release --locked -p trustsc-medui-studio

# ---- runtime stage ----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# mesa-vulkan-drivers: lavapipe, the software Vulkan ICD (ADR-016 §8). libvulkan1: the Vulkan
# loader ash dlopens at runtime (the adapter's "loaded" ash feature, no build-time linking). git:
# only exercised when STUDIO_REPO_URL is set (see entrypoint.sh); ca-certificates so that clone
# can use https:// remotes.
RUN apt-get update && apt-get install -y --no-install-recommends \
      mesa-vulkan-drivers libvulkan1 git ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/target/release/trustsc-medui-studio /usr/local/bin/trustsc-medui-studio
COPY tools/trustsc-medui-studio/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Default bind-mount target for the "mount an existing checkout" deployment mode; also the
# default clone target when STUDIO_REPO_URL is set (see entrypoint.sh).
VOLUME ["/data"]
WORKDIR /data

EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
