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
    local_path=""
    case "$STUDIO_REPO_URL" in
      /* | ./* | ../*) local_path="$STUDIO_REPO_URL" ;;
      # git's ownership check runs against the resolved filesystem path, not the file:// URL
      # string itself, so the scheme has to be stripped for the exemption to actually match.
      file://*) local_path="${STUDIO_REPO_URL#file://}" ;;
    esac
    if [ -n "$local_path" ]; then
      # If it's owned by a different uid than the container runs as, git's "dubious ownership"
      # guard refuses to clone it. Only this specific source path needs the exemption -- unlike
      # a wildcard, this doesn't waive the guard for every repository the container might ever
      # touch. Cloning from a local path checks the *.git* subdirectory specifically, not the
      # working-tree path, so both forms are registered.
      git config --global --add safe.directory "$local_path"
      git config --global --add safe.directory "$local_path/.git"
    fi
    echo "entrypoint: cloning $STUDIO_REPO_URL into $repo_dir"
    git clone --depth 1 "$STUDIO_REPO_URL" "$repo_dir"
  fi
fi

exec trustsc-medui-studio --repo "$repo_dir" --listen 0.0.0.0:8080
