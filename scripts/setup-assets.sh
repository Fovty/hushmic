#!/usr/bin/env bash
# Provision the gitignored binary assets that the build needs:
#   1. ONNX models   -> assets/models/{dpdfnet8_48khz_hr,dpdfnet2_48khz_hr}.onnx
#   2. ONNX Runtime  -> assets/lib/libonnxruntime.so{,.1,.1.27.0}
#
# Self-sufficient for a fresh CI checkout: if the models are absent it fetches
# them reproducibly via the `dpdfnet` Python package; if the runtime is absent it
# downloads the pinned ONNX Runtime release. Idempotent — re-running is a no-op
# once everything is in place.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ORT_VERSION="1.27.0"
ORT_TGZ="onnxruntime-linux-x64-${ORT_VERSION}.tgz"
ORT_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/${ORT_TGZ}"
MODELS=(dpdfnet8_48khz_hr dpdfnet2_48khz_hr)

mkdir -p "$REPO_ROOT/assets/lib" "$REPO_ROOT/assets/models"

# ---------------------------------------------------------------------------
# 1. ONNX models. If both are present we do nothing. Otherwise fetch them with
#    the `dpdfnet` package into its model cache and copy them into the repo.
# ---------------------------------------------------------------------------
models_present() {
  local m
  for m in "${MODELS[@]}"; do
    [ -f "$REPO_ROOT/assets/models/$m.onnx" ] || return 1
  done
  return 0
}

fetch_models() {
  echo "Models missing; fetching via the dpdfnet package..."
  local cache="${HOME}/.cache/dpdfnet/models"
  local dpdfnet_bin=""

  # Prefer an isolated venv (reproducible, no system pollution); fall back to a
  # user/system pip install if venv creation is unavailable in this environment.
  local venv="$REPO_ROOT/.venv-assets"
  if python3 -m venv "$venv" >/dev/null 2>&1; then
    # shellcheck disable=SC1091
    "$venv/bin/pip" install --quiet --upgrade pip
    "$venv/bin/pip" install --quiet dpdfnet
    dpdfnet_bin="$venv/bin/dpdfnet"
  else
    echo "venv unavailable; installing dpdfnet with pip directly" >&2
    if ! python3 -m pip install --quiet --user dpdfnet 2>/dev/null; then
      python3 -m pip install --quiet --break-system-packages dpdfnet
    fi
    dpdfnet_bin="$(command -v dpdfnet || true)"
    [ -n "$dpdfnet_bin" ] || dpdfnet_bin="python3 -m dpdfnet"
  fi

  echo "Downloading the required models..."
  local dl
  for dl in "${MODELS[@]}"; do
    # shellcheck disable=SC2086
    $dpdfnet_bin download "$dl"
  done

  local m
  for m in "${MODELS[@]}"; do
    if [ ! -f "$cache/$m.onnx" ]; then
      echo "ERROR: expected '$cache/$m.onnx' after 'dpdfnet download' but it is missing." >&2
      exit 1
    fi
    cp -f "$cache/$m.onnx" "$REPO_ROOT/assets/models/$m.onnx"
    echo "  copied $m.onnx"
  done
}

if models_present; then
  echo "Models already present; skipping fetch."
else
  fetch_models
fi

# ---------------------------------------------------------------------------
# 2. ONNX Runtime shared library (pinned 1.27.0) for ort's load-dynamic.
#    We keep ONE real file (libonnxruntime.so.1.27.0) plus the two symlinks
#    (.so.1 -> .so.1.27.0, .so -> .so.1) that ort/loaders expect.
# ---------------------------------------------------------------------------
ORT_REAL="libonnxruntime.so.${ORT_VERSION}"
if [ ! -f "$REPO_ROOT/assets/lib/$ORT_REAL" ]; then
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  echo "Downloading ONNX Runtime ${ORT_VERSION}..."
  curl -fsSL "$ORT_URL" -o "$tmp/$ORT_TGZ"
  tar -xzf "$tmp/$ORT_TGZ" -C "$tmp"
  # The tarball ships libonnxruntime.so.<ver> (and sometimes symlinks); copy the
  # real versioned object only.
  src="$(find "$tmp/onnxruntime-linux-x64-${ORT_VERSION}/lib" -maxdepth 1 -type f -name "libonnxruntime.so.*" | head -1)"
  if [ -z "$src" ]; then
    echo "ERROR: libonnxruntime.so.<version> not found in the downloaded archive." >&2
    exit 1
  fi
  cp -f "$src" "$REPO_ROOT/assets/lib/$ORT_REAL"
  rm -rf "$tmp"
  trap - EXIT
fi

# Normalize the symlink chain (idempotent).
( cd "$REPO_ROOT/assets/lib"
  ln -sf "$ORT_REAL" "libonnxruntime.so.1"
  ln -sf "libonnxruntime.so.1" "libonnxruntime.so"
)

echo "Assets ready:"
ls -lh "$REPO_ROOT/assets/models/"
ls -lh "$REPO_ROOT/assets/lib/"
