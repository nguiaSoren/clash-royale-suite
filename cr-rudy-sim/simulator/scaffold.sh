#!/bin/bash
# Scaffold the Clash Royale Simulator project structure
# Run from the simulator/ root directory (where data/ and scripts/ already exist)

set -e
echo "Scaffolding simulator project..."

# ── Create directories ──
mkdir -p engine/src
mkdir -p python

# ══════════════════════════════════════════════
# RUST ENGINE FILES
# ══════════════════════════════════════════════

cat > engine/Cargo.toml << 'TOML'
[package]
name = "cr_engine"
version = "0.1.0"
edition = "2021"

[lib]
name = "cr_engine"
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.22", features = ["extension-module"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
rayon = "1.10"

[profile.release]
opt-level = 3
lto = true
TOML

cat > engine/src/lib.rs << 'RUST'
//! Clash Royale Simulator Engine
//! PyO3 entry point — exposes Rust functions to Python

use pyo3::prelude::*;

mod data_types;
mod game_state;
mod entities;
mod combat;
mod engine;
mod evo_system;
mod hero_system;

#[pymodule]
fn cr_engine(_py: Python, m: &PyModule) -> PyResult<()> {
    // TODO: add exported functions as we build them
    Ok(())
}
RUST

cat > engine/src/data_types.rs << 'RUST'
//! Card stats structs — deserialized from the 6 JSON data files
//!
//! GameData is loaded once at startup and passed as &GameData
//! (immutable borrow) to every match. Zero copies during simulation.

use std::collections::HashMap;
use serde::Deserialize;

// TODO: Phase 1
RUST

cat > engine/src/game_state.rs << 'RUST'
//! Arena state — towers, elixir, entity pools, time/phase
//!
//! GameState is mutable and unique per match.

// TODO: Phase 1
RUST

cat > engine/src/entities.rs << 'RUST'
//! Game entities — Troop, Building, Projectile, Spell
//!
//! Flat Vec<Entity> layout for cache-friendly iteration.

// TODO: Phase 1
RUST

cat > engine/src/combat.rs << 'RUST'
//! Combat system — targeting, damage, buff application
//!
//! Pure functions on &mut GameState + &GameData.

// TODO: Phase 2
RUST

cat > engine/src/engine.rs << 'RUST'
//! Main tick loop — the heart of the simulation
//!
//! 50ms per tick, 4800 ticks per 4-minute match.
//! Order: elixir → actions → entities → projectiles → spells → cleanup → win check

// TODO: Phase 1
RUST

cat > engine/src/evo_system.rs << 'RUST'
//! Evolution ability handlers — one per effect_type enum variant
//!
//! Data from evo_hero_abilities.json → GameData.evolutions

// TODO: Phase 3
RUST

cat > engine/src/hero_system.rs << 'RUST'
//! Hero ability handlers — manual activation, costs elixir
//!
//! Data from evo_hero_abilities.json → GameData.heroes

// TODO: Phase 3
RUST

# ══════════════════════════════════════════════
# PYTHON FILES
# ══════════════════════════════════════════════

cat > python/__init__.py << 'PY'
PY

cat > python/data_loader.py << 'PY'
"""
Load game data and pass to the Rust engine.

Usage:
    from data_loader import load_game_data
    game_data = load_game_data("data/")
"""
# TODO: Phase 4
PY

cat > python/ai_agent.py << 'PY'
"""
Agent interface for card placement decisions.

Agents:
    RandomAgent    — plays random cards (baseline)
    RuleBasedAgent — simple heuristics (sanity check)
    RLAgent        — neural network (training target)
"""
# TODO: Phase 4 — RandomAgent, RuleBasedAgent
# TODO: Phase 5 — RLAgent with gym-like interface
PY

cat > python/match_runner.py << 'PY'
"""
Orchestrate matches and collect results.

Usage:
    results = run_batch(game_data, matchups, n_per_matchup=100)
"""
# TODO: Phase 4
PY

cat > python/training.py << 'PY'
"""
RL training loop — wraps Rust engine in a gym-like env.
"""
# TODO: Phase 5
PY

cat > python/analytics.py << 'PY'
"""
Win rate analysis, balance testing, visualization.
"""
# TODO: Phase 4
PY

# ══════════════════════════════════════════════
# BUILD CONFIG
# ══════════════════════════════════════════════

cat > pyproject.toml << 'TOML'
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "cr_engine"
requires-python = ">=3.10"

[tool.maturin]
manifest-path = "engine/Cargo.toml"
python-source = "python"
TOML

# ══════════════════════════════════════════════
# VERIFY
# ══════════════════════════════════════════════

echo ""
echo "Done! Full project structure:"
echo ""
tree -L 4 -F --dirsfirst -I '__pycache__'
echo ""
echo "Next steps:"
echo "  cd engine && cargo check       # verify Rust compiles"
echo "  cd .. && maturin develop       # build + install Python module"
