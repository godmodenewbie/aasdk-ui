#!/bin/bash
set -e

echo "============================================="
echo "   Android Auto Headunit VM Setup Script     "
echo "   Target OS: Debian 13 (ARM64 or AMD64)     "
echo "============================================="

# 1. Update and install system dependencies
echo "[1/4] Installing system dependencies (requires sudo password)..."
sudo apt-get update
sudo apt-get install -y curl build-essential pkg-config libusb-1.0-0-dev git wget cmake

# 2. Install Rust safely
echo "[2/4] Checking Rust environment..."
if ! command -v cargo &> /dev/null; then
    echo "      Rust not found. Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    export PATH="$HOME/.cargo/bin:$PATH"
    source "$HOME/.cargo/env"
else
    echo "      Rust is already installed."
fi

# 3. Setup Project Directory & Broadway.js Assets
echo "[3/4] Checking and downloading Broadway.js Web UI assets..."
mkdir -p public

BROADWAY_URL="https://raw.githubusercontent.com/mbebenita/Broadway/master/Player"

# Download the required files directly from the Broadway repository into the public folder
wget -q -nc -O public/YUVCanvas.js "$BROADWAY_URL/YUVCanvas.js" || echo "YUVCanvas.js already exists"
wget -q -nc -O public/Decoder.js "$BROADWAY_URL/Decoder.js" || echo "Decoder.js already exists"
wget -q -nc -O public/Player.js "$BROADWAY_URL/Player.js" || echo "Player.js already exists"

echo "      Broadway assets downloaded."

# 4. Compile the Rust Code
echo "[4/4] Building Rust Backend (this may take a few minutes)..."
# We assume this script is executed inside the aasdk-ui workspace folder
if [ -f "Cargo.toml" ]; then
    cargo build --release
else
    echo "⚠️ Warning: Cargo.toml not found! Make sure to run this script from inside the aasdk-ui folder."
    exit 1
fi

echo "============================================="
echo " 🎉 Setup Complete! "
echo "============================================="
echo "CARA MENJALANKAN:"
echo "-----------------------------------"
echo "1. Load environment Rust:"
echo "   source \$HOME/.cargo/env"
echo ""
echo "2. Colokkan HP Android ke Host dan pastikan USB Passthrough ke VM aktif."
echo ""
echo "3. Jalankan dengan verbose log (PENTING untuk debug):"
echo "   sudo RUST_LOG=info ./target/release/aasdk-ui"
echo ""
echo "   Atau untuk debug TLS/USB lebih mendalam:"
echo "   sudo RUST_LOG=debug ./target/release/aasdk-ui"
echo ""
echo "4. Di komputer aslimu (Host), buka browser dan akses:"
echo "   http://<IP_VM_DEBIAN_KAMU>:8080"
echo ""
echo "NOTE: Jika masih Error 7, pastikan Cargo.toml memiliki:"
echo "  [patch.crates-io]"
echo "  rustls-webpki = { git = \"https://github.com/uglyoldbob/webpki\", branch = \"main5\" }"
echo "============================================="

