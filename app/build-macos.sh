#!/usr/bin/env bash

set -Eeuo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
tauri_dir="$script_dir/src-tauri"

build_mode="local"
build_target="native"
notary_profile="${ECHOMSKIT_NOTARY_PROFILE:-}"

usage() {
  cat <<'EOF'
Build the EchoMSkit macOS app and DMG.

Usage:
  bash app/build-macos.sh [--local|--release] [--native|--universal]
                          [--notary-profile NAME]

Options:
  --local       Ad-hoc sign for testing on this Mac (default).
  --release     Developer ID sign, notarize, and staple for distribution.
  --native      Build for this Mac's architecture (default).
  --universal   Build for both Apple Silicon and Intel Macs.
  --notary-profile NAME
                Notarize with credentials stored by notarytool in Keychain.
  -h, --help    Show this help.

Release builds use a Developer ID Application identity from a configured keychain.
Notarization accepts a notarytool Keychain profile, APPLE_ID/APPLE_PASSWORD/
APPLE_TEAM_ID, or APPLE_API_KEY/APPLE_API_ISSUER credentials.
EOF
}

while (($#)); do
  case "$1" in
    --local) build_mode="local" ;;
    --release) build_mode="release" ;;
    --native) build_target="native" ;;
    --universal) build_target="universal" ;;
    --notary-profile)
      if (($# < 2)); then
        echo "Error: --notary-profile requires a profile name." >&2
        exit 2
      fi
      notary_profile="$2"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown option '$1'." >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: this script must be run on macOS." >&2
  exit 1
fi

# Prefer the full Xcode installation without changing the machine-wide selection.
if [[ -d /Applications/Xcode.app/Contents/Developer ]]; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
fi

if ! xcodebuild -version >/dev/null 2>&1; then
  echo "Error: a full Xcode installation is required." >&2
  echo "Set DEVELOPER_DIR to its Contents/Developer directory and try again." >&2
  exit 1
fi

for command_name in cargo rustc codesign lipo; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Error: required command '$command_name' was not found." >&2
    exit 1
  fi
done

if ! cargo tauri --version >/dev/null 2>&1; then
  echo "Installing Tauri CLI 2..."
  cargo install tauri-cli --version "^2" --locked
fi

host_target="$(rustc -Vv | awk '/^host:/ { print $2 }')"
case "$host_target" in
  aarch64-apple-darwin|x86_64-apple-darwin) ;;
  *)
    echo "Error: unsupported macOS Rust target '$host_target'." >&2
    exit 1
    ;;
esac

sidecar_dir="$tauri_dir/binaries"
mkdir -p "$sidecar_dir"

if [[ "$build_target" == "universal" ]]; then
  for rust_target in aarch64-apple-darwin x86_64-apple-darwin; do
    if ! rustup target list --installed | grep -Fxq "$rust_target"; then
      echo "Error: Rust target '$rust_target' is not installed." >&2
      echo "Run: rustup target add $rust_target" >&2
      exit 1
    fi
    echo "Building EchoMSkit worker for $rust_target..."
    cargo build --release --target "$rust_target" --manifest-path "$repo_root/Cargo.toml"
  done

  tauri_target="universal-apple-darwin"
  cp \
    "$repo_root/target/aarch64-apple-darwin/release/EchoMSkit" \
    "$sidecar_dir/EchoMSkit-worker-aarch64-apple-darwin"
  cp \
    "$repo_root/target/x86_64-apple-darwin/release/EchoMSkit" \
    "$sidecar_dir/EchoMSkit-worker-x86_64-apple-darwin"
  sidecar_path="$sidecar_dir/EchoMSkit-worker-$tauri_target"
  lipo -create \
    "$repo_root/target/aarch64-apple-darwin/release/EchoMSkit" \
    "$repo_root/target/x86_64-apple-darwin/release/EchoMSkit" \
    -output "$sidecar_path"
  bundle_root="$tauri_dir/target/$tauri_target/release/bundle"
else
  echo "Building EchoMSkit worker for $host_target..."
  cargo build --release --manifest-path "$repo_root/Cargo.toml"

  tauri_target="$host_target"
  sidecar_path="$sidecar_dir/EchoMSkit-worker-$tauri_target"
  cp "$repo_root/target/release/EchoMSkit" "$sidecar_path"
  bundle_root="$tauri_dir/target/release/bundle"
fi
chmod +x "$sidecar_path"

echo "Generating application icons..."
cargo tauri icon "$script_dir/app-icon.svg" --output "$tauri_dir/icons"

if [[ "$build_mode" == "release" ]]; then
  signing_identity="${APPLE_SIGNING_IDENTITY:-}"
  if [[ -z "$signing_identity" ]]; then
    signing_identity="$(
      security find-identity -v -p codesigning 2>/dev/null \
        | sed -n 's/^.*"\(Developer ID Application:.*\)".*$/\1/p' \
        | head -n 1
    )"
  fi
  if [[ -z "$signing_identity" ]]; then
    echo "Error: no usable 'Developer ID Application' identity was found." >&2
    echo "In Xcode, open Settings > Accounts > Manage Certificates and make sure" >&2
    echo "the certificate and its private key are installed on this Mac." >&2
    exit 1
  fi
  export APPLE_SIGNING_IDENTITY="$signing_identity"

  has_apple_id_credentials=false
  if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    has_apple_id_credentials=true
  fi
  has_api_credentials=false
  if [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
    has_api_credentials=true
  fi
  has_keychain_credentials=false
  if [[ -n "$notary_profile" ]]; then
    has_keychain_credentials=true
  fi

  if [[ "$has_apple_id_credentials" != true \
    && "$has_api_credentials" != true \
    && "$has_keychain_credentials" != true ]]; then
    if [[ ! -t 0 ]]; then
      echo "Error: notarization credentials are not configured." >&2
      echo "Set APPLE_ID, APPLE_PASSWORD (an app-specific password), and APPLE_TEAM_ID." >&2
      exit 1
    fi

    default_team_id=""
    if [[ "$signing_identity" =~ \(([A-Z0-9]+)\)$ ]]; then
      default_team_id="${BASH_REMATCH[1]}"
    fi
    read -r -p "Apple Account email: " APPLE_ID
    read -r -p "Apple Team ID [$default_team_id]: " APPLE_TEAM_ID
    APPLE_TEAM_ID="${APPLE_TEAM_ID:-$default_team_id}"
    read -r -s -p "Apple app-specific password: " APPLE_PASSWORD
    echo
    export APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
  fi

  if [[ "$has_keychain_credentials" == true ]]; then
    # Tauri accepts environment credentials directly. Keychain profiles are
    # submitted after the signed DMG is created, using Apple's notarytool.
    unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH || true
  fi

  echo "Building, signing, and notarizing EchoMSkit GUI..."
else
  export APPLE_SIGNING_IDENTITY="-"
  unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH || true
  echo "Building an ad-hoc signed EchoMSkit GUI test app and DMG..."
fi

(
  cd "$tauri_dir"
  # Avoid Finder/AppleScript automation prompts while arranging the DMG.
  export CI="${CI:-true}"
  if [[ "$build_target" == "universal" ]]; then
    cargo tauri build --bundles app,dmg --target "$tauri_target"
  else
    cargo tauri build --bundles app,dmg
  fi
)

app_path="$bundle_root/macos/EchoMSkit GUI.app"
dmg_path="$(find "$bundle_root/dmg" -maxdepth 1 -type f -name '*.dmg' -print -quit)"

if [[ ! -d "$app_path" || -z "$dmg_path" ]]; then
  echo "Error: the expected app or DMG was not produced." >&2
  exit 1
fi

echo "Verifying application code signatures..."
codesign --verify --deep --strict --verbose=2 "$app_path"

if [[ "$build_mode" == "release" ]]; then
  if [[ -n "$notary_profile" ]]; then
    echo "Submitting the signed DMG to Apple's notary service..."
    xcrun notarytool submit "$dmg_path" --keychain-profile "$notary_profile" --wait
    echo "Stapling and validating the DMG notarization ticket..."
    xcrun stapler staple "$dmg_path"
    xcrun stapler validate "$dmg_path"
    spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg_path"
  else
    echo "Verifying Gatekeeper acceptance and notarization ticket..."
    xcrun stapler validate "$app_path"
    spctl --assess --type execute --verbose=2 "$app_path"
  fi
fi

echo
echo "Build complete."
echo "App: $app_path"
echo "DMG: $dmg_path"
if [[ "$build_mode" == "local" ]]; then
  echo "This local build is for testing only. Use --release for public distribution."
fi
