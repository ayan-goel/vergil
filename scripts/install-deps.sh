#!/usr/bin/env bash
# Install Vergil's external toolchain dependencies.
#
# Phase 1 (verification) requires:
#   - Foundry  — Solidity dev framework (forge/cast/anvil)
#   - Halmos   — symbolic executor for EVM bytecode (pinned)
#   - Slither  — Solidity static analyzer (pinned)
#   - Z3       — SMT solver (primary)
#   - cvc5     — SMT solver (portfolio)
#   - solc     — Solidity compiler (storage layout + SMTChecker CHC)
#
# Phase 2 (LLM spec synthesis) additionally requires:
#   - Gambit   — Solidity mutation tester (pinned commit; Certora, Apache 2.0)
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

# uv is required for isolated Python tool installs (Halmos, Slither).
# Falls back to pip if uv is missing, but uv is strongly recommended:
# system Python (e.g. Anaconda 3.9) often has broken setuptools that breaks slither's deps.
require uv "Install uv: curl -LsSf https://astral.sh/uv/install.sh | sh"

# 2. Halmos — symbolic executor (pinned for fixture stability)
HALMOS_VERSION="${HALMOS_VERSION:-0.3.3}"
if ! command -v halmos >/dev/null 2>&1; then
    log "Installing Halmos $HALMOS_VERSION"
    uv tool install "halmos==$HALMOS_VERSION"
else
    log "Halmos already installed: $(halmos --version 2>&1 | head -1)"
fi

# 3. Slither — Solidity static analyzer (pinned for fixture stability)
SLITHER_VERSION="${SLITHER_VERSION:-0.11.0}"
if ! command -v slither >/dev/null 2>&1; then
    log "Installing Slither $SLITHER_VERSION"
    uv tool install "slither-analyzer==$SLITHER_VERSION"
else
    log "Slither already installed: $(slither --version 2>&1 | head -1)"
fi

# 4. Z3, cvc5, solc (system solvers + Solidity compiler)
case "$OS" in
    Darwin)
        for pkg in z3 solidity; do
            if ! brew list "$pkg" >/dev/null 2>&1; then
                log "Installing $pkg via brew"
                brew install "$pkg"
            else
                log "$pkg already installed"
            fi
        done
        # cvc5 ships as a Homebrew cask, not a formula
        if ! command -v cvc5 >/dev/null 2>&1; then
            log "Installing cvc5 via brew cask (cvc5/cvc5/cvc5)"
            brew tap cvc5/cvc5 >/dev/null 2>&1 || true
            brew install --cask cvc5/cvc5/cvc5
        else
            log "cvc5 already installed: $(cvc5 --version | head -1)"
        fi
        ;;
    Linux)
        log "Installing z3, cvc5, solc via apt-get (requires sudo)"
        sudo apt-get update
        sudo apt-get install -y z3 cvc5 solc
        ;;
esac

# 5. Gambit — Phase 2 (mutation testing as a spec-quality defense).
# Pinned to the commit verified during Phase 2 bootstrap (2026-05-26).
# Pass --skip-gambit to install everything except Gambit (e.g. restricted
# environments that cannot cargo-install from a third-party git URL).
GAMBIT_REV="${GAMBIT_REV:-072ff4c6}"
if [[ "${1:-}" == "--skip-gambit" ]]; then
    log "Skipping Gambit at user request (--skip-gambit). Phase 2 will run in degraded mode."
elif ! command -v gambit >/dev/null 2>&1; then
    log "Installing Gambit (commit $GAMBIT_REV)"
    cargo install --git https://github.com/Certora/gambit --rev "$GAMBIT_REV"
else
    log "Gambit already installed at $(command -v gambit)"
fi

log "All Phase-1 + Phase-2 dependencies installed. Run 'vergil doctor' to verify."
