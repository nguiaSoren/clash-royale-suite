# Deck Composition & Card Synergy Model

Attribute-based deck composition using simulated pairwise interactions — learning card synergy from mechanics, not just historical win rates.

## The Problem

Clash Royale decks are 8 cards. With 178 characters in the game, that's ~4.5 trillion possible decks. You can't brute-force which combinations work. Existing tools (RoyaleAPI, DeckShop, StatsRoyale) rank decks by observed win rate — they know *that* certain decks win, but not *why*. They can't evaluate a deck nobody has played.

This module learns *why* cards synergize at the attribute level using simulated interactions, then uses that understanding to compose and evaluate novel decks.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  PHASE 1: Multi-Order Synergy Measurement (simulator)        │
│                                                              │
│  Pairwise:  C(178,2) = 15,753 pairs      → seconds          │
│  Triples:   C(178,3) = 929,476 triples   → minutes          │
│                                                              │
│  For each group (A, B, ...) of size N:                       │
│    Score(each alone) = individual card vs baseline            │
│    Score(group)      = all N cards vs baseline                │
│    Synergy = Score(group) - Σ Score(individual)              │
│                                                              │
│  Positive = cards amplify each other                         │
│  Zero     = independent   │  Negative = anti-synergy         │
│                                                              │
│  Heuristic bots deploy cards (no AI needed for measurement)  │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  PHASE 2: Synergy Embedding Models (two MLPs)                │
│                                                              │
│  Pairwise model:  35d + 35d = 70d → 128 → 64 → 1           │
│    Training data: 15,753 pairwise synergy scores             │
│                                                              │
│  Triple model:    35d + 35d + 35d = 105d → 256 → 128 → 1   │
│    Training data: 929,476 triple synergy scores              │
│                                                              │
│  Both predict synergy for unseen combinations instantly      │
│  — including new cards added to the JSON after training      │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  PHASE 3: Synergy Cluster Discovery                          │
│                                                              │
│  From pairwise + triple scores, discover groups of 2-4      │
│  cards with high internal synergy (agglomerative clustering) │
│                                                              │
│  Example clusters:                                           │
│    Push:    Golem + Baby Dragon + Tornado                    │
│    Defense: Inferno Dragon + Tombstone                       │
│    Cycle:   Zap + Skeletons + Poison                         │
│                                                              │
│  Output: ~50-100 high-synergy clusters                       │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  PHASE 4: Deck Composition (cluster assembly + validate)     │
│                                                              │
│  Compose decks from 2-3 clusters that sum to 8 cards:        │
│    C(100,3) ≈ 161K cluster combos (tractable)               │
│                                                              │
│  Deck score:                                                 │
│    α × Σ pairwise_synergy (28 pairs in deck)                │
│  + β × Σ triple_synergy   (56 triples in deck)              │
│  + γ × Σ inter-cluster synergy (cluster pairs)              │
│                                                              │
│  Constraints: role coverage, elixir cost, air defense        │
│  Validate top-K candidates in full simulator self-play       │
└──────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────┐
│  PHASE 5: Hierarchical Policy (deck selector + RL agent)     │
│                                                              │
│  High-level: deck composition model selects 8 cards          │
│  Low-level:  RL agent plays matches with that deck           │
│                                                              │
│  Feedback loop:                                              │
│    Deck model proposes deck → RL agent plays 1000s of games  │
│    → win rate feeds back → deck model updates → repeat       │
└──────────────────────────────────────────────────────────────┘
```

## Role Classification (Derived from Data)

Roles are **not hardcoded**. They are derived from existing JSON fields. This is what makes the system data-driven — when Supercell adds a new card, its role is automatically inferred from its attributes.

### Derivation rules

| Role | Condition | Examples |
|------|-----------|---------|
| **Win condition (building targeter)** | `target_only_buildings == True` | Giant, Golem, Hog Rider, Balloon, Lava Hound, Royal Giant |
| **Tank** | `hitpoints > 2000 AND target_only_buildings == False` | PEKKA, Mega Knight, Skeleton King |
| **Mini tank** | `hitpoints > 1000 AND hitpoints <= 2000 AND speed <= 60` | Knight, Valkyrie, Dark Prince |
| **Glass cannon (ranged DPS)** | `range > 3000 AND hitpoints < 800` | Musketeer, Wizard, Electro Wizard |
| **Splash** | `area_damage_radius > 0` | Valkyrie, Baby Dragon, Witch, Bowler |
| **Air unit** | `flying_height > 0` | Baby Dragon, Balloon, Minions, Lava Hound |
| **Anti-air** | `attacks_air == True AND range > 3000` | Musketeer, Electro Wizard, Archers |
| **Spawner** | `spawn_character != None AND spawn_pause_time > 0` | Witch (skeletons), Night Witch (bats) |
| **Kamikaze (spirit)** | `kamikaze == True` | Fire Spirit, Ice Spirit, Heal Spirit, Electro Spirit |
| **Shield troop** | `shield_hitpoints > 0` | Dark Prince, Guards, Royal Recruits |
| **Charger** | `charge_range > 0` | Prince, Dark Prince, Battle Ram |
| **Dasher** | `dash_damage > 0` | Mega Knight, Bandit, Golden Knight |
| **Death value** | `death_spawn_character != None OR death_damage > 0` | Golem (Golemites + death damage), Giant Skeleton (bomb), Lava Hound (pups) |
| **Building targeter (troop only)** | `target_only_troops == True` | Ram Rider |
| **Stealth** | `hides_when_not_attacking == True` | Royal Ghost |

A single card can have multiple roles. Mega Knight is: tank + splash + dasher + death value (spawn splash). Dark Prince is: mini tank + splash + charger + shield troop. This multi-role composition is exactly what the embedding model learns to reason about.

### Why derive instead of hardcode?

If Supercell releases "Electric Knight" with `charge_range=300, area_damage_radius=1500, shield_hitpoints=200, flying_height=0`, the system automatically classifies it as charger + splash + shield troop, computes its attribute vector, and predicts synergy with every existing card — without retraining or manual labeling.

## Card Attribute Vector (35 fields from 297)

These are the fields that define how a card interacts with other cards mechanically. The other 262 fields are visual, cosmetic, per-level scaling arrays, or internal IDs.

### Combat identity

| Field | Type | What it tells the model |
|-------|------|------------------------|
| `damage` | int | Raw damage output per hit |
| `hit_speed` | int (ms) | Attack interval — lower = faster DPS |
| `load_time` | int (ms) | Windup before first hit |
| `range` | int | Melee (1200) vs ranged (5000-9000) |
| `area_damage_radius` | int | 0 = single target, >0 = splash (radius in game units) |
| `attacks_air` | bool | Can hit flying units |
| `attacks_ground` | bool | Can hit ground units |
| `target_only_buildings` | bool | Ignores troops, walks to tower (win condition) |
| `target_only_troops` | bool | Ignores buildings (Ram Rider bola) |
| `multiple_projectiles` | int | Hunter fires 10, Princess fires 5 |
| `multiple_targets` | int | Electro Wizard hits 2 simultaneously |
| `variable_damage2` | int | Second damage tier (Inferno Dragon ramp) |
| `variable_damage3` | int | Third damage tier |

### Survivability

| Field | Type | What it tells the model |
|-------|------|------------------------|
| `hitpoints` | int | Raw tankiness |
| `shield_hitpoints` | int | Shield HP (absorbs damage before main HP) |
| `collision_radius` | int | Physical body size |
| `mass` | int | Push resistance (Golem=20, Skeleton=1) |
| `speed` | int | Movement speed (30=slow, 60=medium, 120=very fast) |

### Special mechanics

| Field | Type | What it tells the model |
|-------|------|------------------------|
| `charge_range` | int | Distance to trigger charge (Prince: 300 units) |
| `charge_speed_multiplier` | int | Speed boost during charge (200 = 2x) |
| `damage_special` | int | Charge hit damage (usually 2x normal) |
| `dash_damage` | int | Dash attack damage (Mega Knight, Bandit) |
| `dash_min_range` | int | Minimum range to trigger dash |
| `dash_max_range` | int | Maximum range for dash |
| `kamikaze` | bool | Self-destructs on contact |
| `flying_height` | int | 0 = ground, >0 = air unit |
| `jump_enabled` | bool | Can jump river (no bridge needed) |
| `ignore_pushback` | bool | Immune to knockback |
| `hides_when_not_attacking` | bool | Stealth / invisibility |

### Death value

| Field | Type | What it tells the model |
|-------|------|------------------------|
| `death_spawn_character` | str/null | What spawns on death |
| `death_spawn_count` | int | How many spawn |
| `death_damage` | int | AoE damage on death |
| `death_damage_radius` | int | Radius of death damage |

### Spawner / support

| Field | Type | What it tells the model |
|-------|------|------------------------|
| `spawn_character` | str/null | What it periodically spawns |
| `spawn_number` | int | How many per wave |
| `spawn_pause_time` | int (ms) | Interval between waves |
| `buff_on_damage` | str/null | Buff applied on hit (E-Wiz stun) |
| `buff_when_not_attacking` | str/null | Idle buff (Battle Healer self-heal) |

### Economy

| Field | Type | What it tells the model |
|-------|------|------------------------|
| `elixir` | int | Cost — critical for synergy (cheap + expensive pairings) |

## Micro-Scenario Design

### No AI needed

The micro-scenarios use **heuristic bots**, not trained agents. The bot's only job is to deploy cards — the synergy score measures mechanical interaction, not strategic play.

### Deployment heuristics (derived from role)

| Role (derived) | Deployment rule |
|----------------|----------------|
| Win condition (`target_only_buildings`) | Deploy at bridge, center lane |
| Tank (`hp > 2000`) | Deploy behind king tower (back placement) |
| Ranged / glass cannon (`range > 3000`) | Deploy behind the paired card |
| Splash (`area_damage_radius > 0`) | Deploy behind the paired card |
| Kamikaze / spirit | Deploy at bridge toward enemy cluster |
| Spawner troop | Deploy behind king tower |
| Air unit (`flying_height > 0`) | Deploy behind paired card (no bridge constraint) |

These rules are simple functions of the same JSON fields used for role derivation. No manual per-card logic.

### Scoring formula

```
score(side) = tower_damage_dealt - tower_damage_taken
              ─────────────────────────────────────────
                        total_elixir_spent

synergy(A, B) = score(A+B) - score(A_alone) - score(B_alone)
```

Normalized to [0, 1] across all pairs for model training. Negative synergy (anti-synergy) maps to values below 0.5.

### Scenario parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Simulation ticks | 200 (10 seconds) | Long enough for troops to engage, short enough for throughput |
| Runs per pair/triple | 100 | Statistical robustness with varied placements |
| Baseline opponent | Equivalent-elixir Knights | Standardized, no special mechanics, predictable DPS |
| Total pairs | C(178, 2) = 15,753 | Every unique character pair |
| Total triples | C(178, 3) = 929,476 | Every unique character triple |
| Pair runtime | 15,753 × 100 × ~1.5μs ≈ 2.4 seconds | Trivial |
| Triple runtime | 929,476 × 100 × ~1.5μs ≈ 2.3 minutes | Still tractable |

This is why multi-order models + cluster discovery matter: learn synergy patterns from pairs and triples (which you can exhaustively simulate), discover natural card clusters, then compose 8-card decks from 2-3 clusters — reducing a 4.5-trillion search to ~166K cluster combinations.

## Embedding Models (Two MLPs)

Pairwise synergy misses higher-order interactions. Golem + Baby Dragon is a good pair. Golem + Tornado is a good pair. But Golem + Baby Dragon + Tornado is better than the sum of those two pairs — Tornado clumps enemies, Baby Dragon splashes the clump, Golem tanks for both. That three-way interaction isn't captured by summing pairwise scores.

Two separate models handle different interaction orders:

### Pairwise model

```
Input:  card_A_attrs (35d) ⊕ card_B_attrs (35d) = 70d
        ↓
Dense:  70 → 128 (ReLU)
        ↓
Dense:  128 → 64 (ReLU)
        ↓
Dense:  64 → 1 (Sigmoid)
        ↓
Output: predicted pairwise synergy [0, 1]
```

| Detail | Value |
|--------|-------|
| Training data | 15,753 pairwise synergy scores (exhaustive) |
| Input | 70-dimensional (35 per card, concatenated) |
| Runtime | C(178,2) = 15K pairs × 100 runs × ~1.5μs = seconds |

### Triple model

```
Input:  card_A (35d) ⊕ card_B (35d) ⊕ card_C (35d) = 105d
        ↓
Dense:  105 → 256 (ReLU)
        ↓
Dense:  256 → 128 (ReLU)
        ↓
Dense:  128 → 1 (Sigmoid)
        ↓
Output: predicted triple synergy [0, 1]
```

| Detail | Value |
|--------|-------|
| Training data | 929,476 triple synergy scores (exhaustive) |
| Input | 105-dimensional (35 per card, concatenated) |
| Runtime | C(178,3) = 929K triples × 100 runs × ~1.5μs = minutes |

### Why two models instead of one?

A single model can't handle variable input sizes cleanly. The pairwise model takes 70d input, the triple model takes 105d. Keeping them separate means each is optimized for its interaction order, and the deck scoring formula combines them with learned weights:

```
deck_score = α × Σ pairwise_model(i,j) for all C(8,2) = 28 pairs
           + β × Σ triple_model(i,j,k)  for all C(8,3) = 56 triples
```

The weights α and β are learned from full-deck validation in the simulator — which combination of pairwise and triple signals best predicts actual deck win rates. In practice, most Clash Royale synergy is explained by 2-card and 3-card interactions. A tank-support pair, a spell-bait triple, a push combo. You rarely need 4+ card synergy to explain why a deck works.

### Scaling beyond triples

| Combination size | Count | Feasibility | Approach |
|------------------|-------|-------------|----------|
| Pairs C(178,2) | 15,753 | Simulate all (seconds) | Exhaustive |
| Triples C(178,3) | 929,476 | Simulate all (minutes) | Exhaustive |
| Quads C(178,4) | ~40 million | Top candidates only | Predict from pair + triple models |
| Full decks C(178,8) | ~4.5 trillion | **Cannot brute-force** | Cluster-based composition |

### Training

Both models share the same training protocol:

| Detail | Value |
|--------|-------|
| Output | Scalar synergy score, normalized [0, 1] |
| Loss | MSE between predicted and simulated synergy |
| Validation | Hold out 20% of combinations |
| Training time | Seconds (pairwise), minutes (triples) |

### Inference

28 pairwise predictions + 56 triple predictions = 84 forward passes through tiny MLPs — microseconds total. You can evaluate millions of candidate decks per second.

## Synergy Cluster Discovery

A Clash Royale deck isn't 8 random cards — it's 2-3 synergy clusters that work together. Real decks are built around functional groups:

| Cluster | Cards | Why they synergize |
|---------|-------|--------------------|
| **Push** | Golem + Baby Dragon + Tornado | Golem tanks, Baby Dragon splashes behind it, Tornado clumps enemies into the splash |
| **Defense** | Inferno Dragon + Tombstone | Tombstone distracts while Inferno ramps damage on the tank |
| **Cycle/Spell** | Zap + Skeletons + Poison | Cheap cycle cards to rotate back to win condition + spell pressure |

That's 3 + 2 + 3 = 8 cards. The deck works because each cluster has high internal synergy AND the clusters complement each other — the push cluster needs the defense cluster to survive until you accumulate enough elixir.

### How clusters are discovered

From the pairwise and triple synergy scores, use agglomerative clustering to find groups of 2-4 cards with high internal synergy:

1. Build a synergy graph: nodes are cards, edge weights are pairwise synergy scores
2. Find dense subgraphs (cliques or near-cliques) of size 2-4 with high average synergy
3. Filter to clusters where triple synergy confirms the group interaction (not just good pairs)
4. Tag each cluster by its dominant role composition (push, defense, spell, cycle)

Output: ~50-100 high-synergy clusters, each with 2-4 cards and a role tag.

### Cluster examples (expected)

| Type | Expected clusters | Role composition |
|------|-------------------|-----------------|
| **Beatdown push** | Golem + Night Witch + Baby Dragon | Win condition + spawner + splash |
| **Bridge spam** | Bandit + Battle Ram + Dark Prince | Dasher + charger + charger/shield |
| **Spell bait** | Goblin Gang + Skeleton Army + Goblin Barrel | Swarm + swarm + deploy-on-enemy |
| **Air push** | Lava Hound + Balloon + Minions | Air tank + air win condition + air DPS |
| **Control spells** | Poison + Tornado + Zap | DOT + pull + stun |
| **Cycle defense** | Skeletons + Ice Spirit + Cannon | Cheap distraction + freeze + building |

## Deck Composition (Cluster Assembly)

Instead of searching 4.5 trillion individual card combinations, compose decks from discovered clusters:

### Step 1: Score inter-cluster synergy

The same way you measure pairwise card synergy, measure pairwise *cluster* synergy in the simulator. Does push cluster + defense cluster perform better together than the sum of their individual scores? Run it. Now you have cluster-level synergy scores.

### Step 2: Assemble 2-3 clusters into 8-card decks

With ~100 clusters and 2-3 per deck, the search space is:
- C(100, 2) = 4,950 two-cluster combos
- C(100, 3) = 161,700 three-cluster combos

That's ~166K combinations — trivially searchable. Filter by:
- Total cards = 8 (clusters must sum exactly)
- Role coverage constraints (at least 1 win condition, 1 anti-air, 1 splash)
- Elixir average between 2.8 and 4.2

### Step 3: Rank by combined score

```
deck_score = α × Σ pairwise_synergy  (28 intra-deck pairs)
           + β × Σ triple_synergy    (56 intra-deck triples)
           + γ × Σ inter_cluster_synergy (cluster pair scores)
```

Weights α, β, γ learned from simulator validation.

### Step 4: Validate top candidates

Top-K decks from the search get full self-play validation in the simulator. The composition model proposes, the simulator disposes.

### Search refinement strategies

After cluster-based assembly produces top candidates, refine with:

**Card swap:** For each deck, try replacing each card with every other card not in the deck. Keep the swap if deck_score improves. This handles edge cases where a cluster is mostly right but one card is suboptimal.

**Genetic algorithm:** Initialize population from top cluster-assembled decks. Crossover: combine clusters from different high-scoring decks. Mutation: swap one card. Evolve over generations.

## Deck Role Constraints

Real decks need role coverage. These constraints are derived from the same JSON fields used for role classification:

| Constraint | Rule | Why |
|------------|------|-----|
| Win condition | At least 1 card with `target_only_buildings == True` | Need a way to damage towers |
| Anti-air | At least 1 card with `attacks_air == True AND range > 3000` | Must counter Balloon, Lava Hound |
| Splash | At least 1 card with `area_damage_radius > 0` | Must handle swarms (Skeleton Army, Minion Horde) |
| Spell | At least 1 spell card | Direct damage, utility |
| Elixir average | Between 2.8 and 4.2 | Too cheap = no power, too expensive = can't cycle |
| Max buildings | At most 2 | More than 2 is a meme, not a strategy |

These constraints prune the search space before scoring, ensuring every candidate deck is structurally viable.

## Hierarchical Policy

The two-level structure that ties deck composition to gameplay:

```
HIGH-LEVEL POLICY (Phases 1-4)           LOW-LEVEL POLICY (Phase 5)
─────────────────────────────           ──────────────────────────────
Input:  card pool + meta data            Input:  game state observation
Output: 8-card deck (from clusters)      Output: card to play + (x, y)
Trains: synergy models + clustering      Trains: self-play in simulator
Updates: when meta shifts or             Updates: every training batch
         new cards are added

            deck ──────────────────→ RL agent uses this deck
            win rate ←───────────── RL agent reports results
```

The high-level policy doesn't need RL — it's synergy models + cluster discovery + constrained search. The low-level policy is the RL agent trained via self-play in the simulator. They improve each other iteratively: better decks → better training signal for RL → higher win rates → better feedback for deck model → better cluster selection → repeat.

## Novelty

Existing deck recommendation systems (RoyaleAPI, DeckShop, StatsRoyale) are **statistical**: they rank decks by observed win rate from player data. They can only recommend decks that humans have already played.

This approach is **generative and causal**: it learns *why* cards synergize from attribute-level mechanical interactions verified in simulation, then composes novel decks that may never have appeared on ladder. It can evaluate new cards the moment they're added to the JSON — before any player has tried them.

This is the difference between "these decks have won" and "these cards should work together because their mechanics complement each other."

📄 **[Novelty Analysis →](https://nguiasoren.github.io/clash-royale-suite/cr-deck-synergy/deck_synergy_comparative_study.html)** — detailed positioning against existing deck tools, game AI research, and related work in combinatorial optimization.

## File Structure

```
cr-deck-synergy/
├── README.md                    (this file)
├── synergy/
│   ├── attribute_vector.py      (extract 35-field vector from card JSON)
│   ├── role_classifier.py       (derive roles from attributes)
│   ├── micro_scenario.py        (scripted pairwise + triple simulation runner)
│   ├── scoring.py               (synergy score computation)
│   ├── pairwise_model.py        (pairwise MLP training + inference)
│   └── triple_model.py          (triple MLP training + inference)
├── clusters/
│   ├── discover.py              (agglomerative clustering from synergy graph)
│   ├── inter_cluster_synergy.py (measure cluster-pair interactions in simulator)
│   └── tag_roles.py             (assign role composition to each cluster)
├── deck/
│   ├── assemble.py              (cluster-based deck assembly)
│   ├── constraints.py           (role coverage, elixir bounds)
│   ├── refine.py                (card swap + genetic refinement)
│   └── evaluate.py              (full simulator validation)
├── data/
│   ├── pairwise_synergy.json    (15K measured pairwise scores)
│   ├── triple_synergy.json      (929K measured triple scores)
│   ├── clusters.json            (discovered synergy clusters)
│   └── deck_candidates.json     (top-K composed decks)
└── notebooks/
    └── analysis.ipynb           (synergy heatmaps, cluster visualization)
```

## Dependencies

| Package | Purpose |
|---------|---------|
| `cr_engine` (PyO3) | Simulator for micro-scenarios |
| `torch` | MLP training |
| `numpy` | Attribute vector processing |
| `pandas` | Synergy data management |
| `matplotlib` | Synergy heatmaps |
| `networkx` | Synergy graph construction + cluster discovery |
| `plotly` / `d3.js` | Interactive synergy visualization |

## Interactive Synergy Atlas

Once pairwise and triple synergy scores are computed, the results will be published as an interactive force-directed graph — a navigable atlas of every card interaction in the game.

**Pairwise graph:** Each node is a card, each edge is weighted by synergy score. High-synergy pairs pull together, anti-synergy pairs repel. Color-coded by derived role (tank, splash, win condition, etc.). Hover to see the synergy score and attribute comparison.

**Triple/cluster graph:** Each node is a discovered synergy cluster (2-4 cards). Edge weights represent inter-cluster synergy. Clicking a cluster node expands it to show its constituent cards and their internal synergy edges. This is the deck-builder's view — pick 2-3 clusters, see how they connect, assemble a deck visually.

Both graphs will be hosted as interactive HTML pages via GitHub Pages, consistent with the existing writeup format.
