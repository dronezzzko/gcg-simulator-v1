"""Engine enumerations: locations, turn structure, decisions, and actions."""

from __future__ import annotations

from enum import IntEnum, StrEnum


class Zone(IntEnum):
    """Game locations (rule 4-1-1) plus engine-only holding places."""

    DECK = 0
    RESOURCE_DECK = 1
    RESOURCE_AREA = 2
    BATTLE = 3
    SHIELD = 4  # shield section of the shield area
    BASE = 5  # base section of the shield area
    REMOVAL = 6
    HAND = 7
    TRASH = 8
    RESOLVING = 9  # Commands / Burst cards whose effects are active (rule 4-1-2)
    OUTSIDE = 10  # tokens outside the game (rule 5-17-4)
    PAIRED = 11  # Pilot placed beneath a Unit in the battle area (rule 3-3-1)

    @property
    def is_public(self) -> bool:
        return self in PUBLIC_ZONES

    @property
    def is_field(self) -> bool:
        return self in FIELD_ZONES


PUBLIC_ZONES = frozenset(
    {
        Zone.RESOURCE_AREA,
        Zone.BATTLE,
        Zone.BASE,
        Zone.REMOVAL,
        Zone.TRASH,
        Zone.RESOLVING,
        Zone.PAIRED,
    }
)
FIELD_ZONES = frozenset({Zone.RESOURCE_AREA, Zone.BATTLE, Zone.SHIELD, Zone.BASE, Zone.PAIRED})
ORDERED_ZONES = frozenset({Zone.DECK, Zone.RESOURCE_DECK, Zone.SHIELD})


class Phase(IntEnum):
    SETUP = 0
    START = 1
    DRAW = 2
    RESOURCE = 3
    MAIN = 4
    END = 5
    GAME_OVER = 6


class Step(IntEnum):
    """Fine-grained procedure steps. Triggered effects resolve between steps (rules 7-1-3, 7-2-2, 7-6-2)."""

    # setup (rule 6-2)
    SETUP_CHOOSE_FIRST = 0
    SETUP_DRAW = 1
    SETUP_REDRAW_P1 = 2
    SETUP_REDRAW_P2 = 3
    SETUP_SHIELDS = 4
    # start phase (7-2)
    ACTIVE_STEP = 10
    START_STEP = 11
    # draw / resource
    DRAW_STEP = 20
    RESOURCE_STEP = 30
    # main phase (7-5)
    MAIN = 40
    # battle (8)
    ATTACK_TRIGGERS = 50
    ATTACK_END_CHECK = 51
    BLOCK = 52
    BLOCK_END_CHECK = 53
    BATTLE_ACTION = 54
    BATTLE_ACTION_END_CHECK = 55
    DAMAGE = 56
    DAMAGE_DONE = 57
    BATTLE_END = 58
    BATTLE_END_DONE = 59
    # end phase (7-6)
    END_ACTION = 60
    END_STEP = 61
    HAND_STEP = 62
    CLEANUP_STEP = 63
    TURN_END = 64
    GAME_OVER = 99


class DecisionKind(StrEnum):
    CHOOSE_FIRST = "choose_first"
    REDRAW = "redraw"
    MAIN = "main"
    BLOCK = "block"
    ACTION_STEP = "action_step"
    ORDER_TRIGGER = "order_trigger"
    BURST = "burst"
    YES_NO = "yes_no"
    SELECT = "select"
    EXCESS = "excess"
    DISCARD = "discard"
    ARRANGE = "arrange"
    PAYMENT = "payment"


class ActionKind(StrEnum):
    GO_FIRST = "go_first"  # a = player index who becomes Player One
    KEEP = "keep"
    REDRAW = "redraw"
    PLAY_UNIT = "play_unit"  # a = uid, c = EX Resources used
    PLAY_BASE = "play_base"  # a = uid, c = EX
    PAIR = "pair"  # a = pilot-capable card uid, b = unit uid, c = EX
    PLAY_COMMAND = "play_command"  # a = uid, c = EX
    ACTIVATE = "activate"  # a = host uid, b = ability key index, c = EX
    ATTACK = "attack"  # a = attacker uid, b = target uid or PLAYER_TARGET
    END_MAIN = "end_main"
    BLOCK = "block"  # a = blocker uid
    NO_BLOCK = "no_block"
    PASS = "pass"
    SELECT = "select"  # a = uid or option index
    DONE = "done"  # finish an optional/variable selection
    YES = "yes"
    NO = "no"
    ORDER = "order"  # a = index into pending trigger list


PLAYER_TARGET = -2
NO_ARG = -1


class Duration(StrEnum):
    THIS_TURN = "this_turn"
    THIS_BATTLE = "this_battle"
    OPPONENT_NEXT_TURN = "opponent_next_turn"  # through the end of the opponent's next turn
    YOUR_NEXT_TURN = "your_next_turn"
    WHILE_ON_FIELD = "while_on_field"


class EndReason(StrEnum):
    BATTLE_DAMAGE = "battle_damage"  # rule 1-2-2-1
    DECK_OUT = "deck_out"  # rule 1-2-2-2
    CONCEDE = "concede"  # rule 1-2-4
    BOTH_DEFEATED = "both_defeated"  # rule 11-2-1 simultaneous defeat → draw
    TURN_LIMIT = "turn_limit"  # engine safety cap (reported, never silent)
