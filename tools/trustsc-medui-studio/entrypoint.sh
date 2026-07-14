#!/bin/sh
# Container entrypoint (ADR-022 wave S10): resolves which .medui repo checkout to serve, then
# execs the server. Two supported deployment modes (see the crate README):
#   - Bind-mounted: `docker run -v /host/repo:/data ...`, STUDIO_REPO_URL unset. Serves
#     whatever is already at $STUDIO_REPO_DIR (default /data) — the container never touches it.
#   - Cloned at startup: STUDIO_REPO_URL set to a git remote. Cloned once into
#     $STUDIO_REPO_DIR on first start (a shallow clone; re-runs against the same volume reuse
#     the existing checkout rather than re-cloning).
set -eu

repo_dir="${STUDIO_REPO_DIR:-/data}"

if [ -n "${STUDIO_REPO_URL:-}" ]; then
  if [ -d "$repo_dir/.git" ]; then
    echo "entrypoint: $repo_dir already a git checkout, not re-cloning"
  else
    # Every checkout git touches in this container is single-tenant and ephemeral (the image's
    # only job), so git's "dubious ownership" guard (relevant when a bind-mounted or
    # locally-cloned source is owned by a different uid than the container runs as) has nothing
    # to protect here; without this it fails closed on some STUDIO_REPO_URL/volume combinations.
    git config --global --add safe.directory '*'
    echo "entrypoint: cloning $STUDIO_REPO_URL into $repo_dir"
    git clone --depth 1 "$STUDIO_REPO_URL" "$repo_dir"
  fi
fi

exec trustsc-medui-studio --repo "$repo_dir" --listen 0.0.0.0:8080
