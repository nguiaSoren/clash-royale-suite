"""
RL training loop — wraps Rust engine in a gym-like env.

Phase 5 implementation. This file provides the environment interface
that will be used with stable-baselines3 or similar RL frameworks.

Usage (Phase 5):
    from training import CREnv
    env = CREnv(data, deck1, deck2)
    obs = env.reset()
    action = agent.predict(obs)
    obs, reward, done, info = env.step(action)
"""

# TODO: Phase 5 — Full implementation
#
# The env will:
# 1. Wrap a cr_engine.Match instance
# 2. Expose observation as a flat numpy array (for neural net input)
# 3. Accept discrete actions (hand_index × grid_position)
# 4. Return shaped reward (tower damage dealt, crowns, elixir efficiency)
# 5. Step the engine N ticks between decisions (e.g., 10 ticks = 0.5s)
#
# Observation space (approx 50 floats):
#   - my_elixir (normalized 0-1)
#   - hand card IDs (4 × one-hot or embedding index)
#   - hand elixir costs (4 floats)
#   - my tower HP (3 × normalized)
#   - opp tower HP (3 × normalized)
#   - my troop count (normalized)
#   - opp troop count (normalized)
#   - phase (one-hot: regular, double, overtime, sudden death)
#   - time remaining (normalized)
#
# Action space (discrete):
#   - 0: do nothing
#   - 1-4: play hand[0-3] at left bridge
#   - 5-8: play hand[0-3] at right bridge
#   - 9-12: play hand[0-3] at left back
#   - 13-16: play hand[0-3] at right back
#   Total: 17 discrete actions
#
# Reward shaping:
#   - +1.0 per crown scored
#   - -1.0 per crown lost
#   - +10.0 for winning
#   - -10.0 for losing
#   - Small penalty per tick for elixir waste (at 10 elixir)
