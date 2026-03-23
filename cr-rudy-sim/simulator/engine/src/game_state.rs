//! Arena state — towers, elixir, entity pools, time/phase
//!
//! GameState is mutable and unique per match.

// TODO: Phase 1
//! Arena layout, tower state, elixir management, and per-match game state.
//!
//! Coordinate system (matching Clash Royale internal units):
//!   - Origin (0, 0) at centre of arena
//!   - X: -8400 (left edge) → +8400 (right edge)   — 16 800 total
//!   - Y: -15400 (bottom edge, player 1) → +15400 (top edge, player 2)
//!   - 1 tile ≈ 600 internal units
//!
//! All coordinates and distances are in these integer units.
//! Time is measured in **ticks** (1 tick = 50 ms, 20 ticks/sec).

use crate::entities::{Entity, EntityId, Team};

// =========================================================================
// Arena constants
// =========================================================================

/// Arena half-width (centre to edge).
pub const ARENA_HALF_W: i32 = 8_400;
/// Arena half-height (centre to top/bottom edge).
pub const ARENA_HALF_H: i32 = 15_400;

/// The river Y band — troops cannot cross until a bridge is reached.
pub const RIVER_Y_MIN: i32 = -1_200;
pub const RIVER_Y_MAX: i32 = 1_200;

/// One tile in internal units. Used for building grid snapping and setback.
/// All arena dimensions are multiples of this value.
pub const TILE_SIZE: i32 = 600;

/// Minimum distance from the river edge for building placement.
/// In real CR, buildings cannot be placed within 1 tile of the river.
/// Derived from TILE_SIZE — not hardcoded to a specific arena layout.
pub const BUILDING_RIVER_SETBACK: i32 = TILE_SIZE;

/// Bridge centres (two bridges, symmetric about X=0).
pub const BRIDGE_LEFT_X: i32 = -5_100;
pub const BRIDGE_RIGHT_X: i32 = 5_100;
pub const BRIDGE_HALF_W: i32 = 1_200;

// Tower positions (Y positive = player 2 side)
pub const P1_KING_POS: (i32, i32) = (0, -13_000);
pub const P1_PRINCESS_LEFT_POS: (i32, i32) = (-5_100, -10_200);
pub const P1_PRINCESS_RIGHT_POS: (i32, i32) = (5_100, -10_200);

pub const P2_KING_POS: (i32, i32) = (0, 13_000);
pub const P2_PRINCESS_LEFT_POS: (i32, i32) = (-5_100, 10_200);
pub const P2_PRINCESS_RIGHT_POS: (i32, i32) = (5_100, 10_200);

// Tower hitpoints (tournament standard — level 11)
pub const KING_TOWER_HP: i32 = 4_824;
pub const PRINCESS_TOWER_HP: i32 = 3_052;

/// King tower activation range — activates when a troop enters this radius.
pub const KING_ACTIVATION_RANGE: i32 = 3_600;
/// Princess tower sight range.
pub const PRINCESS_TOWER_RANGE: i32 = 7_500;
/// King tower attack range.
pub const KING_TOWER_RANGE: i32 = 7_000;

/// Tower attack damage (tournament standard).
pub const PRINCESS_TOWER_DMG: i32 = 109;
pub const KING_TOWER_DMG: i32 = 109;
/// Tower hit speed in ticks (0.8 s = 16 ticks).
pub const TOWER_HIT_SPEED: i32 = 16;
/// Tower initial attack delay in ticks (0.4s = 8 ticks).
/// In real CR, towers have a load time before their first shot when a troop
/// enters range. This prevents towers from firing instantly when a troop
/// appears inside range (e.g., Goblin Barrel). Derived from observed
/// tower behavior (~0.4s first-hit delay). Applied as the starting
/// attack_cooldown in TowerState::new_king() and new_princess().
pub const TOWER_LOAD_FIRST_HIT: i32 = 8;

// =========================================================================
// Match timing
// =========================================================================

/// Ticks per second.
pub const TICKS_PER_SEC: i32 = 20;
/// Regular time: 3 min = 180 s.
pub const REGULAR_TIME_TICKS: i32 = 180 * TICKS_PER_SEC;
/// Overtime: 2 min = 120 s  (added after regular time).
pub const OVERTIME_TICKS: i32 = 120 * TICKS_PER_SEC;
/// Sudden death / tiebreaker: 3 min.
pub const SUDDEN_DEATH_TICKS: i32 = 180 * TICKS_PER_SEC;
/// Double-elixir starts at 2:00 remaining in regular time → tick 60*20 = 1200.
pub const DOUBLE_ELIXIR_TICK: i32 = 60 * TICKS_PER_SEC;
/// Triple-elixir in overtime.
pub const TRIPLE_ELIXIR_TICK: i32 = REGULAR_TIME_TICKS;
/// Maximum match length.
pub const MAX_MATCH_TICKS: i32 = REGULAR_TIME_TICKS + OVERTIME_TICKS + SUDDEN_DEATH_TICKS;

// =========================================================================
// Elixir constants
// =========================================================================

/// Starting elixir (×10000 for sub-tick precision).
/// 5 elixir = 50000 internal units.
pub const STARTING_ELIXIR: i32 = 50_000;
/// Max elixir (×10000).
/// 10 elixir = 100000 internal units.
pub const MAX_ELIXIR: i32 = 100_000;
/// Base elixir generation rate per tick (×10000 fixed-point).
/// Real CR: 1 elixir per 2.8s = 1 elixir per 56 ticks.
/// In ×10000 space: 10000 units = 1 elixir. Rate = 10000/56 ≈ 178.6 per tick.
/// 179 gives 1 elixir per 55.9 ticks (2.795s) — near-perfect match.
pub const BASE_ELIXIR_RATE: i32 = 179;

// =========================================================================
// TowerState
// =========================================================================

/// State of a single tower (king or princess).
#[derive(Debug, Clone)]
pub struct TowerState {
    pub hp: i32,
    pub max_hp: i32,
    pub pos: (i32, i32),
    pub alive: bool,
    pub activated: bool, // Only meaningful for king tower
    pub attack_cooldown: i32, // Ticks until next attack
    /// Hitspeed multiplier from active spell zones (Rage on tower).
    /// 100 = normal speed, 135 = Rage (+35% faster). Recomputed each tick
    /// by tick_tower_buffs() from overlapping friendly spell zones.
    /// Affects attack_cooldown reset: TOWER_HIT_SPEED * 100 / rage_hitspeed.
    pub rage_hitspeed: i32,
}

impl TowerState {
    pub fn new_king(pos: (i32, i32)) -> Self {
        TowerState {
            hp: KING_TOWER_HP,
            max_hp: KING_TOWER_HP,
            pos,
            alive: true,
            activated: false,
            attack_cooldown: TOWER_LOAD_FIRST_HIT,
            rage_hitspeed: 100, // No buff active — normal speed
        }
    }

    pub fn new_princess(pos: (i32, i32)) -> Self {
        TowerState {
            hp: PRINCESS_TOWER_HP,
            max_hp: PRINCESS_TOWER_HP,
            pos,
            alive: true,
            activated: true,
            attack_cooldown: TOWER_LOAD_FIRST_HIT,
            rage_hitspeed: 100, // No buff active — normal speed
        }
    }

    /// Apply damage to this tower. Returns true if the tower just died.
    pub fn take_damage(&mut self, dmg: i32) -> bool {
        if !self.alive {
            return false;
        }
        self.hp -= dmg;
        if self.hp <= 0 {
            self.hp = 0;
            self.alive = false;
            return true;
        }
        false
    }
}

// =========================================================================
// PlayerState
// =========================================================================

/// Per-player state within a match.
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub team: Team,
    /// Elixir ×100 for sub-tick precision.
    pub elixir: i32,
    pub king: TowerState,
    pub princess_left: TowerState,
    pub princess_right: TowerState,
    /// Cards in hand (indices into the deck).
    pub hand: Vec<usize>,
    /// Remaining deck (card keys).
    pub deck: Vec<String>,
    /// Next card index in the deck cycle.
    pub next_card_idx: usize,
    /// Crown count (princess tower = 1, king = 3).
    pub crowns: i32,
    /// Last played card info for Mirror: (card_key, elixir_cost, level).
    /// None if no card has been played yet this match.
    pub last_played_card: Option<(String, i32, usize)>,
}

impl PlayerState {
    pub fn new(team: Team, deck: Vec<String>) -> Self {
        let (king_pos, pl, pr) = match team {
            Team::Player1 => (P1_KING_POS, P1_PRINCESS_LEFT_POS, P1_PRINCESS_RIGHT_POS),
            Team::Player2 => (P2_KING_POS, P2_PRINCESS_LEFT_POS, P2_PRINCESS_RIGHT_POS),
        };

        // Initial hand = first 4 cards, next = index 4
        let hand = if deck.len() >= 4 {
            vec![0, 1, 2, 3]
        } else {
            (0..deck.len()).collect()
        };

        PlayerState {
            team,
            elixir: STARTING_ELIXIR,
            king: TowerState::new_king(king_pos),
            princess_left: TowerState::new_princess(pl),
            princess_right: TowerState::new_princess(pr),
            hand,
            deck,
            next_card_idx: 4,
            crowns: 0,
            last_played_card: None,
        }
    }

    /// Whole elixir available (for AI decision-making).
    pub fn elixir_whole(&self) -> i32 {
        self.elixir / 10_000
    }

    /// Try to spend `cost` elixir (whole units). Returns true on success.
    pub fn spend_elixir(&mut self, cost: i32) -> bool {
        let cost_fixed = cost * 10_000;
        if self.elixir >= cost_fixed {
            self.elixir -= cost_fixed;
            true
        } else {
            false
        }
    }

    /// Tick elixir generation. `multiplier`: 1 = normal, 2 = double, 3 = triple.
    pub fn tick_elixir(&mut self, multiplier: i32) {
        self.elixir += BASE_ELIXIR_RATE * multiplier;
        if self.elixir > MAX_ELIXIR {
            self.elixir = MAX_ELIXIR;
        }
    }

    /// Cycle card: remove `hand_idx` from hand, push `next_card_idx`, advance cycle.
    pub fn cycle_card(&mut self, hand_idx: usize) {
        if hand_idx < self.hand.len() && !self.deck.is_empty() {
            self.hand[hand_idx] = self.next_card_idx % self.deck.len();
            self.next_card_idx += 1;
        }
    }

    /// Count crowns from tower state.
    pub fn recount_opponent_crowns(&self) -> i32 {
        let mut c = 0;
        if !self.princess_left.alive { c += 1; }
        if !self.princess_right.alive { c += 1; }
        if !self.king.alive { c += 3; }
        c
    }

    /// Returns true if king tower is destroyed (instant loss).
    pub fn is_eliminated(&self) -> bool {
        !self.king.alive
    }

    /// Return all alive tower positions with their ranges and damages.
    pub fn alive_towers(&self) -> Vec<(i32, i32, i32, i32, bool)> {
        // Returns: (x, y, range, damage, is_king)
        let mut towers = Vec::new();
        if self.princess_left.alive {
            towers.push((
                self.princess_left.pos.0,
                self.princess_left.pos.1,
                PRINCESS_TOWER_RANGE,
                PRINCESS_TOWER_DMG,
                false,
            ));
        }
        if self.princess_right.alive {
            towers.push((
                self.princess_right.pos.0,
                self.princess_right.pos.1,
                PRINCESS_TOWER_RANGE,
                PRINCESS_TOWER_DMG,
                false,
            ));
        }
        if self.king.alive && self.king.activated {
            towers.push((
                self.king.pos.0,
                self.king.pos.1,
                KING_TOWER_RANGE,
                KING_TOWER_DMG,
                true,
            ));
        }
        towers
    }
}

// =========================================================================
// MatchPhase
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchPhase {
    Regular,
    DoubleElixir,
    Overtime,       // triple elixir
    SuddenDeath,
}

// =========================================================================
// MatchResult
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    Player1Win,
    Player2Win,
    Draw,
    InProgress,
}

// =========================================================================
// GameState — the full match state
// =========================================================================

/// Complete state of a single match. Mutated each tick by the engine.
#[derive(Debug, Clone)]
pub struct GameState {
    pub tick: i32,
    pub phase: MatchPhase,
    pub result: MatchResult,
    pub player1: PlayerState,
    pub player2: PlayerState,
    /// Flat entity pool — all troops, buildings, projectiles, spells on the field.
    pub entities: Vec<Entity>,
    /// Monotonically increasing entity ID counter.
    next_entity_id: u32,
}

impl GameState {
    /// Create a new match with the given decks.
    pub fn new(deck1: Vec<String>, deck2: Vec<String>) -> Self {
        GameState {
            tick: 0,
            phase: MatchPhase::Regular,
            result: MatchResult::InProgress,
            player1: PlayerState::new(Team::Player1, deck1),
            player2: PlayerState::new(Team::Player2, deck2),
            entities: Vec::with_capacity(64),
            next_entity_id: 1,
        }
    }

    /// Allocate a fresh entity ID.
    pub fn alloc_id(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id += 1;
        id
    }

    /// Get current elixir multiplier based on phase.
    pub fn elixir_multiplier(&self) -> i32 {
        match self.phase {
            MatchPhase::Regular => 1,
            MatchPhase::DoubleElixir => 2,
            MatchPhase::Overtime | MatchPhase::SuddenDeath => 3,
        }
    }

    /// Update match phase based on current tick.
    pub fn update_phase(&mut self) {
        self.phase = if self.tick < DOUBLE_ELIXIR_TICK {
            MatchPhase::Regular
        } else if self.tick < REGULAR_TIME_TICKS {
            MatchPhase::DoubleElixir
        } else if self.tick < REGULAR_TIME_TICKS + OVERTIME_TICKS {
            MatchPhase::Overtime
        } else {
            MatchPhase::SuddenDeath
        };
    }

    /// Get a reference to the player for a given team.
    pub fn player(&self, team: Team) -> &PlayerState {
        match team {
            Team::Player1 => &self.player1,
            Team::Player2 => &self.player2,
        }
    }

    /// Get a mutable reference to the player for a given team.
    pub fn player_mut(&mut self, team: Team) -> &mut PlayerState {
        match team {
            Team::Player1 => &mut self.player1,
            Team::Player2 => &mut self.player2,
        }
    }

    /// Get a reference to the opponent for a given team.
    pub fn opponent(&self, team: Team) -> &PlayerState {
        match team {
            Team::Player1 => &self.player2,
            Team::Player2 => &self.player1,
        }
    }

    /// Check if the match should end. Updates `self.result`.
    pub fn check_match_end(&mut self) {
        // King tower destroyed → instant win
        if self.player1.is_eliminated() {
            self.result = MatchResult::Player2Win;
            return;
        }
        if self.player2.is_eliminated() {
            self.result = MatchResult::Player1Win;
            return;
        }

        // Time-based resolution
        if self.tick >= MAX_MATCH_TICKS {
            let c1 = self.player2.recount_opponent_crowns(); // crowns scored ON p2
            let c2 = self.player1.recount_opponent_crowns(); // crowns scored ON p1
            self.result = if c1 > c2 {
                MatchResult::Player1Win
            } else if c2 > c1 {
                MatchResult::Player2Win
            } else {
                // Compare king tower HP percentage
                let hp1_pct = self.player1.king.hp as f32 / self.player1.king.max_hp as f32;
                let hp2_pct = self.player2.king.hp as f32 / self.player2.king.max_hp as f32;
                if hp1_pct > hp2_pct {
                    MatchResult::Player1Win
                } else if hp2_pct > hp1_pct {
                    MatchResult::Player2Win
                } else {
                    MatchResult::Draw
                }
            };
            return;
        }

        // Overtime: enter if tied at end of regular time
        if self.tick == REGULAR_TIME_TICKS {
            let c1 = self.player2.recount_opponent_crowns();
            let c2 = self.player1.recount_opponent_crowns();
            if c1 != c2 {
                self.result = if c1 > c2 {
                    MatchResult::Player1Win
                } else {
                    MatchResult::Player2Win
                };
            }
            // else: tied → overtime continues
        }
    }

    /// Convenience: is the match still running?
    pub fn is_running(&self) -> bool {
        self.result == MatchResult::InProgress
    }
}