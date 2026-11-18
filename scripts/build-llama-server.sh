#!/usr/bin/env bash
# Builds llama.cpp's `llama-server`, the helper that runs the free summary
# model, as one program, and puts it where Tauri bundles it:
#   src-tauri/binaries/bolke-llama-server-<target triple>[.exe]
# Full app builds then ship it beside Bolke by adding
#   --config src-tauri/tauri.summary-helper.conf.json
# It's named bolke-llama-server so it never clashes with a llama-server the
# system already has (Linux packages put both in /usr/bin).
#
# Usage: scripts/build-llama-server.sh [target-triple]
# The triple defaults to this machine's (from `rustc -vV`). CI passes the
# target it is building. Extra CMake flags can come from LLAMA_CMAKE_ARGS.
#
# It runs beside Bolke, not inside it: Bolke's speech engine links its own
# copy of ggml, and a second copy in the same program would clash.
set -euo pipefail

LLAMA_TAG="v1.0.0"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/.llama-cpp/$LLAMA_TAG"
TARGET="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
BUILD="$ROOT/.llama-cpp/build-$TARGET"
OUT_DIR="$ROOT/src-tauri/binaries"

case "$TARGET" in
  *windows*) EXE=".exe" ;;
  *) EXE="" ;;
esac
OUT="$OUT_DIR/bolke-llama-server-$TARGET$EXE"

if [ -x "$OUT" ] && [ "${FORCE:-0}" != "1" ]; then
  echo "llama-server for $TARGET is already built: $OUT (FORCE=1 rebuilds)"
  exit 0
fi

if [ ! -d "$SRC" ]; then
  mkdir -p "$ROOT/.llama-cpp"
  git clone --quiet --depth 1 --branch "$LLAMA_TAG" \
    https://github.com/ggml-org/llama.cpp "$SRC"
fi

# One program with no libraries beside it, no downloads of its own, and a
# CPU floor any machine of that kind has.
ARGS=(
  -DCMAKE_BUILD_TYPE=Release
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_NATIVE=OFF
  -DLLAMA_CURL=OFF
  -DLLAMA_OPENSSL=OFF
  -DLLAMA_BUILD_TESTS=OFF
  -DLLAMA_BUILD_EXAMPLES=OFF
  -DLLAMA_BUILD_TOOLS=ON
  -DLLAMA_BUILD_SERVER=ON
  # No web page inside: Bolke talks to it, nobody opens it in a browser.
  -DLLAMA_BUILD_UI=OFF
  -DLLAMA_USE_PREBUILT_UI=OFF
)
case "$TARGET" in
  aarch64-apple-darwin)
    # Metal (the Mac's GPU), with its shaders built into the program.
    ARGS+=(-DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0
      -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON)
    ;;
  x86_64-apple-darwin)
    ARGS+=(-DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=10.15
      -DGGML_METAL=OFF)
    ;;
  aarch64-pc-windows-msvc)
    # Built with clang-cl (CI sets CC/CXX), whose OpenMP has no runtime to
    # link; ggml's own threading is safe outside MSVC codegen.
    ARGS+=(-DGGML_OPENMP=OFF)
    ;;
  *windows*)
    # OpenMP stays on with MSVC (ggml's own barrier can deadlock under MSVC
    # codegen); vcomp140.dll and the VC++ runtime ship beside Bolke already.
    ;;
  *linux*)
    # No libgomp needed on the user's system.
    ARGS+=(-DGGML_OPENMP=OFF)
    ;;
esac
# shellcheck disable=SC2206
ARGS+=(${LLAMA_CMAKE_ARGS:-})

cmake -S "$SRC" -B "$BUILD" "${ARGS[@]}"
cmake --build "$BUILD" --config Release --target llama-server -j "$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)"

BIN="$(find "$BUILD" -type f -name "llama-server$EXE" -path '*bin*' | head -n 1)"
if [ -z "$BIN" ]; then
  echo "llama-server wasn't built" >&2
  exit 1
fi
mkdir -p "$OUT_DIR"
cp "$BIN" "$OUT"
chmod +x "$OUT"
echo "Built $OUT"
