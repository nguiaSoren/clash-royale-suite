"""
Agent interface for card placement decisions.

Agents:
    BaseAgent      — abstract interface
    DoNothingAgent — never plays (control baseline)
    RandomAgent    — plays random cards at random positions (baseline)
    RuleBasedAgent — simple heuristics (sanity check)

Usage:
    from ai_agent import RandomAgent, RuleBasedAgent
    agent = RandomAgent()
    action = agent.decide(match, player=1, game_data=data)
"""

import random
from abc import ABC, abstractmethod
from typing import Optional

try:
    import cr_engine
except ImportError:
    raise ImportError(
        "cr_engine not found. Build with: cd simulator && maturin develop --release"
    )


# =========================================================================
# Action — what an agent returns each tick
# =========================================================================

class Action:
    """Represents a single agent action per tick.

    Attributes:
        play: If True, deploy a card. If False, do nothing this tick.
        hand_index: Which card in hand to play (0-3).
        x: X coordinate for placement.
        y: Y coordinate for placement.
        activate_hero: If set, entity ID of a hero to activate ability.
    """

    def __init__(
        self,
        play: bool = False,
        hand_index: int = 0,
        x: int = 0,
        y: int = 0,
        activate_hero: Optional[int] = None,
    ):
        self.play = play
        self.hand_index = hand_index
        self.x = x
        self.y = y
        self.activate_hero = activate_hero

    @staticmethod
    def do_nothing() -> "Action":
        return Action(play=False)

    @staticmethod
    def deploy(hand_index: int, x: int, y: int) -> "Action":
        return Action(play=True, hand_index=hand_index, x=x, y=y)

    def __repr__(self) -> str:
        if self.activate_hero is not None:
            return f"Action(hero={self.activate_hero})"
        if self.play:
            return f"Action(play hand[{self.hand_index}] at ({self.x}, {self.y}))"
        return "Action(wait)"


# =========================================================================
# Base Agent
# =========================================================================

class BaseAgent(ABC):
    """Abstract base for all agents."""

    @abstractmethod
    def decide(
        self,
        match: "cr_engine.Match",
        player: int,
        game_data: "cr_engine.GameData",
    ) -> Action:
        """Choose an action for this tick.

        Args:
            match: The current match state (cr_engine.Match).
            player: Which player this agent controls (1 or 2).
            game_data: Immutable game data for card lookups.

        Returns:
            An Action (deploy a card or do nothing).
        """
        ...

    def on_match_start(self, match: "cr_engine.Match", player: int):
        """Called once at the start of a match. Override for setup."""
        pass

    def on_match_end(self, match: "cr_engine.Match", player: int, result: dict):
        """Called once at the end of a match. Override for learning."""
        pass


# =========================================================================
# DoNothingAgent
# =========================================================================

class DoNothingAgent(BaseAgent):
    """Never plays any cards. Useful as a control baseline."""

    def decide(self, match, player, game_data):
        return Action.do_nothing()


# =========================================================================
# RandomAgent  [FIX 1: elixir overflow prevention]
# =========================================================================

class RandomAgent(BaseAgent):
    """Plays a random affordable card at a random valid position.

    Rate control:
        - Normal: probability roll each tick (~1 card per 10s at default 0.005).
        - Overflow: if at 10 elixir AND cooldown has expired, force a play.
        - Cooldown: after every play, wait at least `min_play_gap` ticks before
          the next one. This prevents the "spam every tick at 9+ elixir" bug.

    Args:
        play_probability: Chance of playing a card each tick (0.0 to 1.0).
            Default 0.005 = ~1 card per 10 seconds at 20 tps.
        min_play_gap: Minimum ticks between plays. Default 40 (2 seconds).
    """

    def __init__(self, play_probability: float = 0.005, min_play_gap: int = 40):
        self.play_probability = play_probability
        self._min_play_gap = min_play_gap
        self._ticks_since_play = min_play_gap  # Start ready to play

    def decide(self, match, player, game_data):
        self._ticks_since_play += 1

        # Cooldown not expired — can't play regardless of elixir
        if self._ticks_since_play < self._min_play_gap:
            return Action.do_nothing()

        obs = match.get_observation(player)
        my_elixir = obs["my_elixir"]

        # Force a play ONLY at exactly 10 (capped, actively wasting).
        # Don't force at 9 — that caused the spam problem.
        force_play = my_elixir >= 10

        if not force_play and random.random() > self.play_probability:
            return Action.do_nothing()

        # Get playable cards (enough elixir)
        playable = match.playable_cards(player)
        if not playable:
            return Action.do_nothing()

        hand_idx = random.choice(playable)

        # Random position on own side
        x_min, x_max, y_min, y_max = match.get_deploy_bounds(player)
        x = random.randint(x_min, x_max)
        y = random.randint(y_min, y_max)

        self._ticks_since_play = 0  # Reset cooldown
        return Action.deploy(hand_idx, x, y)


# =========================================================================
# Placement helpers
# =========================================================================

# Arena geometry constants (matching game_state.rs)
_PRINCESS_LEFT_X = -5100
_PRINCESS_RIGHT_X = 5100
_BRIDGE_LEFT_X = -5100
_BRIDGE_RIGHT_X = 5100


def _defensive_position(player: int, threat_x: int) -> tuple:
    """Place a defender near the threatened princess tower.

    The idea: if enemy troops are pushing left lane, drop a defender next to
    your left princess tower so it engages them immediately instead of
    walking the full arena length from behind king.
    """
    # Pick the lane being threatened
    if threat_x <= 0:
        x = _PRINCESS_LEFT_X + random.randint(-800, 800)
    else:
        x = _PRINCESS_RIGHT_X + random.randint(-800, 800)

    # Just behind the princess tower Y (a few tiles back from princess)
    if player == 1:
        y = -10200 + random.randint(-1500, -500)   # behind P1 princess
    else:
        y = 10200 + random.randint(500, 1500)       # behind P2 princess

    return (x, y)


def _bridge_position(player: int, lane: Optional[str] = None) -> tuple:
    """Place a troop at the bridge for an aggressive push.

    lane: "left", "right", or None (random).
    """
    if lane is None:
        lane = random.choice(["left", "right"])

    x = _BRIDGE_LEFT_X if lane == "left" else _BRIDGE_RIGHT_X
    x += random.randint(-600, 600)

    if player == 1:
        y = -1200 + random.randint(-400, 0)   # just behind P1 bridge edge
    else:
        y = 1200 + random.randint(0, 400)      # just behind P2 bridge edge

    return (x, y)


def _back_position(player: int) -> tuple:
    """Place a troop behind king tower to build a slow push."""
    x = random.randint(-3000, 3000)
    if player == 1:
        y = random.randint(-14000, -12000)
    else:
        y = random.randint(12000, 14000)
    return (x, y)


def _detect_enemy_pressure(match, player: int) -> Optional[tuple]:
    """Scan entities for the nearest enemy troop on our side of the map.

    Returns (x, y) of the closest threat, or None if no enemies nearby.
    """
    entities = match.get_entities()
    my_side_y_max = -1200 if player == 1 else 15400
    my_side_y_min = -15400 if player == 1 else 1200

    closest = None
    closest_dist = float("inf")

    # Our king tower position for distance measurement
    king_y = -13000 if player == 1 else 13000

    for e in entities:
        if not e["alive"] or e["kind"] != "troop":
            continue
        # Enemy team
        if e["team"] == player:
            continue
        ey = e["y"]
        # Is this enemy on our half of the map?
        if my_side_y_min <= ey <= my_side_y_max:
            dist = abs(ey - king_y)
            if dist < closest_dist:
                closest_dist = dist
                closest = (e["x"], e["y"])

    return closest


# =========================================================================
# RuleBasedAgent  [FIX 1 + 2 + 3]
# =========================================================================

class RuleBasedAgent(BaseAgent):
    """Heuristic agent with reactive placement and card variety.

    Rate control:
        - Normal cadence: evaluates a play every ~60 ticks (3 seconds).
        - Overflow protection: at 10 elixir, bypasses the 60-tick cadence
          BUT still respects a minimum 40-tick cooldown after each play.
        - Post-play cooldown: after every play, wait at least 40 ticks (2s)
          before the next play. This prevents the "play every tick" spam
          that happens when elixir_urgent bypassed all gating.

    Args:
        aggression: 0.0 (very defensive) to 1.0 (very aggressive).
            Controls elixir threshold for attacking. Default 0.5.
        min_play_gap: Minimum ticks between plays. Default 40 (2 seconds).
    """

    def __init__(self, aggression: float = 0.5, min_play_gap: int = 40):
        self.aggression = max(0.0, min(1.0, aggression))
        self._min_elixir = max(4, int(8 - aggression * 4))
        self._tick_counter = 0
        self._min_play_gap = min_play_gap
        self._ticks_since_play = min_play_gap  # Start ready to play
        # FIX 3: Track last played hand index so we rotate card selection
        self._last_played_idx = -1

    def decide(self, match, player, game_data):
        self._tick_counter += 1
        self._ticks_since_play += 1

        # Hard cooldown: never play faster than min_play_gap, period.
        if self._ticks_since_play < self._min_play_gap:
            return Action.do_nothing()

        obs = match.get_observation(player)
        my_elixir = obs["my_elixir"]

        # At 10 elixir we bypass the 60-tick cadence (overflow protection).
        # At 9 we don't — the cooldown alone prevents waste at 9 because
        # the 40-tick gap means we'll play before capping most of the time.
        elixir_overflow = my_elixir >= 10

        if not elixir_overflow:
            # Normal pace: only consider playing every ~60 ticks (3s)
            if self._tick_counter % 60 != 0:
                return Action.do_nothing()
            # Don't play if below minimum elixir threshold
            if my_elixir < self._min_elixir:
                return Action.do_nothing()

        playable = match.playable_cards(player)
        if not playable:
            return Action.do_nothing()

        # Build card info for hand
        hand = obs["my_hand"]
        hand_costs = []
        for idx in playable:
            if idx < len(hand):
                cost = game_data.get_elixir_cost(hand[idx])
                if cost > 0:
                    hand_costs.append((idx, cost, hand[idx]))

        if not hand_costs:
            return Action.do_nothing()

        # -- FIX 2: Detect enemy pressure and react --
        # Original bug: always placed at fixed Y behind king. Now we scan
        # for enemy troops on our side and drop a defender near the threat.
        threat = _detect_enemy_pressure(match, player)

        if threat is not None:
            # DEFENSIVE mode - enemy troops on our side
            hand_idx, _cost, _key = self._pick_card(hand_costs, prefer="mid")
            x, y = _defensive_position(player, threat[0])
            self._ticks_since_play = 0
            return Action.deploy(hand_idx, x, y)

        # -- No pressure: offensive / buildup logic --

        # At max elixir - play most expensive at the bridge to avoid waste
        if my_elixir >= 10:
            hand_idx, _cost, _key = self._pick_card(hand_costs, prefer="expensive")
            x, y = _bridge_position(player)
            self._ticks_since_play = 0
            return Action.deploy(hand_idx, x, y)

        # High elixir (7+) - aggressive push at the bridge
        if my_elixir >= 7:
            hand_idx, _cost, _key = self._pick_card(hand_costs, prefer="expensive")
            x, y = _bridge_position(player)
            self._ticks_since_play = 0
            return Action.deploy(hand_idx, x, y)

        # Medium elixir - build a push from the back
        hand_idx, _cost, _key = self._pick_card(hand_costs, prefer="mid")
        x, y = _back_position(player)
        self._ticks_since_play = 0
        return Action.deploy(hand_idx, x, y)

    def _pick_card(
        self,
        hand_costs: list,
        prefer: str = "mid",
    ) -> tuple:
        """Pick a card with rotation to avoid always playing the same slot.

        FIX 3: The original always picked cheapest or most_expensive, which
        due to deck cycling caused hand[3] to be played endlessly. Now we:
          1. Sort by cost as before.
          2. Pick the preferred position (cheap/mid/expensive).
          3. BUT if it's the same hand_index we played last time, rotate to
             the next option. This breaks the degenerate cycle.

        prefer: "cheap", "mid", "expensive"
        """
        hand_costs_sorted = sorted(hand_costs, key=lambda x: x[1])

        if prefer == "cheap":
            candidates = hand_costs_sorted  # cheapest first
        elif prefer == "expensive":
            candidates = list(reversed(hand_costs_sorted))  # most expensive first
        else:  # "mid"
            # Rotate the list so mid-cost is first
            mid = len(hand_costs_sorted) // 2
            candidates = hand_costs_sorted[mid:] + hand_costs_sorted[:mid]

        # Pick the first candidate that isn't the same hand index we just played
        for card in candidates:
            if card[0] != self._last_played_idx:
                self._last_played_idx = card[0]
                return card

        # All candidates are the same index (only 1 card playable) - play it anyway
        choice = candidates[0]
        self._last_played_idx = choice[0]
        return choice