//! Clash Royale Simulator Engine
//! PyO3 entry point — exposes Rust functions to Python
//!
//! Python usage:
//! ```python
//! import cr_engine
//!
//! data = cr_engine.load_data("data/")
//! match = cr_engine.new_match(
//!     ["Knight", "Archers", "Fireball", "Giant", "Musketeer", "Valkyrie", "Hog Rider", "Minions"],
//!     ["Witch", "Skeleton Army", "Baby Dragon", "Prince", "Goblin Barrel", "Inferno Tower", "Zap", "Mega Knight"],
//! )
//! result = cr_engine.run_match(match, data)
//! print(result)  # {"winner": "player1", "crowns": [2, 1], "ticks": 3847, ...}
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;

// Phase 1-3 modules 
mod data_types;
mod entities;
mod game_state;
mod combat;
mod engine;
mod evo_system;
mod hero_system;
mod champion_system;

use data_types::GameData;
use entities::{Entity, EntityId, EntityKind, Team};
use game_state::{GameState, MatchResult};

// =========================================================================
// Internal helpers
// =========================================================================

/// Card type for deploy routing.
enum CardType {
    Character,
    Building,
    Spell,
    SpellProjectile,
}

// =========================================================================
// Card blacklist — event/champion/debug cards with broken or missing stats
// =========================================================================

/// Returns true if this card key is a supported standard card.
/// Excludes event variants, champions with zero damage, and debug entries.
fn is_supported_standard_card(key: &str) -> bool {
    !key.starts_with("super-")
        && !key.starts_with("party-")
        && key != "santa-hog-rider"
        && key != "terry"
        && key != "raging-prince"
}

// =========================================================================
// PyGameData — Python-visible wrapper around GameData
// =========================================================================

/// Wraps GameData in an Arc so Python can hold a reference and pass it
/// to multiple matches without cloning. GameData is immutable after load.
#[pyclass(name = "GameData")]
struct PyGameData {
    inner: Arc<GameData>,
}

#[pymethods]
impl PyGameData {
    /// Number of loaded character cards.
    #[getter]
    fn num_characters(&self) -> usize {
        self.inner.characters.len()
    }

    #[getter]
    fn num_buildings(&self) -> usize {
        self.inner.buildings.len()
    }

    #[getter]
    fn num_spells(&self) -> usize {
        self.inner.spells.len()
    }

    #[getter]
    fn num_projectiles(&self) -> usize {
        self.inner.projectiles.len()
    }

    #[getter]
    fn num_buffs(&self) -> usize {
        self.inner.buffs.len()
    }

    #[getter]
    fn num_evolutions(&self) -> usize {
        self.inner.evolutions.len()
    }

    #[getter]
    fn num_heroes(&self) -> usize {
        self.inner.heroes.len()
    }

    /// List all character card keys.
    fn character_keys(&self) -> Vec<String> {
        self.inner.characters.keys().cloned().collect()
    }

    /// List all building card keys.
    fn building_keys(&self) -> Vec<String> {
        self.inner.buildings.keys().cloned().collect()
    }

    /// List all spell names.
    fn spell_keys(&self) -> Vec<String> {
        self.inner.spells.keys().cloned().collect()
    }

    /// Check if a card key is a valid playable card (exists in cards.json).
    fn has_card(&self, key: &str) -> bool {
        self.inner.card_registry.contains_key(key)
    }

    /// Get basic stats for a character card as a dict.
    fn get_character_stats(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        let stats = self
            .inner
            .characters
            .get(key)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("Unknown character: {}", key)))?;

        let dict = PyDict::new_bound(py);
        dict.set_item("name", &stats.name)?;
        dict.set_item("key", &stats.key)?;
        dict.set_item("elixir", stats.elixir)?;
        dict.set_item("hitpoints", stats.hitpoints)?;
        dict.set_item("damage", stats.damage)?;
        dict.set_item("hit_speed", stats.hit_speed)?;
        dict.set_item("speed", stats.speed)?;
        dict.set_item("range", stats.range)?;
        dict.set_item("rarity", &stats.rarity)?;
        dict.set_item("attacks_air", stats.attacks_air)?;
        dict.set_item("attacks_ground", stats.attacks_ground)?;
        dict.set_item("is_flying", stats.is_flying())?;
        dict.set_item("is_ranged", stats.is_ranged())?;
        dict.set_item("is_splash", stats.is_splash())?;
        Ok(dict.into())
    }

    fn __repr__(&self) -> String {
        format!(
            "GameData({} characters, {} buildings, {} spells, {} projectiles, {} buffs, {} evos, {} heroes)",
            self.inner.characters.len(),
            self.inner.buildings.len(),
            self.inner.spells.len(),
            self.inner.projectiles.len(),
            self.inner.buffs.len(),
            self.inner.evolutions.len(),
            self.inner.heroes.len(),
        )
    }

    // === Phase 4: Agent-facing API ===

    /// List all playable cards from cards.json registry.
    /// Returns list of {"key": str, "elixir": int, "type": str}.
    /// Excludes blacklisted event/champion/debug cards.
    fn list_cards(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let mut cards = Vec::new();
        for (key, ci) in &self.inner.card_registry {
            if !is_supported_standard_card(key) {
                continue;
            }
            // Only include cards whose stats we can actually resolve
            let has_stats = self.inner.characters.contains_key(key)
                || self.inner.buildings.contains_key(key)
                || self.inner.spells.contains_key(key)
                || self.inner.spell_projectiles.contains_key(key);
            if !has_stats {
                continue;
            }
            let d = PyDict::new_bound(py);
            d.set_item("key", key)?;
            d.set_item("elixir", ci.elixir)?;
            d.set_item("type", &ci.card_type)?;
            d.set_item("has_evo", self.inner.evolutions.contains_key(key))?;
            d.set_item("has_hero", self.inner.heroes.contains_key(key))?;
            cards.push(d.into());
        }
        Ok(cards)
    }

    /// Get elixir cost for a card key. Returns -1 if not found.
    fn get_elixir_cost(&self, key: &str) -> i32 {
        if let Some(s) = self.inner.characters.get(key) {
            s.elixir
        } else if let Some(s) = self.inner.buildings.get(key) {
            s.elixir
        } else if self.inner.spells.contains_key(key) || self.inner.spell_projectiles.contains_key(key) {
            self.inner.card_registry.get(key).map(|ci| ci.elixir).unwrap_or(-1)
        } else {
            -1
        }
    }

    /// Check if a card has an evolution variant.
    fn has_evolution(&self, key: &str) -> bool {
        self.inner.evolutions.contains_key(key)
    }

    /// Check if a card has a hero variant.
    fn has_hero(&self, key: &str) -> bool {
        self.inner.heroes.contains_key(key)
    }

    /// Validate a deck (8 cards, all supported standard cards). Returns None if valid, else error string.
    fn validate_deck(&self, deck: Vec<String>) -> Option<String> {
        if deck.len() != 8 {
            return Some(format!("Deck must have 8 cards, got {}", deck.len()));
        }
        for key in &deck {
            if key == "mirror" {
                continue; // Mirror is a special meta-card
            }
            if !self.inner.card_registry.contains_key(key) {
                return Some(format!("'{}' is not a playable card (not in cards.json)", key));
            }
            if !is_supported_standard_card(key) {
                return Some(format!("'{}' is blacklisted (event/champion/debug card)", key));
            }
        }
        None
    }
}

// =========================================================================
// PyMatch — Python-visible wrapper around GameState
// =========================================================================

#[pyclass(name = "Match")]
struct PyMatch {
    state: GameState,
    /// Keep a reference to GameData so Python doesn't have to pass it every call.
    data: Arc<GameData>,
}

#[pymethods]
impl PyMatch {
    /// Current tick number.
    #[getter]
    fn tick(&self) -> i32 {
        self.state.tick
    }

    /// Is the match still running?
    #[getter]
    fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// Current match phase as string.
    #[getter]
    fn phase(&self) -> &str {
        match self.state.phase {
            game_state::MatchPhase::Regular => "regular",
            game_state::MatchPhase::DoubleElixir => "double_elixir",
            game_state::MatchPhase::Overtime => "overtime",
            game_state::MatchPhase::SuddenDeath => "sudden_death",
        }
    }

    /// Player 1 elixir (whole units).
    #[getter]
    fn p1_elixir(&self) -> i32 {
        self.state.player1.elixir_whole()
    }

    /// Player 2 elixir (whole units).
    #[getter]
    fn p2_elixir(&self) -> i32 {
        self.state.player2.elixir_whole()
    }

    /// Player 1 elixir in fixed-point (×10000). 10000 = 1 elixir.
    #[getter]
    fn p1_elixir_raw(&self) -> i32 {
        self.state.player1.elixir
    }

    /// Player 2 elixir in fixed-point (×10000).
    #[getter]
    fn p2_elixir_raw(&self) -> i32 {
        self.state.player2.elixir
    }

    /// Set a player's elixir to a specific whole-unit value (for testing).
    fn set_elixir(&mut self, player: i32, amount: i32) -> PyResult<()> {
        let ps = match player {
            1 => &mut self.state.player1,
            2 => &mut self.state.player2,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("player must be 1 or 2")),
        };
        ps.elixir = (amount * 10_000).min(crate::game_state::MAX_ELIXIR).max(0);
        Ok(())
    }

    /// Player 1 crowns scored.
    #[getter]
    fn p1_crowns(&self) -> i32 {
        self.state.player1.crowns
    }

    /// Player 2 crowns scored.
    #[getter]
    fn p2_crowns(&self) -> i32 {
        self.state.player2.crowns
    }

    /// Number of entities currently on the field.
    #[getter]
    fn num_entities(&self) -> usize {
        self.state.entities.len()
    }

    /// Player 1 tower HP: [king, princess_left, princess_right]
    fn p1_tower_hp(&self) -> Vec<i32> {
        vec![
            self.state.player1.king.hp,
            self.state.player1.princess_left.hp,
            self.state.player1.princess_right.hp,
        ]
    }

    /// Player 2 tower HP: [king, princess_left, princess_right]
    fn p2_tower_hp(&self) -> Vec<i32> {
        vec![
            self.state.player2.king.hp,
            self.state.player2.princess_left.hp,
            self.state.player2.princess_right.hp,
        ]
    }

    /// Player 1 hand (card keys).
    fn p1_hand(&self) -> Vec<String> {
        self.state
            .player1
            .hand
            .iter()
            .filter_map(|&idx| self.state.player1.deck.get(idx).cloned())
            .collect()
    }

    /// Player 2 hand (card keys).
    fn p2_hand(&self) -> Vec<String> {
        self.state
            .player2
            .hand
            .iter()
            .filter_map(|&idx| self.state.player2.deck.get(idx).cloned())
            .collect()
    }

    /// Advance the match by one tick. Returns True if match is still running.
    fn step(&mut self) -> bool {
        engine::tick(&mut self.state, &self.data);
        self.state.is_running()
    }

    /// Advance the match by `n` ticks. Returns True if match is still running.
    fn step_n(&mut self, n: i32) -> bool {
        for _ in 0..n {
            if !self.state.is_running() {
                break;
            }
            engine::tick(&mut self.state, &self.data);
        }
        self.state.is_running()
    }

    /// Run the match to completion. Returns result string.
    fn run_to_end(&mut self) -> String {
        let result = engine::run_match(&mut self.state, &self.data);
        match result {
            MatchResult::Player1Win => "player1".to_string(),
            MatchResult::Player2Win => "player2".to_string(),
            MatchResult::Draw => "draw".to_string(),
            MatchResult::InProgress => "in_progress".to_string(),
        }
    }

    /// Spawn a troop for a player at (x, y). Player: 1 or 2.
    /// Card must be a character key in GameData.
    /// Level defaults to 11 (tournament standard).
    /// is_evolved: if true, spawns as an evolved troop with evo_state + evo stat modifiers.
    /// Returns entity ID on success.
    #[pyo3(signature = (player, card_key, x, y, level=11, is_evolved=false))]
    fn spawn_troop(
        &mut self,
        player: i32,
        card_key: &str,
        x: i32,
        y: i32,
        level: usize,
        is_evolved: bool,
    ) -> PyResult<u32> {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "player must be 1 or 2",
                ))
            }
        };

        let stats = self
            .data
            .characters
            .get(card_key)
            .ok_or_else(|| {
                pyo3::exceptions::PyKeyError::new_err(format!("Unknown character: {}", card_key))
            })?;

        let id = self.state.alloc_id();
        let mut entity = Entity::new_troop(id, team, stats, x, y, level, is_evolved);

        // If stats.key was empty, override card_key with the lookup key
        // so hero_system and evo_system can find the right data.
        if entity.card_key.is_empty() {
            entity.card_key = card_key.to_string();
        }

        // Initialize hero state if this is a champion (has ability field)
        // and hero data exists for this key.
        let is_hero = hero_system::is_hero_card(&self.data, card_key)
            || hero_system::is_hero_card(&self.data, &entity.card_key);
        if is_hero {
            hero_system::setup_hero_state(&mut entity, card_key);
        }

        // Apply evo stat modifiers if evolved
        if is_evolved {
            evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
        }

        self.state.entities.push(entity);
        Ok(id.0)
    }

    /// Spawn a building for a player at (x, y).
    #[pyo3(signature = (player, card_key, x, y, level=11))]
    fn spawn_building(
        &mut self,
        player: i32,
        card_key: &str,
        x: i32,
        y: i32,
        level: usize,
    ) -> PyResult<u32> {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "player must be 1 or 2",
                ))
            }
        };

        let stats = self
            .data
            .buildings
            .get(card_key)
            .ok_or_else(|| {
                pyo3::exceptions::PyKeyError::new_err(format!("Unknown building: {}", card_key))
            })?;

        let id = self.state.alloc_id();
        let entity = Entity::new_building(id, team, stats, x, y, level);
        self.state.entities.push(entity);
        Ok(id.0)
    }

    /// Play a card from hand. Player: 1 or 2, hand_index: 0-3, (x, y) placement.
    /// Spends elixir and cycles the card. Returns entity ID or raises ValueError.
    #[pyo3(signature = (player, hand_index, x, y, level=11))]
    fn play_card(
        &mut self,
        player: i32,
        hand_index: usize,
        x: i32,
        y: i32,
        level: usize,
    ) -> PyResult<u32> {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "player must be 1 or 2",
                ))
            }
        };

        let ps = self.state.player_mut(team);

        // Validate hand index
        if hand_index >= ps.hand.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "hand_index {} out of range (hand size {})",
                hand_index,
                ps.hand.len()
            )));
        }

        let deck_idx = ps.hand[hand_index];
        let card_key = ps
            .deck
            .get(deck_idx)
            .cloned()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("Invalid deck index"))?;

        // ─── Mirror special handling ───
        // Mirror copies the last played card at +1 level for +1 elixir.
        // It's a meta-card that doesn't exist in spell/character data.
        if card_key == "mirror" {
            let last = self.state.player(team).last_played_card.clone();
            if let Some((last_key, last_cost, last_level)) = last {
                let mirror_cost = last_cost + 1;
                let mirror_level = last_level + 1;

                // Spend elixir
                let ps = self.state.player_mut(team);
                if !ps.spend_elixir(mirror_cost) {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "Not enough elixir for Mirror: need {}, have {}",
                        mirror_cost, ps.elixir_whole()
                    )));
                }
                ps.cycle_card(hand_index);
                // Mirror records itself as last played so you can't double-mirror
                ps.last_played_card = Some(("mirror".to_string(), mirror_cost, mirror_level));

                // Re-play the last card at +1 level using the same coordinate
                // Look up what type the last card was and spawn accordingly
                let id = self.state.alloc_id();
                if let Some(stats) = self.data.characters.get(&last_key) {
                    // Check if this is a burrow troop (Miner) — needs special deploy
                    if stats.spawn_pathfind_speed > 0 {
                        let king_pos = match team {
                            Team::Player1 => game_state::P1_KING_POS,
                            Team::Player2 => game_state::P2_KING_POS,
                        };
                        let mut entity = Entity::new_troop(
                            id, team, stats, king_pos.0, king_pos.1, mirror_level, false,
                        );
                        if entity.card_key.is_empty() {
                            entity.card_key = last_key.clone();
                        }
                        if let EntityKind::Troop(ref mut t) = entity.kind {
                            t.is_burrowing = true;
                            t.burrow_target_x = x;
                            t.burrow_target_y = y;
                            t.burrow_deploy_ticks = entity.deploy_timer;
                        }
                        entity.deploy_timer = i32::MAX;
                        self.state.entities.push(entity);
                        return Ok(id.0);
                    }

                    let mut entity = Entity::new_troop(
                        id, team, stats, x, y, mirror_level, false,
                    );
                    if entity.card_key.is_empty() {
                        entity.card_key = last_key.clone();
                    }
                    let is_hero = hero_system::is_hero_card(&self.data, &last_key);
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &last_key);
                    }
                    self.state.entities.push(entity);
                    return Ok(id.0);
                } else if let Some(stats) = self.data.buildings.get(&last_key) {
                    let entity = Entity::new_building(id, team, stats, x, y, mirror_level);
                    self.state.entities.push(entity);
                    return Ok(id.0);
                } else {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "Mirror: cannot re-play '{}' (not found in data)", last_key
                    )));
                }
            } else {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Mirror: no card played yet this match"
                ));
            }
        }

        // Look up card — try characters, buildings, zone spells, projectile spells
        let (elixir_cost, card_type) = if let Some(stats) = self.data.characters.get(&card_key) {
            (stats.elixir, CardType::Character)
        } else if let Some(stats) = self.data.buildings.get(&card_key) {
            (stats.elixir, CardType::Building)
        } else if let Some(_stats) = self.data.spells.get(&card_key) {
            // Look up real elixir cost from card registry
            let cost = self.data.card_registry.get(&card_key)
                .map(|ci| ci.elixir)
                .unwrap_or(3);
            (cost, CardType::Spell)
        } else if let Some(_stats) = self.data.spell_projectiles.get(&card_key) {
            let cost = self.data.card_registry.get(&card_key)
                .map(|ci| ci.elixir)
                .unwrap_or(4);
            (cost, CardType::SpellProjectile)
        } else {
            return Err(pyo3::exceptions::PyKeyError::new_err(format!(
                "Card key '{}' not found in characters, buildings, or spells",
                card_key
            )));
        };

        // ─── Deploy position clamping ───
        // Clamp the requested (x, y) into the valid deploy zone for this team.
        // Spells can target anywhere on the arena, so they skip this.
        // Troops with spawn_pathfind_speed > 0 (Miner) can also target anywhere —
        // they burrow underground to the target position.
        // Normal troops and buildings are clamped to the player's own side, with
        // extensions into opponent territory when their princess tower is destroyed.

        // Check if this is a burrow-capable troop (spawn_pathfind_speed > 0 in data)
        // or a building whose summon_character has spawn_pathfind_speed > 0 (Goblin Drill)
        let is_burrow_deploy = match card_type {
            CardType::Character => {
                self.data.characters.get(&card_key)
                    .map(|s| s.spawn_pathfind_speed > 0)
                    .unwrap_or(false)
            }
            CardType::Building => {
                // Check if building's summon_character has spawn_pathfind_speed > 0
                self.data.buildings.get(&card_key)
                    .and_then(|s| s.summon_character.as_ref())
                    .and_then(|sc| self.data.characters.get(sc.as_str()))
                    .map(|dig| dig.spawn_pathfind_speed > 0)
                    .unwrap_or(false)
            }
            _ => false,
        };

        let (x, y) = if matches!(card_type, CardType::Spell | CardType::SpellProjectile) || is_burrow_deploy {
            // Spells and burrow troops/buildings (Miner, Goblin Drill): clamp to arena bounds only
            (
                x.clamp(-game_state::ARENA_HALF_W, game_state::ARENA_HALF_W),
                y.clamp(-game_state::ARENA_HALF_H, game_state::ARENA_HALF_H),
            )
        } else {
            // Troops and buildings: clamp to valid deploy zone
            let mut cx = x.clamp(-game_state::ARENA_HALF_W, game_state::ARENA_HALF_W);
            let mut cy = y;

            // Base deploy zone: own side of the river.
            // Buildings have an additional setback from the river edge (1 tile = BUILDING_RIVER_SETBACK).
            // In real CR, buildings cannot be placed within 1 tile of the river.
            // Derived from TILE_SIZE constant — adapts to any arena size.
            let is_building_deploy = matches!(card_type, CardType::Building);
            let river_setback = if is_building_deploy { game_state::BUILDING_RIVER_SETBACK } else { 0 };

            let (own_y_min, own_y_max) = match team {
                Team::Player1 => (-game_state::ARENA_HALF_H, game_state::RIVER_Y_MIN - river_setback),
                Team::Player2 => (game_state::RIVER_Y_MAX + river_setback, game_state::ARENA_HALF_H),
            };

            // Extended zone: if an enemy princess tower is destroyed, the player
            // can deploy in that lane on the enemy side (up to the princess tower Y line).
            let opp = self.state.opponent(team);
            let opp_princess_y = match team {
                Team::Player1 => 10200i32,   // P2 princess tower Y
                Team::Player2 => -10200i32,  // P1 princess tower Y
            };
            let left_lane_open = !opp.princess_left.alive;
            let right_lane_open = !opp.princess_right.alive;

            // Check if the requested position is in an extended opponent-side zone
            let in_opponent_side = match team {
                Team::Player1 => cy > game_state::RIVER_Y_MIN,
                Team::Player2 => cy < game_state::RIVER_Y_MAX,
            };

            if in_opponent_side {
                // Only allow if the corresponding lane's princess tower is destroyed
                let lane_open = if cx < 0 { left_lane_open } else { right_lane_open };
                if lane_open {
                    // Clamp Y to between river edge and the opponent princess tower line
                    cy = match team {
                        Team::Player1 => cy.clamp(game_state::RIVER_Y_MAX, opp_princess_y),
                        Team::Player2 => cy.clamp(opp_princess_y, game_state::RIVER_Y_MIN),
                    };
                } else {
                    // Not allowed on opponent side — clamp back to own side
                    cy = cy.clamp(own_y_min, own_y_max);
                }
            } else {
                // Own side: just clamp within bounds
                cy = cy.clamp(own_y_min, own_y_max);
            }

            // Tower overlap check: push away from any tower center
            // Towers have effective radius ~1400 (king) or ~1100 (princess)
            let tower_positions: [(i32, i32, i32); 6] = [
                (0, -13000, 1600),      // P1 King
                (-5100, -10200, 1300),   // P1 Princess Left
                (5100, -10200, 1300),    // P1 Princess Right
                (0, 13000, 1600),        // P2 King
                (-5100, 10200, 1300),    // P2 Princess Left
                (5100, 10200, 1300),     // P2 Princess Right
            ];
            for &(tx, ty, min_dist) in &tower_positions {
                let dx = cx - tx;
                let dy = cy - ty;
                let dist_sq = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
                let min_sq = (min_dist as i64) * (min_dist as i64);
                if dist_sq < min_sq && dist_sq > 0 {
                    // Push outward from tower center to min_dist
                    let dist = (dist_sq as f64).sqrt();
                    let scale = min_dist as f64 / dist;
                    cx = tx + (dx as f64 * scale) as i32;
                    cy = ty + (dy as f64 * scale) as i32;
                }
            }

            // Building grid snapping: in real CR, buildings snap to a tile grid.
            // Tile size is TILE_SIZE (600 internal units). Snap both X and Y to the
            // nearest tile center. This is data-driven from the arena's TILE_SIZE
            // constant — adapts to any arena dimensions. Troops skip grid snapping.
            if is_building_deploy {
                let tile = game_state::TILE_SIZE;
                if tile > 0 {
                    // Snap to nearest tile center: round to nearest multiple of tile_size,
                    // then offset by half a tile to center within the tile.
                    let half = tile / 2;
                    cx = ((cx + half) / tile) * tile;
                    cy = ((cy + half) / tile) * tile;
                }
            }

            // Final arena bounds clamp
            cx = cx.clamp(-game_state::ARENA_HALF_W, game_state::ARENA_HALF_W);
            cy = cy.clamp(-game_state::ARENA_HALF_H, game_state::ARENA_HALF_H);

            // Re-apply river setback after grid snap (snap may have pushed into river zone)
            if is_building_deploy {
                cy = match team {
                    Team::Player1 => cy.min(own_y_max),
                    Team::Player2 => cy.max(own_y_min),
                };
            }

            (cx, cy)
        };

        // Spend elixir
        let ps = self.state.player_mut(team);
        if !ps.spend_elixir(elixir_cost) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Not enough elixir: need {}, have {}",
                elixir_cost,
                ps.elixir_whole()
            )));
        }

        // Cycle card in hand
        ps.cycle_card(hand_index);

        // Record last played card for Mirror
        ps.last_played_card = Some((card_key.clone(), elixir_cost, level));

        // Spawn entity
        let id = self.state.alloc_id();
        match card_type {
            CardType::Building => {
                let stats = self.data.buildings.get(&card_key).unwrap();

                // ─── Data-driven deploy: spell_as_deploy ───
                // When spell_as_deploy=true, the building deploys via a burrowing
                // summon character that travels underground to the target position
                // and morphs into the real building on arrival.
                // Data flow: spell_as_deploy gates the mechanic → summon_character
                // provides the dig troop → spawn_pathfind_speed is the travel speed
                // → spawn_pathfind_morph is the building key to morph into.
                // This replaces any hardcoded Goblin Drill detection.
                let dig_key = if stats.spell_as_deploy {
                    stats.summon_character.as_ref().and_then(|sc| {
                        self.data.characters.get(sc.as_str()).and_then(|dig_stats| {
                            if dig_stats.spawn_pathfind_speed > 0 {
                                Some((sc.clone(), dig_stats.clone()))
                            } else {
                                None
                            }
                        })
                    })
                } else {
                    None
                };

                if let Some((dig_char_key, dig_stats)) = dig_key {
                    // Spawn the dig character at king tower, burrowing to target
                    let king_pos = match team {
                        Team::Player1 => game_state::P1_KING_POS,
                        Team::Player2 => game_state::P2_KING_POS,
                    };
                    let mut entity = Entity::new_troop(
                        id, team, &dig_stats, king_pos.0, king_pos.1, level, false,
                    );
                    entity.card_key = card_key.clone(); // Keep original card key for morph lookup
                    if let EntityKind::Troop(ref mut t) = entity.kind {
                        t.is_burrowing = true;
                        t.burrow_target_x = x;
                        t.burrow_target_y = y;
                        t.burrow_deploy_ticks = entity.deploy_timer;
                    }
                    entity.deploy_timer = i32::MAX;
                    self.state.entities.push(entity);
                } else {
                    // Normal building deploy (no burrow)
                    let mut entity = Entity::new_building(id, team, stats, x, y, level);

            // Phase 3: Buildings can be evolved too (e.g., Furnace, Cannon, Tesla)
            let is_evo = self.data.evolutions.contains_key(&card_key);
            if is_evo {
                entity.evo_state = Some(entities::EvoState {
                    prev_x: x, prev_y: y, ..Default::default()
                });
                // Apply evo stat modifiers to building
                evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
            }

            self.state.entities.push(entity);
                }
            }
            CardType::Character => {
            let stats = self.data.characters.get(&card_key).unwrap();
            // Phase 3: Check if this card has an evolution or hero variant
            let is_evo = self.data.evolutions.contains_key(&card_key);
            let is_hero = hero_system::is_hero_card(&self.data, &card_key);

            // Phase 3: Multi-unit cards (Skeleton Army, Barbarians, Minion Horde, etc.)
            // These have summon_number > 0 and summon_character set
            let spawn_count = if stats.summon_number > 0 {
                let base_count = stats.summon_number;
                // Check evo spawn_count_override
                if is_evo {
                    if let Some(evo_def) = self.data.evolutions.get(&card_key) {
                        evo_def.stat_modifiers.spawn_count_override
                            .unwrap_or(base_count)
                    } else {
                        base_count
                    }
                } else {
                    base_count
                }
            } else {
                0
            };

            if spawn_count > 0 {
                // Multi-unit card: spawn N individual units
                let unit_key = stats.summon_character.as_ref()
                    .unwrap_or(&card_key);
                let unit_stats = self.data.characters.get(unit_key.as_str())
                    .unwrap_or(stats);
                let spread = unit_stats.collision_radius.max(200);

                // ── Royal Recruits: horizontal line deployment ──
                // In real CR, Royal Recruits deploy in a horizontal line spanning
                // the arena width, NOT clustered around the deploy point.
                // - Line centered at x=0, spread from far left to far right boundary
                // - Margin = unit collision radius (so they don't clip arena edge)
                // - Evenly spaced across the usable width
                // - All at the deploy Y coordinate (deploy X is ignored)
                // - Leftmost 3 → left lane, rightmost 3 → right lane (natural split)
                // - All spawn simultaneously (no stagger, no wave)
                // This is hardcoded in Supercell's engine; not in CSV data.
                let is_royal_recruits = card_key == "royal-recruits";

                if is_royal_recruits {
                    let margin = unit_stats.collision_radius.max(200);
                    let usable_width = (game_state::ARENA_HALF_W - margin) * 2;
                    let spacing = if spawn_count > 1 {
                        usable_width / (spawn_count as i32 - 1)
                    } else {
                        0
                    };
                    let start_x = -(game_state::ARENA_HALF_W - margin);

                    for i in 0..spawn_count {
                        let uid = if i == 0 { id } else { self.state.alloc_id() };
                        let recruit_x = start_x + (i as i32) * spacing;
                        let mut entity = Entity::new_troop(
                            uid, team, unit_stats, recruit_x, y, level, is_evo,
                        );
                        // No stagger — all 6 deploy simultaneously
                        if is_evo {
                            evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                        }
                        self.state.entities.push(entity);
                    }
                } else if unit_stats.spawn_angle_shift > 0 {
                // ── Circular cluster spawn (Bats, Night Witch spawned bats) ──
                // Units with spawn_angle_shift > 0 deploy in a circular pattern
                // around the deploy point, NOT a grid.
                // - Each unit offset by spawn_angle_shift degrees from the previous
                // - Radius = collision_radius (tight cluster, non-overlapping)
                // - All spawn simultaneously (no stagger)
                // - All go to the same lane (based on deploy position)
                // Data-driven: spawn_angle_shift is from character CSV data.
                let angle_step = unit_stats.spawn_angle_shift as f64 * std::f64::consts::PI / 180.0;
                let radius = unit_stats.collision_radius.max(200) as f64;

                for i in 0..spawn_count {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let angle = (i as f64) * angle_step;
                    let ox = (angle.cos() * radius) as i32;
                    let oy = (angle.sin() * radius) as i32;

                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y + oy, level, is_evo,
                    );
                    // No stagger — all spawn simultaneously
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else if card_key == "minion-horde" || card_key == "skeleton-army" || card_key == "goblin-gang" {
                // ── Filled circular cluster (Minion Horde, Skeleton Army, Goblin Gang) ──
                // Units distributed in concentric rings to fill a circle.
                // Small counts (≤8): single ring. Large counts (>8): center + inner + outer rings.
                // Radius scaled by spawn_radius_multiplier from override data.
                let mult = if stats.spawn_radius_multiplier > 0.0 { stats.spawn_radius_multiplier } else { 1.0 };
                let cr = unit_stats.collision_radius.max(200) as f64;

                // Build concentric ring layout: Vec of (offset_x, offset_y)
                let mut offsets: Vec<(i32, i32)> = Vec::new();
                let n = spawn_count as usize;

                if n <= 8 {
                    // Single ring (Minion Horde 6, Goblin Gang 5)
                    let radius = cr * mult as f64;
                    for i in 0..n {
                        let angle = (i as f64) * 2.0 * std::f64::consts::PI / n as f64;
                        offsets.push(((angle.cos() * radius) as i32, (angle.sin() * radius) as i32));
                    }
                } else {
                    // Multi-ring fill (Skeleton Army 15)
                    // Ring 0: 1 unit at center
                    // Ring 1: ~5 units at 1× collision_radius
                    // Ring 2: remainder at 2× collision_radius
                    offsets.push((0, 0));
                    let inner_count = ((n - 1) as f64 * 0.35).round() as usize; // ~35% on inner ring
                    let outer_count = n - 1 - inner_count;
                    let inner_r = cr * mult as f64 * 0.5;
                    let outer_r = cr * mult as f64;
                    for i in 0..inner_count {
                        let angle = (i as f64) * 2.0 * std::f64::consts::PI / inner_count as f64;
                        offsets.push(((angle.cos() * inner_r) as i32, (angle.sin() * inner_r) as i32));
                    }
                    for i in 0..outer_count {
                        let angle = (i as f64) * 2.0 * std::f64::consts::PI / outer_count as f64;
                        offsets.push(((angle.cos() * outer_r) as i32, (angle.sin() * outer_r) as i32));
                    }
                }

                for (i, &(ox, oy)) in offsets.iter().enumerate().take(n) {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y + oy, level, is_evo,
                    );
                    if i > 0 && unit_stats.deploy_delay > 0 {
                        let stagger = entities::ms_to_ticks(unit_stats.deploy_delay) * i as i32;
                        entity.deploy_timer += stagger;
                    }
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else if card_key == "barbarians" {
                // ── Semi-circle / arc (Barbarians) ──
                // 5 barbarians in an arc: wider than skeletons, tighter than horde.
                // Arc spans ~180° in front of deploy point.
                // Radius scaled by spawn_radius_multiplier from override data.
                let mult = if stats.spawn_radius_multiplier > 0.0 { stats.spawn_radius_multiplier } else { 1.0 };
                let radius = (unit_stats.collision_radius.max(200) as f64) * mult as f64;
                let arc_span = std::f64::consts::PI; // 180° arc
                let start_angle = -arc_span / 2.0;

                for i in 0..spawn_count {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let angle = if spawn_count > 1 {
                        start_angle + (i as f64) * arc_span / (spawn_count as f64 - 1.0)
                    } else {
                        0.0
                    };
                    let ox = (angle.cos() * radius) as i32;
                    let oy = (angle.sin() * radius) as i32;

                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y + oy, level, is_evo,
                    );
                    if i > 0 && unit_stats.deploy_delay > 0 {
                        let stagger = entities::ms_to_ticks(unit_stats.deploy_delay) * i as i32;
                        entity.deploy_timer += stagger;
                    }
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else if card_key == "skeletons" || card_key == "guards"
                    || card_key == "goblins" || card_key == "spear-goblins" || card_key == "minions" {
                // ── Triangle formation (3-unit cards) ──
                // Tight triangle: one in front, two behind.
                let cr = unit_stats.collision_radius.max(200);
                let offsets: [(i32, i32); 3] = [
                    (0, cr * team.forward_y()),          // front center
                    (-cr, 0),                            // back left
                    (cr, 0),                             // back right
                ];
                for i in 0..spawn_count.min(3) {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let (ox, oy) = offsets[i as usize];

                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y + oy, level, is_evo,
                    );
                    if i > 0 && unit_stats.deploy_delay > 0 {
                        let stagger = entities::ms_to_ticks(unit_stats.deploy_delay) * i as i32;
                        entity.deploy_timer += stagger;
                    }
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else if card_key == "elite-barbarians" || card_key == "archers"
                    || card_key == "wall-breakers" || card_key == "skeleton-dragons" {
                // ── Lateral pair (2-unit cards) ──
                // 2 units side by side, symmetric about deploy point.
                let cr = unit_stats.collision_radius.max(200);
                for i in 0..spawn_count.min(2) {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let ox = if i == 0 { -cr } else { cr };

                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y, level, is_evo,
                    );
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else if card_key == "royal-hogs" {
                // ── 2×2 grid (Royal Hogs) ──
                // 4 hogs in a square: 2 front, 2 back.
                let cr = unit_stats.collision_radius.max(200);
                let mult = if stats.spawn_radius_multiplier > 0.0 { stats.spawn_radius_multiplier } else { 1.0 };
                let half = (cr as f64 * mult as f64) as i32;
                let fwd = team.forward_y();
                // Positions: (-half, +fwd*half), (+half, +fwd*half), (-half, 0), (+half, 0)
                let grid: [(i32, i32); 4] = [
                    (-half, half * fwd),   // front left
                    ( half, half * fwd),   // front right
                    (-half, 0),            // back left
                    ( half, 0),            // back right
                ];
                for i in 0..spawn_count.min(4) {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let (ox, oy) = grid[i as usize];
                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y + oy, level, is_evo,
                    );
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else if card_key == "three-musketeers" {
                // ── Tight horizontal line (Three Musketeers) ──
                // 3 musketeers in a row: left, center, right.
                // Tighter than Royal Recruits — just collision_radius spacing.
                let cr = unit_stats.collision_radius.max(200);
                let mult = if stats.spawn_radius_multiplier > 0.0 { stats.spawn_radius_multiplier } else { 1.0 };
                let spacing = (cr as f64 * mult as f64 * 2.0) as i32; // 2× collision_radius between centers
                let start_x = -(spacing * (spawn_count as i32 - 1)) / 2;
                for i in 0..spawn_count {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    let ox = start_x + (i as i32) * spacing;
                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y, level, is_evo,
                    );
                    if i > 0 && unit_stats.deploy_delay > 0 {
                        let stagger = entities::ms_to_ticks(unit_stats.deploy_delay) * i as i32;
                        entity.deploy_timer += stagger;
                    }
                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }
                    self.state.entities.push(entity);
                }
                } else {
                // ── Default: grid pattern around deploy point ──
                for i in 0..spawn_count {
                    let uid = if i == 0 { id } else { self.state.alloc_id() };
                    // Spread units in a grid pattern around deploy point
                    let cols = 5.min(spawn_count);
                    let col = i % cols;
                    let row = i / cols;
                    let ox = (col as i32 - cols as i32 / 2) * spread;
                    let oy = (row as i32) * spread * team.forward_y();

                    let mut entity = Entity::new_troop(
                        uid, team, unit_stats, x + ox, y + oy, level, is_evo,
                    );

                    // deploy_delay stagger: in real CR, multi-unit cards deploy
                    // units with a stagger interval (deploy_delay field, typically
                    // 400ms = 8 ticks). Unit 0 deploys at normal deploy_time,
                    // unit 1 at deploy_time + deploy_delay, unit 2 at deploy_time
                    // + 2*deploy_delay, etc. This creates the wave-like spawn
                    // pattern visible when playing Skeleton Army, Barbarians, etc.
                    if i > 0 && unit_stats.deploy_delay > 0 {
                        let stagger = entities::ms_to_ticks(unit_stats.deploy_delay) * i as i32;
                        entity.deploy_timer += stagger;
                    }

                    if is_evo {
                        evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                    }
                    if is_hero {
                        hero_system::setup_hero_state(&mut entity, &card_key);
                    }

                    self.state.entities.push(entity);
                }
                } // end default grid

                // Also spawn secondary summon if present (e.g., some cards summon 2 types)
                if let Some(ref second_key) = stats.summon_character_second {
                    if stats.summon_character_second_count > 0 {
                        if let Some(second_stats) = self.data.characters.get(second_key.as_str()) {
                            for i in 0..stats.summon_character_second_count {
                                let uid = self.state.alloc_id();
                                let ox = ((i % 3) as i32 - 1) * spread;
                                let oy = ((i / 3) as i32 + 1) * spread * team.forward_y();
                                let entity = Entity::new_troop(
                                    uid, team, second_stats, x + ox, y + oy, level, false,
                                );
                                self.state.entities.push(entity);
                            }
                        }
                    }
                }
            } else {
                // Single-unit card (Knight, Musketeer, etc.)
                // Phase 3: Check if evo spawn_count_override applies to a non-multi-unit card
                // (e.g., Skeleton Barrel evo deploys 2 barrels instead of 1)
                let copy_count = if is_evo {
                    self.data.evolutions.get(&card_key)
                        .and_then(|e| e.stat_modifiers.spawn_count_override)
                        .unwrap_or(1)
                } else {
                    1
                };

                // ─── Burrow troop detection (Miner, GoblinDrillDig) ───
                // Data-driven: spawn_pathfind_speed > 0 means this troop burrows
                // underground to its target position before emerging.
                let burrow_speed = stats.spawn_pathfind_speed;

                let spread = stats.collision_radius.max(300);
                for c in 0..copy_count {
                    let uid = if c == 0 { id } else { self.state.alloc_id() };
                    let ox = if copy_count > 1 {
                        (c as i32 - copy_count as i32 / 2) * spread
                    } else {
                        0
                    };

                    if burrow_speed > 0 {
                        // ─── Burrow deploy (Miner) ───
                        // 1. Spawn entity at the king tower position (burrow origin)
                        // 2. Set target to the requested (x, y) — anywhere on the arena
                        // 3. Entity travels underground at spawn_pathfind_speed
                        // 4. On arrival, deploy_timer counts down (emerge animation)
                        let king_pos = match team {
                            Team::Player1 => game_state::P1_KING_POS,
                            Team::Player2 => game_state::P2_KING_POS,
                        };
                        let spawn_x = king_pos.0 + ox;
                        let spawn_y = king_pos.1;
                        let target_x = x + ox;
                        let target_y = y;

                        let mut entity = Entity::new_troop(
                            uid, team, stats, spawn_x, spawn_y, level, is_evo,
                        );

                        // Save the deploy timer and set up burrow state
                        if let EntityKind::Troop(ref mut t) = entity.kind {
                            t.is_burrowing = true;
                            t.burrow_target_x = target_x;
                            t.burrow_target_y = target_y;
                            // burrow_speed already set from spawn_pathfind_speed in new_troop
                            t.burrow_deploy_ticks = entity.deploy_timer;
                        }
                        // Keep deploy_timer > 0 during burrow so entity is untargetable.
                        // Set to i32::MAX so it won't expire during travel — tick_burrow
                        // will restore the real deploy timer when the troop arrives.
                        entity.deploy_timer = i32::MAX;

                        if is_evo {
                            evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                        }
                        if is_hero {
                            hero_system::setup_hero_state(&mut entity, &card_key);
                        }

                        self.state.entities.push(entity);
                    } else {
                        // Normal deploy (non-burrow troop)
                        let mut entity = Entity::new_troop(
                            uid, team, stats, x + ox, y, level, is_evo,
                        );

                        if is_evo {
                            evo_system::apply_evo_stat_modifiers(&mut entity, &self.data);
                        }
                        if is_hero {
                            hero_system::setup_hero_state(&mut entity, &card_key);
                        }

                        self.state.entities.push(entity);
                    }
                }
            }
            }
            CardType::Spell => {
                // Zone spell deployment: create a spell zone entity
                if let Some(spell_stats) = self.data.spells.get(&card_key) {
                    let radius = spell_stats.radius;
                    // Convert ms → ticks (20 ticks/sec), with per-level duration scaling.
                    // life_duration_increase_per_level: additional ms per level above 1.
                    // Graveyard, Goblin Drill, etc. last longer at higher levels.
                    let base_duration_ms = spell_stats.life_duration;
                    let level_bonus_ms = if spell_stats.life_duration_increase_per_level > 0 && level > 1 {
                        spell_stats.life_duration_increase_per_level * (level as i32 - 1)
                    } else {
                        0
                    };
                    let total_duration_ms = base_duration_ms + level_bonus_ms;
                    let duration_ticks = if total_duration_ms > 0 {
                        (total_duration_ms * 20 + 999) / 1000
                    } else {
                        1 // Instant spells last 1 tick
                    };
                    // Use level-scaled damage if available
                    let damage = if !spell_stats.damage_per_level.is_empty() && level > 0 {
                        let idx = (level - 1).min(spell_stats.damage_per_level.len() - 1);
                        spell_stats.damage_per_level[idx]
                    } else {
                        spell_stats.damage
                    };

                    // FIX 1+2: Compute level scaling ratio from damage_per_level.
                    // In real CR, ALL spell stats (damage, DOT, heal) scale by the same
                    // rarity-based multiplier per level. The JSON only provides damage_per_level,
                    // but heal_per_second and buff damage_per_second scale identically.
                    // Ratio: level_damage / base_damage (base = level 1 = damage_per_level[0]).
                    // Used below to scale heal_per_second and buff DOT at zone creation time.
                    let level_scale_num: i64 = if !spell_stats.damage_per_level.is_empty() && level > 0 {
                        let idx = (level - 1).min(spell_stats.damage_per_level.len() - 1);
                        spell_stats.damage_per_level[idx] as i64
                    } else {
                        1
                    };
                    let level_scale_den: i64 = if !spell_stats.damage_per_level.is_empty() {
                        spell_stats.damage_per_level[0].max(1) as i64
                    } else {
                        1
                    };

                    // FIX 2: Level-scale heal_per_second using the same ratio as damage.
                    // In real CR, Heal spell healing scales with card level identically
                    // to how damage scales. heal_per_second in JSON is the base (level 1) value.
                    let heal_per_second = if spell_stats.heal_per_second > 0 && level_scale_den > 0 {
                        (spell_stats.heal_per_second as i64 * level_scale_num / level_scale_den) as i32
                    } else {
                        spell_stats.heal_per_second
                    };

                    let hit_interval = if spell_stats.hit_speed > 0 {
                        (spell_stats.hit_speed * 20 + 999) / 1000
                    } else {
                        duration_ticks // Single hit for instant spells
                    };
                    // For targeting: use hits_air/hits_ground if aoe fields are unset.
                    // Lightning has hits_air=True, hits_ground=True but aoe_to_air/ground=false
                    // because it's not an AoE splash — it's targeted strikes. Use the hits_ fields.
                    let affects_air = spell_stats.aoe_to_air || spell_stats.hits_air;
                    let affects_ground = spell_stats.aoe_to_ground || spell_stats.hits_ground;
                    let only_enemies = spell_stats.only_enemies;
                    let only_own = spell_stats.only_own_troops;
                    let ct_pct = spell_stats.crown_tower_damage_percent;
                    let buff_key = spell_stats.buff.clone();
                    let buff_time = if spell_stats.buff_time > 0 {
                        (spell_stats.buff_time * 20 + 999) / 1000
                    } else {
                        duration_ticks // Buff lasts as long as the zone
                    };

                    // Compute displacement strength from buff data (Tornado).
                    // attract_percentage > 0 means this spell pulls entities toward center.
                    // Strength = attract_percentage * push_speed_factor / 100, scaled to
                    // units per tick. Empirically, Tornado (360%, speed 100) pulls at ~50-60
                    // units/tick, which drags a troop across its 5500-radius in ~1 second.
                    let attract_strength = buff_key.as_ref()
                        .and_then(|bk| self.data.buffs.get(bk))
                        .map(|bs| {
                            if bs.attract_percentage > 0 && bs.controlled_by_parent {
                                // Base pull: attract_percentage / 6 gives ~60 u/tick for Tornado (360/6=60)
                                // Modulate by push_speed_factor (100 = normal)
                                let base = bs.attract_percentage / 6;
                                if bs.push_speed_factor > 0 {
                                    (base as i64 * bs.push_speed_factor as i64 / 100) as i32
                                } else {
                                    base
                                }
                            } else {
                                0
                            }
                        })
                        .unwrap_or(0);

                    // FIX 3: Pre-compute no_effect_to_crown_towers from both spell and buff.
                    // Must be computed before buff_key is moved into new_spell_zone().
                    let no_effect_ct = spell_stats.no_effect_to_crown_towers
                        || buff_key.as_ref()
                            .and_then(|bk| self.data.buffs.get(bk))
                            .map(|bs| bs.no_effect_to_crown_towers)
                            .unwrap_or(false);

                    // Graveyard spawn fields from SpellStats
                    let spawn_char = spell_stats.spawn_character.clone();
                    let spawn_interval_ticks = if spell_stats.spawn_interval > 0 {
                        (spell_stats.spawn_interval * 20 + 999) / 1000
                    } else {
                        0
                    };
                    let spawn_initial_delay_ticks = if spell_stats.spawn_initial_delay > 0 {
                        (spell_stats.spawn_initial_delay * 20 + 999) / 1000
                    } else if spawn_interval_ticks > 0 {
                        spawn_interval_ticks // Default: first spawn after one interval
                    } else {
                        0
                    };

                    // FIX 6: Graveyard spawn_time — cap how long the spawner is active.
                    // spawn_time defines the total active spawn window (e.g., Graveyard 9100ms).
                    // If the zone's life_duration is longer, skeletons stop spawning after
                    // spawn_time even though the zone lingers. If spawn_time < life_duration,
                    // override the zone duration to spawn_time so the spawner stops on time.
                    // The zone itself may last longer (for lingering effects), but spawn_time
                    // controls when spawning stops.
                    let spawn_duration_override = if spell_stats.spawn_time > 0 && spawn_char.is_some() {
                        let spawn_time_ticks = (spell_stats.spawn_time * 20 + 999) / 1000;
                        // Use the shorter of zone duration and spawn_time for the zone lifetime.
                        // This ensures skeletons stop spawning at the right time.
                        if spawn_time_ticks > 0 && spawn_time_ticks < duration_ticks {
                            Some(spawn_time_ticks)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let duration_ticks = spawn_duration_override.unwrap_or(duration_ticks);

                    // Lightning: look up projectile damage for hit_biggest_targets spells.
                    // Lightning has hit_biggest_targets=True and projectile=LighningSpell.
                    // Each strike hits the highest-HP target with projectile damage, not zone damage.
                    let (hit_biggest, proj_damage, proj_ct_pct) = if spell_stats.hit_biggest_targets {
                        if let Some(ref proj_name) = spell_stats.projectile {
                            if let Some(proj) = self.data.projectiles.get(proj_name.as_str()) {
                                let pd = if !proj.damage_per_level.is_empty() && level > 0 {
                                    let idx = (level - 1).min(proj.damage_per_level.len() - 1);
                                    proj.damage_per_level[idx]
                                } else {
                                    proj.damage
                                };
                                (true, pd, proj.crown_tower_damage_percent)
                            } else {
                                (true, damage, ct_pct)
                            }
                        } else {
                            (true, damage, ct_pct)
                        }
                    } else {
                        (false, 0, 0)
                    };
                    // FIX 3: Use data-driven maximum_targets instead of hardcoded 3.
                    // Lightning has maximum_targets=3. Future spells may use different values.
                    let max_hit_targets = if hit_biggest {
                        if spell_stats.maximum_targets > 0 {
                            spell_stats.maximum_targets
                        } else {
                            3 // Fallback for Lightning if field is unset
                        }
                    } else {
                        0
                    };
                    // Override zone duration for hit_biggest spells: fire once then expire.
                    // duration = hit_interval means: tick 0 fires (hit_timer=0), then
                    // remaining decrements to 0 on the same tick as hit_timer would
                    // next reach 0 — so the zone dies before a second volley.
                    let duration_ticks = if hit_biggest {
                        hit_interval
                    } else {
                        duration_ticks
                    };

                    // FIX 1: Compute heal_per_hit from SpellStats.heal_per_second.
                    // The Heal spell has heal_per_second directly on SpellStats (not via buff).
                    // Convert to per-hit-interval: heal_per_hit = heal_per_second * hit_interval / 20.
                    let heal_per_hit = if heal_per_second > 0 && hit_interval > 0 {
                        (heal_per_second as i64 * hit_interval as i64 / 20).max(1) as i32
                    } else if heal_per_second > 0 {
                        // No hit_interval: per-tick heal
                        (heal_per_second / 20).max(1)
                    } else {
                        0
                    };

                    let entity = Entity::new_spell_zone(
                        id, team, &card_key,
                        x, y, radius, duration_ticks, damage, hit_interval,
                        affects_air, affects_ground,
                        buff_key, buff_time,
                        only_enemies, only_own, ct_pct,
                        attract_strength,
                        spawn_char, spawn_interval_ticks, spawn_initial_delay_ticks,
                        level,
                        hit_biggest, max_hit_targets, proj_damage, proj_ct_pct,
                        spell_stats.projectile.clone(),
                        spell_stats.spawn_min_radius, // Graveyard=3000: ring spawn
                        heal_per_hit,
                        spell_stats.pushback,     // zone pushback (Zap knockback)
                        spell_stats.pushback_all, // pushback all enemies in zone
                        spell_stats.min_pushback, // distance-scaled: min at edge
                        spell_stats.max_pushback, // distance-scaled: max at center
                        no_effect_ct,             // FIX 3: baked from spell + buff
                        spell_stats.affects_hidden, // FIX 4: baked from SpellStats
                        level_scale_num,          // FIX 1: for buff DOT scaling
                        level_scale_den,          // FIX 1: for buff DOT scaling
                    );
                    self.state.entities.push(entity);
                }
            }
            CardType::SpellProjectile => {
                // Projectile spell deployment (Rocket, Fireball, Arrows, Log, etc.)
                // These launch a projectile from the player's king tower toward (x,y).
                // On impact the projectile deals AoE damage via the existing splash system.
                if let Some(proj_stats) = self.data.spell_projectiles.get(&card_key) {
                    // Use level-scaled damage if available
                    let damage = if !proj_stats.damage_per_level.is_empty() && level > 0 {
                        let idx = (level - 1).min(proj_stats.damage_per_level.len() - 1);
                        proj_stats.damage_per_level[idx]
                    } else {
                        proj_stats.damage
                    };

                    // FIX: Use projectile_radius as fallback when radius is 0.
                    // Rolling projectiles (Log, Barb Barrel) store their AoE radius
                    // in projectile_radius, not radius.
                    let splash_radius = if proj_stats.radius > 0 {
                        proj_stats.radius
                    } else if proj_stats.projectile_radius > 0 {
                        proj_stats.projectile_radius
                    } else {
                        0
                    };
                    let ct_pct = proj_stats.crown_tower_damage_percent;

                    // AoE targeting: air/ground from projectile data
                    let aoe_air = proj_stats.aoe_to_air;
                    let aoe_ground = proj_stats.aoe_to_ground;

                    // Detect rolling spells (Log, Barb Barrel):
                    // These have projectile_radius > 0 but radius == 0.
                    let is_rolling = proj_stats.projectile_radius > 0 && proj_stats.radius == 0;

                    // FIX 10: Data-driven hit_biggest from ProjectileStats.
                    // If a projectile-type spell has hit_biggest=true in its ProjectileStats,
                    // it should behave like Lightning (hit N highest-HP targets within radius)
                    // rather than a standard traveling projectile. This handles the case where
                    // hit_biggest is on the projectile data but NOT on a SpellStats entry.
                    //
                    // Previously, hit_biggest was only read from SpellStats.hit_biggest_targets
                    // (the zone spell path). Projectile-type spells (resolved via
                    // spell_projectiles) never checked ProjectileStats.hit_biggest, causing
                    // any future projectile-type spell with this flag to silently behave
                    // as a normal AoE projectile instead of targeted-strike.
                    if proj_stats.hit_biggest {
                        // Create a spell zone with hit_biggest_targets=true, same as
                        // the Lightning path in CardType::Spell. The zone fires once,
                        // hitting the N highest-HP targets within radius.
                        let max_targets = if let Some(ci) = self.data.card_registry.get(&card_key) {
                            // Try to find maximum_targets from a matching SpellStats entry
                            self.data.spells.get(&card_key)
                                .or_else(|| self.data.spells.get(&ci.sc_key))
                                .map(|ss| ss.maximum_targets)
                                .unwrap_or(3) // Fallback to 3 (Lightning default)
                        } else {
                            3
                        };
                        let hit_interval = 1; // Single volley
                        let duration_ticks = hit_interval; // Zone dies after one hit cycle

                        // Look up target_buff from the projectile (Lightning → ZapFreeze stun)
                        let spell_projectile_key = Some(proj_stats.name.clone());

                        let zone = Entity::new_spell_zone(
                            id, team, &card_key,
                            x, y,
                            splash_radius,      // radius of target selection area
                            duration_ticks,
                            0,                  // zone damage = 0 (damage comes from projectile_damage)
                            hit_interval,
                            aoe_air, aoe_ground,
                            None,               // buff_key (applied via spell_projectile_key instead)
                            0,                  // buff_duration
                            true,               // only_enemies
                            false,              // only_own
                            ct_pct,
                            0,                  // attract_strength (no displacement)
                            None, 0, 0, level,  // no spawner
                            true,               // hit_biggest_targets = TRUE (data-driven)
                            max_targets,
                            damage,             // projectile_damage (per-strike)
                            ct_pct,             // projectile crown tower damage percent
                            spell_projectile_key,
                            0,                  // spawn_min_radius
                            0,                  // heal_per_hit
                            0, false,           // no pushback (Lightning-style)
                            0, 0,               // no distance-scaled pushback
                            false,              // no_effect_to_crown_towers
                            true,               // affects_hidden (Lightning hits hidden Tesla)
                            1, 1,               // level_scale: N/A (damage already level-scaled)
                        );
                        self.state.entities.push(zone);
                    } else if is_rolling {
                        // ── Rolling spells (Log, Barb Barrel) ──
                        // In real CR, the Log spawns at the deploy point and rolls
                        // forward in the team's direction, damaging enemies in its
                        // rectangular hitbox (projectile_radius × projectile_radius_y)
                        // as it passes through them. It travels projectile_range total
                        // distance at the given speed. Each enemy is hit only once.
                        let forward = team.forward_y();
                        let roll_range = if proj_stats.projectile_range > 0 {
                            proj_stats.projectile_range
                        } else {
                            10000 // Fallback
                        };
                        let roll_speed = entities::speed_to_units_per_tick(proj_stats.speed);
                        let roll_speed = roll_speed.max(30); // Minimum speed

                        // Target is the end point of the roll
                        let target_x = x;
                        let target_y = y + forward * roll_range;

                        let mut proj = Entity::new_projectile(
                            id,
                            team,
                            EntityId(0),
                            x, y,           // start at deploy point
                            EntityId(0),
                            target_x, target_y, // end of roll
                            roll_speed,
                            damage,
                            splash_radius,  // used as fallback if rolling_radius_x is 0
                            false,
                            ct_pct,
                            aoe_air,
                            aoe_ground,
                        );
                        // Set rolling-specific fields
                        if let EntityKind::Projectile(ref mut pd) = proj.kind {
                            pd.is_rolling = true;
                            pd.rolling_radius_x = proj_stats.projectile_radius;
                            pd.rolling_radius_y = if proj_stats.projectile_radius_y > 0 {
                                proj_stats.projectile_radius_y
                            } else {
                                proj_stats.projectile_radius
                            };
                            pd.rolling_range = roll_range;
                            pd.pushback = proj_stats.pushback;
                            pd.pushback_all = proj_stats.pushback_all;
                            pd.target_buff = proj_stats.target_buff.clone();
                            pd.target_buff_time = if proj_stats.buff_time > 0 {
                                entities::ms_to_ticks(proj_stats.buff_time)
                            } else { 0 };
                        }
                        proj.card_key = card_key.clone();
                        self.state.entities.push(proj);
                    } else {
                        // ── Standard spell projectiles (Fireball, Rocket, Arrows, Snowball) ──
                        // Launch from king tower toward (x,y).
                        //
                        // Speed conversion: spell projectile speed values (200-1100) are
                        // much higher than troop speeds (45-120). In CR, a Fireball
                        // (speed=600) crosses the arena in ~2s, Snowball (800) in ~1.5s.
                        // Arena half-height ≈ 15000 units, 2s = 40 ticks → need ~375 u/tick.
                        // Formula: speed * 6 / 10 matches observed CR travel times.
                        //
                        // FIX 6: Arcing projectiles (gravity > 0) use parabolic arc physics.
                        // In real CR, gravity-affected projectiles (Goblin Barrel, Royal
                        // Delivery) follow a parabolic trajectory. The travel time depends
                        // on distance and gravity, not just horizontal speed. We compute
                        // the arc travel time directly from the distance and then derive
                        // the effective speed = distance / travel_ticks. This gives correct
                        // timing for all distances (near deploy = fast, cross-arena = slow).
                        let from = match team {
                            Team::Player1 => game_state::P1_KING_POS,
                            Team::Player2 => game_state::P2_KING_POS,
                        };
                        let ddx = (from.0 - x) as f64;
                        let ddy = (from.1 - y) as f64;
                        let travel_dist = (ddx * ddx + ddy * ddy).sqrt();

                        let proj_speed = if proj_stats.gravity > 0 && travel_dist > 0.0 {
                            // Parabolic arc: t = sqrt(2 * distance / gravity_accel).
                            // gravity in data units (e.g., Goblin Barrel=40). Scale factor
                            // converts data gravity to internal-units/tick²: gravity * 0.3
                            // gives Goblin Barrel (g=40): 21000u → sqrt(2*21000/12) ≈ 59
                            // ticks = 2.95s; real CR ≈ 3.0s for cross-arena. ✓
                            let g_accel = (proj_stats.gravity as f64) * 0.3;
                            let arc_ticks = (2.0 * travel_dist / g_accel.max(1.0)).sqrt();
                            let arc_ticks = arc_ticks.max(1.0);
                            (travel_dist / arc_ticks) as i32
                        } else {
                            (proj_stats.speed * 6 / 10).max(60)
                        };

                        let mut proj = Entity::new_projectile(
                            id,
                            team,
                            EntityId(0),
                            from.0, from.1,
                            EntityId(0),
                            x, y,
                            proj_speed,
                            damage,
                            splash_radius,
                            false,
                            ct_pct,
                            aoe_air,
                            aoe_ground,
                        );
                        // Set pushback + target buff from projectile data
                        if let EntityKind::Projectile(ref mut pd) = proj.kind {
                            // ── Gravity arc ──
                            // Spell-card projectiles with gravity > 0 fly to a fixed landing
                            // point. They are already created non-homing (false above), but
                            // we also set is_gravity_arc so tick_projectiles skips any homing
                            // update for them. In real CR, Fireball/Rocket/Arrows land at the
                            // cast location — moving troops can dodge.
                            if proj_stats.gravity > 0 {
                                pd.is_gravity_arc = true;
                            }
                            pd.pushback = proj_stats.pushback;
                            pd.pushback_all = proj_stats.pushback_all;
                            // Wire distance-based pushback scaling from SpellStats.
                            // SpellStats has min_pushback/max_pushback (Fireball, Snowball).
                            // If not found in spells, also try the card's zone spell entry.
                            let spell_pb = self.data.spells.get(&card_key);
                            if let Some(ss) = spell_pb {
                                if ss.min_pushback > 0 && ss.max_pushback > 0 {
                                    pd.min_pushback = ss.min_pushback;
                                    pd.max_pushback = ss.max_pushback;
                                }
                            }
                            pd.target_buff = proj_stats.target_buff.clone();
                            pd.target_buff_time = if proj_stats.buff_time > 0 {
                                entities::ms_to_ticks(proj_stats.buff_time)
                            } else { 0 };
                        }
                        proj.card_key = card_key.clone();
                        self.state.entities.push(proj);
                    }

                    // Spawn troops on impact for Goblin Barrel, Royal Delivery, Barb Barrel
                    if let Some(ref spawn_key) = proj_stats.spawn_character {
                        let spawn_count = if proj_stats.spawn_character_count > 0 {
                            proj_stats.spawn_character_count
                        } else {
                            1
                        };
                        // Resolve spawn character via fuzzy lookup
                        if let Some(char_stats) = self.data.find_character(spawn_key) {
                            let spread = char_stats.collision_radius.max(200);

                            // Compute deploy timer for spawned troops.
                            // Use spawn_character_deploy_time from projectile data if
                            // available (e.g., Goblin Barrel 1100ms = 22 ticks).
                            // Otherwise estimate from projectile travel time.
                            let spawn_deploy_timer = if is_rolling {
                                // Rolling projectile (Barb Barrel, The Log).
                                // In real CR, the barbarian pops out of the barrel partway
                                // through the roll — NOT after the full roll completes.
                                // spawn_character_deploy_time is the delay from barrel launch
                                // until the barbarian emerges (BarbLogProjectileRolling=500ms).
                                //
                                // BUG FIX: Previously calculated travel_ticks + deploy_ticks
                                // (75 + 10 = 85 ticks for Barb Barrel), making the barbarian
                                // invisible and frozen for 4.25s. The barbarian was pre-spawned
                                // at the launch point with a massive deploy timer.
                                //
                                // Fix: Use only spawn_character_deploy_time as the deploy timer.
                                // The barbarian spawns at the play position and becomes active
                                // after the short deploy animation (500ms = 10 ticks). The barrel
                                // projectile continues rolling independently (handled by
                                // tick_projectiles). Data-driven from ProjectileStats.
                                let deploy_ticks = if proj_stats.spawn_character_deploy_time > 0 {
                                    entities::ms_to_ticks(proj_stats.spawn_character_deploy_time)
                                } else {
                                    10 // Fallback: 0.5s deploy if not specified
                                };
                                deploy_ticks.max(1)
                            } else if proj_stats.spawn_character_deploy_time > 0 {
                                // Data-driven: deploy time relative to barrel landing.
                                // Total = projectile_travel + spawn_deploy.
                                let from = match team {
                                    Team::Player1 => game_state::P1_KING_POS,
                                    Team::Player2 => game_state::P2_KING_POS,
                                };
                                // FIX 6: Use parabolic arc travel time for gravity projectiles.
                                let ddx = (from.0 - x) as f64;
                                let ddy = (from.1 - y) as f64;
                                let travel_dist_f = (ddx * ddx + ddy * ddy).sqrt();
                                let travel_ticks = if proj_stats.gravity > 0 && travel_dist_f > 0.0 {
                                    let g_accel = (proj_stats.gravity as f64) * 0.3;
                                    (2.0 * travel_dist_f / g_accel.max(1.0)).sqrt().max(1.0) as i32
                                } else {
                                    let proj_speed = (proj_stats.speed * 6 / 10).max(60);
                                    let travel_dist = travel_dist_f as i32;
                                    if proj_speed > 0 { travel_dist / proj_speed } else { 20 }
                                };
                                let deploy_ms = proj_stats.spawn_character_deploy_time;
                                let deploy_ticks = entities::ms_to_ticks(deploy_ms);
                                (travel_ticks + deploy_ticks).max(1)
                            } else {
                                // Estimate from travel time
                                let from = match team {
                                    Team::Player1 => game_state::P1_KING_POS,
                                    Team::Player2 => game_state::P2_KING_POS,
                                };
                                let proj_speed = (proj_stats.speed * 6 / 10).max(60);
                                let ddx = (from.0 - x) as i64;
                                let ddy = (from.1 - y) as i64;
                                let travel_dist = ((ddx*ddx + ddy*ddy) as f64).sqrt() as i32;
                                let estimated_travel = if proj_speed > 0 {
                                    travel_dist / proj_speed
                                } else {
                                    20
                                };
                                (estimated_travel + 1).max(1)
                            };

                            for i in 0..spawn_count {
                                let uid = self.state.alloc_id();
                                // Spawn in equilateral triangle pattern (like Goblin Barrel in real CR).
                                // Triangle radius ≈ projectile radius / 1.5.
                                // Rotation depends on player side: P1 points toward P2, P2 toward P1.
                                let tri_radius = (proj_stats.radius as f64 / 1.5) as i32;
                                let base_angle = if team == Team::Player1 {
                                    std::f64::consts::PI / 2.0  // point upward (toward P2)
                                } else {
                                    -std::f64::consts::PI / 2.0 // point downward (toward P1)
                                };
                                let angle = base_angle + (i as f64) * 2.0 * std::f64::consts::PI / (spawn_count as f64);
                                let ox = (tri_radius as f64 * angle.cos()) as i32;
                                let oy = (tri_radius as f64 * angle.sin()) as i32;
                                let mut troop = Entity::new_troop(
                                    uid, team, char_stats, x + ox, y + oy, level, false,
                                );
                                troop.deploy_timer = spawn_deploy_timer;
                                self.state.entities.push(troop);
                            }
                        }
                    }
                }
            }
        }

        Ok(id.0)
    }

    /// Get a snapshot of all entities as a list of dicts (for Python inspection/AI obs).
    fn get_entities(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let mut result = Vec::with_capacity(self.state.entities.len());
        for e in &self.state.entities {
            let dict = PyDict::new_bound(py);
            dict.set_item("id", e.id.0)?;
            dict.set_item("team", if e.team == Team::Player1 { 1 } else { 2 })?;
            dict.set_item("card_key", &e.card_key)?;
            dict.set_item("x", e.x)?;
            dict.set_item("y", e.y)?;
            dict.set_item("z", e.z)?;
            dict.set_item("hp", e.hp)?;
            dict.set_item("max_hp", e.max_hp)?;
            dict.set_item("shield_hp", e.shield_hp)?;
            dict.set_item("alive", e.alive)?;
            dict.set_item("damage", e.damage)?;
            dict.set_item(
                "kind",
                match &e.kind {
                    EntityKind::Troop(_) => "troop",
                    EntityKind::Building(_) => "building",
                    EntityKind::Projectile(_) => "projectile",
                    EntityKind::SpellZone(_) => "spell_zone",
                },
            )?;
            // Phase 3: Buff/evo/hero state
            dict.set_item("num_buffs", e.buffs.len())?;
            // Spell zone debug info
            if let EntityKind::SpellZone(ref sz) = e.kind {
                dict.set_item("sz_damage_per_tick", sz.damage_per_tick)?;
                dict.set_item("sz_affects_air", sz.affects_air)?;
                dict.set_item("sz_affects_ground", sz.affects_ground)?;
                dict.set_item("sz_radius", sz.radius)?;
                dict.set_item("sz_remaining", sz.remaining)?;
                dict.set_item("sz_hit_timer", sz.hit_timer)?;
                dict.set_item("sz_hit_interval", sz.hit_interval)?;
                dict.set_item("sz_only_enemies", sz.only_enemies)?;
                // Lightning debug fields
                dict.set_item("sz_hit_biggest", sz.hit_biggest_targets)?;
                dict.set_item("sz_max_hit_targets", sz.max_hit_targets)?;
                dict.set_item("sz_projectile_damage", sz.projectile_damage)?;
                dict.set_item("sz_projectile_ct_pct", sz.projectile_ct_pct)?;
                // Spawn fields (Graveyard)
                dict.set_item("sz_spawn_character", sz.spawn_character.as_deref().unwrap_or(""))?;
                dict.set_item("sz_spawn_interval", sz.spawn_interval)?;
                dict.set_item("sz_spawn_timer", sz.spawn_timer)?;
                dict.set_item("sz_spawn_initial_delay", sz.spawn_initial_delay)?;
            }
            dict.set_item("is_stunned", e.is_stunned())?;
            dict.set_item("is_frozen", e.is_frozen())?;
            dict.set_item("is_invisible", e.is_invisible())?;
            dict.set_item("speed_mult", e.speed_multiplier())?;
            dict.set_item("hitspeed_mult", e.hitspeed_multiplier())?;
            dict.set_item("damage_mult", e.damage_multiplier())?;
            dict.set_item("deploy_timer", e.deploy_timer)?;
            dict.set_item("is_evolved", e.evo_state.as_ref().map_or(false, |_| {
                matches!(&e.kind, EntityKind::Troop(t) if t.is_evolved)
            }))?;
            dict.set_item("is_hero", e.hero_state.as_ref().map_or(false, |h| h.is_hero))?;
            dict.set_item("hero_ability_active", e.hero_state.as_ref().map_or(false, |h| h.ability_active))?;
            // Attack animation state
            if let EntityKind::Troop(ref t) = e.kind {
                dict.set_item("is_burrowing", t.is_burrowing)?;
                if t.is_burrowing {
                    dict.set_item("burrow_target_x", t.burrow_target_x)?;
                    dict.set_item("burrow_target_y", t.burrow_target_y)?;
                }
                dict.set_item("attack_phase", match t.attack_phase {
                    entities::AttackPhase::Idle => "idle",
                    entities::AttackPhase::Windup => "windup",
                    entities::AttackPhase::Backswing => "backswing",
                    entities::AttackPhase::PostAttackStop => "post_attack_stop",
                })?;
                dict.set_item("phase_timer", t.phase_timer)?;
                dict.set_item("windup_ticks", t.windup_ticks)?;
                dict.set_item("backswing_ticks", t.backswing_ticks)?;
                dict.set_item("attack_cooldown", t.attack_cooldown)?;
                dict.set_item("hit_speed", t.hit_speed)?;
                dict.set_item("range_sq", t.range_sq)?;
                // Charge state (Ram Rider, Prince, etc.)
                if t.charge_range > 0 {
                    dict.set_item("charge_ready", t.charge_hit_ready)?;
                    dict.set_item("charge_damage", t.charge_damage)?;
                }
                // Dash state (Bandit, Mega Knight, Golden Knight)
                if t.dash_damage > 0 {
                    dict.set_item("is_dashing", t.is_dashing)?;
                    dict.set_item("dash_immune", t.dash_immune_remaining > 0)?;
                }
            }
            result.push(dict.into());
        }
        Ok(result)
    }

    /// Get full match result as a dict.
    fn get_result(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new_bound(py);
        dict.set_item(
            "winner",
            match self.state.result {
                MatchResult::Player1Win => "player1",
                MatchResult::Player2Win => "player2",
                MatchResult::Draw => "draw",
                MatchResult::InProgress => "in_progress",
            },
        )?;
        dict.set_item("ticks", self.state.tick)?;
        dict.set_item("seconds", self.state.tick as f64 / 20.0)?;
        dict.set_item("p1_crowns", self.state.player1.crowns.min(3))?;
        dict.set_item("p2_crowns", self.state.player2.crowns.min(3))?;
        dict.set_item("p1_king_hp", self.state.player1.king.hp)?;
        dict.set_item("p2_king_hp", self.state.player2.king.hp)?;
        dict.set_item(
            "p1_towers_alive",
            [
                self.state.player1.king.alive,
                self.state.player1.princess_left.alive,
                self.state.player1.princess_right.alive,
            ]
            .iter()
            .filter(|&&a| a)
            .count(),
        )?;
        dict.set_item(
            "p2_towers_alive",
            [
                self.state.player2.king.alive,
                self.state.player2.princess_left.alive,
                self.state.player2.princess_right.alive,
            ]
            .iter()
            .filter(|&&a| a)
            .count(),
        )?;
        Ok(dict.into())
    }

    fn __repr__(&self) -> String {
        format!(
            "Match(tick={}, phase={}, entities={}, p1_elixir={}, p2_elixir={}, running={})",
            self.state.tick,
            self.phase(),
            self.state.entities.len(),
            self.state.player1.elixir_whole(),
            self.state.player2.elixir_whole(),
            self.state.is_running(),
        )
    }

    fn activate_hero(&mut self, entity_id: u32) -> PyResult<()> {
    hero_system::activate_hero_ability(&mut self.state, &self.data, EntityId(entity_id))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    // === Phase 4: Agent-facing API ===

    /// Get a compact observation dict for an agent (one player's perspective).
    /// Includes: elixir, hand, tower HP, entity counts, phase info.
    fn get_observation(&self, py: Python<'_>, player: i32) -> PyResult<PyObject> {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("player must be 1 or 2")),
        };

        let ps = self.state.player(team);
        let opp = self.state.opponent(team);

        let dict = PyDict::new_bound(py);

        // Time
        dict.set_item("tick", self.state.tick)?;
        dict.set_item("phase", self.phase())?;
        dict.set_item("time_remaining", game_state::MAX_MATCH_TICKS - self.state.tick)?;

        // Elixir
        dict.set_item("my_elixir", ps.elixir_whole())?;

        // Hand — card keys
        let hand: Vec<String> = ps.hand.iter()
            .filter_map(|&idx| ps.deck.get(idx).cloned())
            .collect();
        dict.set_item("my_hand", hand)?;

        // Tower HP (own)
        dict.set_item("my_king_hp", ps.king.hp)?;
        dict.set_item("my_princess_left_hp", ps.princess_left.hp)?;
        dict.set_item("my_princess_right_hp", ps.princess_right.hp)?;
        dict.set_item("my_king_alive", ps.king.alive)?;
        dict.set_item("my_princess_left_alive", ps.princess_left.alive)?;
        dict.set_item("my_princess_right_alive", ps.princess_right.alive)?;

        // Tower HP (opponent)
        dict.set_item("opp_king_hp", opp.king.hp)?;
        dict.set_item("opp_princess_left_hp", opp.princess_left.hp)?;
        dict.set_item("opp_princess_right_hp", opp.princess_right.hp)?;
        dict.set_item("opp_king_alive", opp.king.alive)?;
        dict.set_item("opp_princess_left_alive", opp.princess_left.alive)?;
        dict.set_item("opp_princess_right_alive", opp.princess_right.alive)?;

        // Crowns
        dict.set_item("my_crowns", ps.crowns)?;
        dict.set_item("opp_crowns", opp.crowns)?;

        // Entity counts by team
        let my_troops = self.state.entities.iter()
            .filter(|e| e.alive && e.team == team && e.is_troop())
            .count();
        let opp_troops = self.state.entities.iter()
            .filter(|e| e.alive && e.team == team.opponent() && e.is_troop())
            .count();
        dict.set_item("my_troop_count", my_troops)?;
        dict.set_item("opp_troop_count", opp_troops)?;

        // Total entity count
        dict.set_item("total_entities", self.state.entities.iter().filter(|e| e.alive).count())?;

        Ok(dict.into())
    }

    /// Get valid placement bounds for a given player (base own-side zone).
    /// Returns (x_min, x_max, y_min, y_max) tuple — backward compatible.
    /// For extended zones (opponent side after tower destruction), use get_deploy_zones().
    fn get_deploy_bounds(&self, player: i32) -> PyResult<(i32, i32, i32, i32)> {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("player must be 1 or 2")),
        };

        let (base_y_min, base_y_max) = match team {
            Team::Player1 => (-game_state::ARENA_HALF_H, game_state::RIVER_Y_MIN),
            Team::Player2 => (game_state::RIVER_Y_MAX, game_state::ARENA_HALF_H),
        };

        // Extend Y range into opponent territory if either princess tower is destroyed.
        // The simple (x_min, x_max, y_min, y_max) tuple can't express per-lane
        // extensions, so we widen to the furthest valid Y if *any* lane is open.
        // The play_card clamping handles per-lane correctness; this just tells
        // the AI the widest possible range so it can attempt opponent-side deploys.
        let opp = self.state.opponent(team);
        let any_lane_open = !opp.princess_left.alive || !opp.princess_right.alive;

        let (y_min, y_max) = if any_lane_open {
            let opp_princess_y = match team {
                Team::Player1 => 10200i32,
                Team::Player2 => -10200i32,
            };
            match team {
                Team::Player1 => (base_y_min, opp_princess_y),
                Team::Player2 => (opp_princess_y, base_y_max),
            }
        } else {
            (base_y_min, base_y_max)
        };

        Ok((-game_state::ARENA_HALF_W, game_state::ARENA_HALF_W, y_min, y_max))
    }

    /// Get full deploy zone info including opponent-side extensions.
    /// Returns a dict with base bounds and optional lane extensions when
    /// enemy princess towers are destroyed.
    fn get_deploy_zones(&self, py: Python<'_>, player: i32) -> PyResult<PyObject> {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => return Err(pyo3::exceptions::PyValueError::new_err("player must be 1 or 2")),
        };

        let (y_min, y_max) = match team {
            Team::Player1 => (-game_state::ARENA_HALF_H, game_state::RIVER_Y_MIN),
            Team::Player2 => (game_state::RIVER_Y_MAX, game_state::ARENA_HALF_H),
        };

        let dict = PyDict::new_bound(py);
        dict.set_item("x_min", -game_state::ARENA_HALF_W)?;
        dict.set_item("x_max", game_state::ARENA_HALF_W)?;
        dict.set_item("y_min", y_min)?;
        dict.set_item("y_max", y_max)?;

        // Check if opponent princess towers are destroyed → extended deploy zones
        let opp = self.state.opponent(team);
        let opp_princess_y = match team {
            Team::Player1 => 10200i32,
            Team::Player2 => -10200i32,
        };

        if !opp.princess_left.alive {
            let (ext_y_min, ext_y_max) = match team {
                Team::Player1 => (game_state::RIVER_Y_MAX, opp_princess_y),
                Team::Player2 => (opp_princess_y, game_state::RIVER_Y_MIN),
            };
            dict.set_item("left_lane_ext_y_min", ext_y_min)?;
            dict.set_item("left_lane_ext_y_max", ext_y_max)?;
        }
        if !opp.princess_right.alive {
            let (ext_y_min, ext_y_max) = match team {
                Team::Player1 => (game_state::RIVER_Y_MAX, opp_princess_y),
                Team::Player2 => (opp_princess_y, game_state::RIVER_Y_MIN),
            };
            dict.set_item("right_lane_ext_y_min", ext_y_min)?;
            dict.set_item("right_lane_ext_y_max", ext_y_max)?;
        }

        Ok(dict.into())
    }

    /// Check if a card can be played (enough elixir, valid hand index).
    fn can_play_card(&self, player: i32, hand_index: usize) -> bool {
        let team = match player {
            1 => Team::Player1,
            2 => Team::Player2,
            _ => return false,
        };
        let ps = self.state.player(team);
        if hand_index >= ps.hand.len() {
            return false;
        }
        let deck_idx = ps.hand[hand_index];
        let card_key = match ps.deck.get(deck_idx) {
            Some(k) => k,
            None => return false,
        };
        let cost = if let Some(s) = self.data.characters.get(card_key) {
            s.elixir
        } else if let Some(s) = self.data.buildings.get(card_key) {
            s.elixir
        } else if let Some(_) = self.data.spells.get(card_key) {
            self.data.card_registry.get(card_key).map(|ci| ci.elixir).unwrap_or(99)
        } else if let Some(_) = self.data.spell_projectiles.get(card_key) {
            self.data.card_registry.get(card_key).map(|ci| ci.elixir).unwrap_or(99)
        } else {
            return false;
        };
        ps.elixir_whole() >= cost
    }

    /// Get list of playable hand indices (have enough elixir).
    fn playable_cards(&self, player: i32) -> Vec<usize> {
        (0..4).filter(|&i| self.can_play_card(player, i)).collect()
    }

    /// Check if a card can be deployed anywhere on the arena (Miner-like burrow).
    /// Data-driven: returns true iff the character has spawn_pathfind_speed > 0.
    /// Python AI agents should check this to know whether to use full-arena
    /// coordinates or own-side-only coordinates for placement.
    fn card_can_deploy_anywhere(&self, card_key: &str) -> bool {
        // Characters with spawn_pathfind_speed > 0 (Miner)
        if let Some(stats) = self.data.characters.get(card_key) {
            if stats.spawn_pathfind_speed > 0 {
                return true;
            }
        }
        // Buildings with can_deploy_on_enemy_side (Goblin Drill) or whose
        // summon_character has spawn_pathfind_speed > 0
        if let Some(stats) = self.data.buildings.get(card_key) {
            if stats.can_deploy_on_enemy_side {
                return true;
            }
            if let Some(ref sc) = stats.summon_character {
                if let Some(dig) = self.data.characters.get(sc.as_str()) {
                    if dig.spawn_pathfind_speed > 0 {
                        return true;
                    }
                }
            }
        }
        // Spells with can_deploy_on_enemy_side (Fireball, Poison, etc.)
        if let Some(stats) = self.data.spells.get(card_key) {
            if stats.can_deploy_on_enemy_side {
                return true;
            }
        }
        // Projectile-type spells (Rocket, Log, etc.) can always target anywhere
        if self.data.spell_projectiles.contains_key(card_key) {
            return true;
        }
        false
    }
}

// =========================================================================
// Module functions
// =========================================================================

/// Load game data from the data directory.
/// ```python
/// data = cr_engine.load_data("data/")
/// ```
#[pyfunction]
fn load_data(data_dir: &str) -> PyResult<PyGameData> {
    let gd = GameData::load(data_dir)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))?;
    Ok(PyGameData {
        inner: Arc::new(gd),
    })
}

/// Create a new match with two decks (lists of card keys).
/// ```python
/// m = cr_engine.new_match(data, ["Knight", "Archers", ...], ["Witch", "Prince", ...])
/// ```
#[pyfunction]
fn new_match(data: &PyGameData, deck1: Vec<String>, deck2: Vec<String>) -> PyResult<PyMatch> {
    // Validate deck sizes
    if deck1.len() != 8 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "deck1 must have 8 cards, got {}",
            deck1.len()
        )));
    }
    if deck2.len() != 8 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "deck2 must have 8 cards, got {}",
            deck2.len()
        )));
    }

    // Validate all card keys exist
    for key in deck1.iter().chain(deck2.iter()) {
        // Mirror is a special meta-card not in any data file
        if key == "mirror" {
            continue;
        }
        if !data.inner.characters.contains_key(key)
            && !data.inner.buildings.contains_key(key)
            && !data.inner.spells.contains_key(key)
            && !data.inner.spell_projectiles.contains_key(key)
        {
            return Err(pyo3::exceptions::PyKeyError::new_err(format!(
                "Unknown card: '{}'",
                key
            )));
        }
    }

    let state = GameState::new(deck1, deck2);
    Ok(PyMatch {
        state,
        data: Arc::clone(&data.inner),
    })
}

/// Run a batch of matches. Returns list of result dicts.
/// All matches use the same two decks (for statistical sampling).
/// ```python
/// results = cr_engine.run_batch(data, deck1, deck2, count=1000)
/// ```
#[pyfunction]
#[pyo3(signature = (data, deck1, deck2, count=100))]
fn run_batch(
    py: Python<'_>,
    data: &PyGameData,
    deck1: Vec<String>,
    deck2: Vec<String>,
    count: usize,
) -> PyResult<Vec<PyObject>> {
    let mut results = Vec::with_capacity(count);

    for _ in 0..count {
        let mut state = GameState::new(deck1.clone(), deck2.clone());
        engine::run_match(&mut state, &data.inner);

        let dict = PyDict::new_bound(py);
        dict.set_item(
            "winner",
            match state.result {
                MatchResult::Player1Win => "player1",
                MatchResult::Player2Win => "player2",
                MatchResult::Draw => "draw",
                MatchResult::InProgress => "in_progress",
            },
        )?;
        dict.set_item("ticks", state.tick)?;
        dict.set_item("p1_crowns", state.player1.crowns)?;
        dict.set_item("p2_crowns", state.player2.crowns)?;
        dict.set_item("p1_king_hp", state.player1.king.hp)?;
        dict.set_item("p2_king_hp", state.player2.king.hp)?;
        results.push(dict.into());
    }

    Ok(results)
}

// =========================================================================
// PyO3 module registration
// =========================================================================

#[pymodule]
fn cr_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGameData>()?;
    m.add_class::<PyMatch>()?;
    m.add_function(wrap_pyfunction!(load_data, m)?)?;
    m.add_function(wrap_pyfunction!(new_match, m)?)?;
    m.add_function(wrap_pyfunction!(run_batch, m)?)?;
    Ok(())
}