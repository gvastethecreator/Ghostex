#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GPUI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$GPUI_DIR/../.." && pwd)"
BUILD_CACHE_DIR="${GHOSTEX_BUILD_CACHE_DIR:-$REPO_ROOT/build/${GHOSTEX_MACOS_ARCH:-$(uname -m)}/build-cache}"
source "$SCRIPT_DIR/build-cache.sh"

build_cef_sidebar_bundle_if_needed() {
	local bundle_digest
	local -a bundle_outputs

	# CDXC:Build 2026-09-05 WHY:
	# Generated CSS is an output, so hashing it as an input invalidates the cache after Tailwind changes it.
	# Hash the CEF entries, their shared imports and toolchain inputs so Rust-only edits reuse the web bundle.
	bundle_digest="$(fingerprint_inputs \
		--value "cef-sidebar-bundle-v3" \
		--exclude-path "$REPO_ROOT/packages/core-ui/styles/shadcn.generated.css" \
		--value "bun=$(bun --version 2>/dev/null || true)" \
		--path "$SCRIPT_DIR/build-macos-sidebar.sh" \
		--path "$GPUI_DIR/vite.config.ts" \
		--path "$GPUI_DIR/tsconfig.json" \
		--path "$GPUI_DIR/manage.html" \
		--path "$GPUI_DIR/work.html" \
		--path "$GPUI_DIR/sidebar" \
		--path "$GPUI_DIR/views" \
		--path "$REPO_ROOT/packages/core-ui" \
		--path "$REPO_ROOT/packages/components" \
		--path "$REPO_ROOT/packages/shared" \
		--path "$REPO_ROOT/tooling/docs-classic-assets.ts" \
		--path "$REPO_ROOT/package.json" \
		--path "$REPO_ROOT/bun.lock" \
		--path "$REPO_ROOT/tsconfig.json")"
	bundle_outputs=(
		"$REPO_ROOT/packages/core-ui/styles/shadcn.generated.css"
		"$GPUI_DIR/dist/sidebar/manage.html"
		"$GPUI_DIR/dist/sidebar/work.html"
	)
	if cache_matches "cef-sidebar-bundle" "$bundle_digest" "${bundle_outputs[@]}"; then
		echo "CEF sidebar bundle is current; skipping web build."
		return 0
	fi
	(
		cd "$REPO_ROOT"
		bunx tailwindcss -i packages/core-ui/styles/shadcn.css -o packages/core-ui/styles/shadcn.generated.css --minify
		bunx vite build --config "$GPUI_DIR/vite.config.ts"
	)
	write_cache_stamp "cef-sidebar-bundle" "$bundle_digest"
}

build_cef_sidebar_bundle_if_needed
