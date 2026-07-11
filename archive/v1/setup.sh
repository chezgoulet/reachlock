#!/usr/bin/env bash
# Dev environment setup for REACHLOCK.
# Run once on a fresh Ubuntu 24.04 machine.
set -euo pipefail

echo "==> Installing Go and gh CLI via apt..."
sudo apt-get update -q
sudo apt-get install -y golang-go gh

echo "==> Installing Rust via rustup..."
if ! command -v rustc &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    source "$HOME/.cargo/env"
else
    echo "    rustc already installed: $(rustc --version)"
fi

echo "==> Installing Godot 4 via Flatpak (official Godot Foundation build)..."
if ! command -v flatpak &>/dev/null; then
    sudo apt-get install -y flatpak
    flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
fi
flatpak install -y flathub org.godotengine.Godot

echo ""
echo "==> All tools installed."
echo ""
echo "  Go:    $(go version)"
echo "  gh:    $(gh --version | head -1)"
echo "  Rust:  $(rustc --version 2>/dev/null || echo 'restart shell, then: source ~/.cargo/env')"
echo "  Godot: flatpak run org.godotengine.Godot --version"
echo ""
echo "Next steps:"
echo "  1. gh auth login"
echo "  2. source ~/.cargo/env   (if this is a new Rust install)"
echo "  3. flatpak run org.godotengine.Godot  (to open the game project)"
