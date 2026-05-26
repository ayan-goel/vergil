#!/usr/bin/env bash
# Install Vergil's external toolchain dependencies.
#
# Required by `vergil verify`:
#   - Foundry  — Solidity dev framework (forge/cast/anvil)
#   - Halmos   — symbolic executor for EVM bytecode
#   - Slither  — Solidity static analyzer
#   - Gambit   — Solidity mutation tester
#   - Z3       — SMT solver (primary)
#   - cvc5     — SMT solver (portfolio)
#   - solc     — Solidity compiler (storage layout + SMTChecker CHC)
#
# Detects macOS vs Linux and uses the appropriate package manager.

set -euo pipefail

OS="$(uname -s)"

log() {
    printf '\033[1;34m[install-deps]\033[0m %s\n' "$1"
}

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        log "ERROR: $1 not found. $2"
        exit 1
    fi
}

# Prerequisites
require curl "Install curl first."
case "$OS" in
    Darwin)  require brew "Install Homebrew first: https://brew.sh" ;;
    Linux)   require apt-get "This script assumes Debian/Ubuntu apt-get." ;;
    *)       log "ERROR: unsupported OS: $OS"; exit 1 ;;
esac
require python3 "Install Python 3.10+."
require cargo "Install Rust via rustup: https://rustup.rs"

# 1. Foundry — provides forge, cast, anvil
if ! command -v forge >/dev/null 2>&1; then
    log "Installing Foundry"
    curl -L https://foundry.paradigm.xyz | bash
    "$HOME/.foundry/bin/foundryup"
else
    log "Foundry already installed: $(forge --version | head -1)"
fi

# 2. Halmos — symbolic executor
if ! command -v halmos >/dev/null 2>&1; then
    log "Installing Halmos"
    if command -v uv >/dev/null 2>&1; then
        uv tool install halmos
    else
        pip install --user halmos
    fi
else
    log "Halmos already installed: $(halmos --version 2>&1 | head -1)"
fi

# 3. Slither — Solidity static analyzer
if ! command -v slither >/dev/null 2>&1; then
    log "Installing Slither"
    pip install --user slither-analyzer
else
    log "Slither already installed: $(slither --version 2>&1 | head -1)"
fi

# 4. Gambit — Solidity mutation tester
if ! command -v gambit >/dev/null 2>&1; then
    log "Installing Gambit"
    cargo install --git https://github.com/Certora/gambit
else
    log "Gambit already installed: $(gambit --version 2>&1 | head -1)"
fi

# 5. Z3 and cvc5
case "$OS" in
    Darwin)
        for pkg in z3 cvc5 solidity; do
            if ! brew list "$pkg" >/dev/null 2>&1; then
                log "Installing $pkg via brew"
                brew install "$pkg"
            else
                log "$pkg already installed"
            fi
        done
        ;;
    Linux)
        log "Installing z3, cvc5, solc via apt-get (requires sudo)"
        sudo apt-get update
        sudo apt-get install -y z3 cvc5 solc
        ;;
esac

log "All dependencies installed. Run 'vergil doctor' once that's implemented to verify."
