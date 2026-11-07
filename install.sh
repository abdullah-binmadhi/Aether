#!/usr/bin/env bash
# ==============================================================================
#  A E T H E R  —  Automated Native Installer
#  The Pro Grand Cockpit for Host Silicon, Diagnostics & Network Dominance
# ==============================================================================

set -euo pipefail

# Visual colors
BOLD="\033[1m"
GREEN="\033[38;5;82m"
AMBER="\033[38;5;214m"
CYAN="\033[38;5;51m"
RED="\033[38;5;196m"
DIM="\033[2m"
RESET="\033[0m"

echo -e "${AMBER}${BOLD}"
cat << "EOF"
  █████╗ ███████╗████████╗██╗  ██╗███████╗██████╗ 
 ██╔══██╗██╔════╝╚══██╔══╝██║  ██║██╔════╝██╔══██╗
 ███████║█████╗     ██║   ███████║█████╗  ██████╔╝
 ██╔══██║██╔══╝     ██║   ██╔══██║██╔══╝  ██╔══██╗
 ██║  ██║███████╗   ██║   ██║  ██║███████╗██║  ██║
 ╚═╝  ╚═╝╚══════╝   ╚═╝   ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝
EOF
echo -e "${RESET}"
echo -e "${BOLD}Aether Native System & Network Cockpit${RESET} ${DIM}(v2.0.0)${RESET}"
echo -e "${DIM}Designed for Apple Silicon & Modern Unix Systems${RESET}\n"

# 1. Platform Detection
OS="$(uname -s)"
ARCH="$(uname -m)"
echo -e "${CYAN}›${RESET} Detected Platform: ${BOLD}${OS} (${ARCH})${RESET}"

if [ "$OS" != "Darwin" ] && [ "$OS" != "Linux" ]; then
    echo -e "${AMBER}[!] Note: Aether is optimized for macOS (Darwin) and Linux.${RESET}"
fi

# 2. Check Rust & Cargo Toolchain
if ! command -v cargo >/dev/null 2>&1; then
    echo -e "${RED}[✗] Rust / Cargo toolchain not detected.${RESET}"
    echo -e "    Aether requires Rust 1.80+ for native compilation."
    echo -e "\n${BOLD}Installing Rust via rustup...${RESET}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

CARGO_BIN="$HOME/.cargo/bin"
mkdir -p "$CARGO_BIN"

# 3. Compile and Install Aether + wifi-tui alias
echo -e "\n${CYAN}›${RESET} Compiling and installing Aether directly from repository..."
cargo install --git https://github.com/abdullah-binmadhi/Aether.git --force

# 4. Verify Local PATH and Alias Integration
LOCAL_BIN="$HOME/.local/bin"
mkdir -p "$LOCAL_BIN"

if [ -f "$CARGO_BIN/aether" ]; then
    # Ensure aether and wifi-tui binary alias exist in ~/.local/bin
    ln -sf "$CARGO_BIN/aether" "$LOCAL_BIN/aether" 2>/dev/null || cp "$CARGO_BIN/aether" "$LOCAL_BIN/aether"
    if [ -f "$CARGO_BIN/wifi-tui" ]; then
        ln -sf "$CARGO_BIN/wifi-tui" "$LOCAL_BIN/wifi-tui" 2>/dev/null || cp "$CARGO_BIN/wifi-tui" "$LOCAL_BIN/wifi-tui"
    fi
fi

# 5. Check PATH presence
PATH_OK=false
case ":$PATH:" in
    *:"$CARGO_BIN":*|*:"$LOCAL_BIN":*) PATH_OK=true ;;
esac

if [ "$PATH_OK" = false ]; then
    SHELL_PROFILE=""
    if [ -n "${ZSH_VERSION:-}" ] || [ -f "$HOME/.zshrc" ]; then
        SHELL_PROFILE="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
        SHELL_PROFILE="$HOME/.bashrc"
    fi

    if [ -n "$SHELL_PROFILE" ]; then
        echo -e "${CYAN}›${RESET} Adding ${CARGO_BIN} to ${SHELL_PROFILE}..."
        echo 'export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"' >> "$SHELL_PROFILE"
        echo 'alias wifi-tui="aether"' >> "$SHELL_PROFILE"
    fi
fi

echo -e "\n${GREEN}${BOLD}✓ Aether 2.0 successfully installed!${RESET}"
echo -e "───────────────────────────────────────────────────────"
echo -e " ${BOLD}Launch Cockpit :${RESET} ${GREEN}aether${RESET}"
echo -e " ${BOLD}Legacy Alias   :${RESET} ${DIM}wifi-tui${RESET} ${GREEN}→ aether${RESET}"
echo -e " ${BOLD}Spotlight Menu :${RESET} Press ${AMBER}[P]${RESET} anywhere in cockpit"
echo -e " ${BOLD}Phosphor Themes:${RESET} Press ${AMBER}[F2]${RESET} to cycle 10 themes"
echo -e "───────────────────────────────────────────────────────"
echo -e "${DIM}Run 'aether --help' for headless automation CLI flags.${RESET}\n"
