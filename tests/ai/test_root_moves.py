"""Root-move corrections behind search settings that are off in the baseline.

Each behaviour test also runs the baseline configuration (``BASELINE_PRESETS["standard"]``)
and shows it failing the same check; the guards show what the settings must keep.
"""

from __future__ import annotations

import dataclasses
from collections.abc import Callable, Iterator
from contextlib import contextmanager

import pytest
from tests.ai.test_information_set import (
    AGENT_SEEDS,
    GAME_SEEDS,
    PERMUTATION_SEEDS,
    STATES_PER_GAME,
    _hidden_ids,
    midgame_states,
)

from gcg_sim.ai import make_agent, noop
from gcg_sim.ai.actions import Key, canonical, merge_equivalent, prune_dominated
from gcg_sim.ai.baseline import BASELINE_PRESETS, BASELINE_WEIGHTS
from gcg_sim.ai.config import SearchConfig
from gcg_sim.effects import dsl as d
from gcg_sim.effects.registry import get_registry, override_script, restore_entry
from gcg_sim.engine import core
from gcg_sim.engine.game import apply
from gcg_sim.engine.observe import information_set_key, permute_hidden
from gcg_sim.engine.state import Action, GameState
from gcg_sim.engine.types import PLAYER_TARGET, ActionKind, DecisionKind, Zone
from gcg_sim.testkit import Scenario, activate, card_text, has_action, play, select_if_asked

A = ActionKind
FILLER = "GD01-060"
ZEON = ("ST03-008", "GD01-059", "GD01-035", "GD01-030", "GD01-031", "ST03-013", "GD01-115")
BASE = BASELINE_PRESETS["standard"]
SEEDS = tuple(range(31001, 31009))
NOOP_SEED = 31999


def config(**flags: bool) -> SearchConfig:
    return dataclasses.replace(BASE, **flags)


def root_moves(st: GameState, **flags: bool) -> list[Action]:
    dec = st.pending
    assert dec is not None
    return [a for _, a in prune_dominated(st, dec, canonical(st, dec), **flags)]


def flagged(st: GameState, **flags: bool) -> list[Action]:
    """Spending moves the no-effect check drops."""
    dec = st.pending
    assert dec is not None
    moves = prune_dominated(st, dec, canonical(st, dec))
    idle: set[Key] = noop.without_effect(st, dec.player, dec, moves, NOOP_SEED, **flags)
    return [a for key, a in moves if key in idle]


def play_turn(st: GameState, cfg: SearchConfig, seed: int) -> None:
    agents = tuple(
        make_agent("mcts", seed=seed + 100 * p, config=cfg, weights=BASELINE_WEIGHTS)
        for p in (0, 1)
    )
    stop = st.turn + 1
    while st.pending is not None and st.turn < stop:
        p = st.pending.player
        apply(st, agents[p].choose(st, p))


def hide_enemy_cards(sc: Scenario, hand: int) -> None:
    """Give player 1 ``hand`` cards hidden from player 0 and a varied deck, so that
    determinizations (and ``permute_hidden``) differ."""
    for i in range(hand):
        sc.add(1, ZEON[(2 * i) % len(ZEON)], Zone.HAND, known=False)
    sc.deck(1, *[ZEON[i % len(ZEON)] for i in range(28)])


# ---------------------------------------------------------------------------------------------
# noop_branch_choices: the no-effect check tries every option of the move's first choice


def rests_the_blocker(*, rested_decoy: bool, hidden: int = 0) -> GameState:
    """The lethal puzzle "rest the Blocker with Siege Ploy, then attack", optionally with a
    rested enemy Strike Dagger: the policy aims Siege Ploy at the more valuable Dagger, where
    resting does nothing."""
    sc = Scenario()
    sc.resources(0, 3)
    sc.add(0, "ST01-005")  # GM 2/2
    sc.hand(0, "ST02-014")  # Siege Ploy: rest 1 enemy Unit with 5 or less HP
    sc.add(1, "ST01-008")  # Demi Trainer <Blocker>
    if rested_decoy:
        sc.add(1, "ST04-005", rested=True)  # Strike Dagger 3/2
    hide_enemy_cards(sc, hidden)
    sc.shields(0, FILLER, FILLER, FILLER)
    return sc.start()


def holds_removal() -> GameState:
    """Resting the Blocker now is wasted (it stands up before the Gundam can attack)."""
    sc = Scenario()
    sc.resources(0, 4, rested=3)
    sc.add(0, "GD01-013", deployed_this_turn=True)
    sc.add(0, "ST02-014", Zone.HAND)
    sc.add(1, "ST01-008")
    sc.resources(1, 2)
    sc.shields(0, FILLER, FILLER, FILLER, FILLER)
    return sc.start()


def test_branching_keeps_a_command_whose_policy_target_is_already_rested() -> None:
    st = rests_the_blocker(rested_decoy=True)
    assert [a.kind for a in flagged(st)] == [A.PLAY_COMMAND]
    assert flagged(st, branch_choices=True) == []


def test_lethal_rests_the_blocker_while_a_bigger_unit_is_rested() -> None:
    def wins(cfg: SearchConfig) -> int:
        won = 0
        for seed in SEEDS:
            st = rests_the_blocker(rested_decoy=True)
            play_turn(st, cfg, seed)
            won += st.winner == 0
        return won

    assert wins(config(noop_branch_choices=True)) >= 7
    assert wins(BASE) <= 1


def test_branching_still_prunes_removal_that_should_wait() -> None:
    st = holds_removal()
    assert [a.kind for a in flagged(st, branch_choices=True)] == [A.PLAY_COMMAND]


def test_branching_changes_nothing_when_no_option_is_better() -> None:
    st = rests_the_blocker(rested_decoy=False)
    assert flagged(st) == flagged(st, branch_choices=True) == []


# ---------------------------------------------------------------------------------------------
# strict_attack_prune: attack triggers and battle tricks as the engine judges them


def zero_ap_scenario(*, active_resources: int, hidden: int = 0) -> tuple[GameState, int]:
    """CGS Mobile Worker (0 AP) next to Gundam Flauros, whose 【Attack】 trigger fires only for
    its own attacks, with Unforeseen Incident (Lv.3, cost 1) in hand."""
    sc = Scenario()
    sc.resources(0, active_resources)
    worker = sc.add(0, "ST05-003")
    sc.add(0, "GD05-060")
    sc.hand(0, "ST01-014")
    sc.add(1, "ST04-005")
    hide_enemy_cards(sc, hidden)
    sc.shields(0, FILLER, FILLER, FILLER)
    sc.shields(1, FILLER, FILLER, FILLER)
    return sc.start(), worker


def attackers(st: GameState, *, strict: bool) -> set[int]:
    return {a.a for a in root_moves(st, strict_attacks=strict) if a.kind is A.ATTACK}


def test_strict_prune_drops_a_0ap_attack_another_units_trigger_cannot_help() -> None:
    st, worker = zero_ap_scenario(active_resources=2)  # Lv.2: the trick is not castable
    assert worker in attackers(st, strict=False)
    assert worker not in attackers(st, strict=True)


def test_strict_prune_keeps_a_0ap_attack_with_a_castable_trick() -> None:
    st, worker = zero_ap_scenario(active_resources=3)
    assert worker in attackers(st, strict=True)


def test_strict_prune_keeps_a_0ap_attack_that_triggers_another_units_effect() -> None:
    """The-O: "When one of your other Units with <Repair> attacks, ... Rest it." (paired)."""

    def scenario() -> tuple[GameState, int]:
        sc = Scenario()
        worker = sc.add(0, "ST05-003")
        sc.add(0, "GD03-002", pilot="ST01-010")
        sc.add(1, "ST04-005", rested=True)
        sc.shields(1, FILLER, FILLER, FILLER)
        return sc.start(), worker

    with card_text("ST05-003", "<Repair 1>"):
        st, worker = scenario()
        assert worker in attackers(st, strict=True)
    st, worker = scenario()
    assert worker not in attackers(st, strict=True)


@pytest.mark.parametrize(
    ("unit", "pilot"),
    [("EB01-013", None), ("ST05-003", "GD05-096")],
    ids=["own-attack-boost", "pilot-attack-trigger"],
)
def test_strict_prune_keeps_a_0ap_attack_with_its_own_attack_trigger(
    unit: str, pilot: str | None
) -> None:
    """Red Gundam (0085) (0 AP, 【Attack】 AP+2) and a Mobile Worker paired with Chad Chadan
    (0 AP, 【Attack】 deal 1 damage to this Unit ...)."""
    sc = Scenario()
    attacker = sc.add(0, unit, pilot=pilot)
    sc.shields(1, FILLER, FILLER, FILLER)
    st = sc.start()
    assert attacker in attackers(st, strict=True)


# ---------------------------------------------------------------------------------------------
# prune_noop_rest_targets: an effect that only rests does not choose a rested enemy Unit


def amuro_rests(
    *, extra_units: tuple[str, ...] = (), hidden: int = 0, reactor: str | None = None
) -> tuple[GameState, int, int]:
    """Amuro Ray pairs: 【When Paired】rest 1 enemy Unit with 5 or less HP. Schuzrum-Galluss is
    already rested; resting the active Zaku II is weakly dominant."""
    sc = Scenario()
    sc.resources(0, 4)
    host = sc.add(0, "GD01-013")
    for number in extra_units:
        sc.add(0, number)
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    rested = sc.add(1, "GD04-040", rested=True)
    zaku = sc.add(1, "GD01-035")
    if reactor is not None:
        sc.base(1, reactor)
    sc.resources(1, 4)
    hide_enemy_cards(sc, hidden)
    sc.shields(0, *[FILLER] * 5)
    sc.shields(1, *[ZEON[(3 * i) % len(ZEON)] for i in range(5)])
    st = sc.start()
    play(st, amuro, onto=host)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    return st, rested, zaku


@pytest.mark.parametrize(
    ("extra_units", "hidden"), [((), 0), (("ST01-005",), 4)], ids=["plain", "hidden-hand"]
)
def test_rest_effect_rests_the_active_unit(extra_units: tuple[str, ...], hidden: int) -> None:
    st, rested, zaku = amuro_rests(extra_units=extra_units, hidden=hidden)
    assert Action(A.SELECT, rested) in root_moves(st)
    assert root_moves(st, rest_targets=True) == [Action(A.SELECT, zaku)]

    def correct(cfg: SearchConfig) -> int:
        good = 0
        for seed in SEEDS:
            agent = make_agent("mcts", seed=seed, config=cfg, weights=BASELINE_WEIGHTS)
            good += agent.choose(st.clone(), 0) == Action(A.SELECT, zaku)
        return good

    assert correct(config(prune_noop_rest_targets=True)) == len(SEEDS)
    assert correct(BASE) < len(SEEDS)


def test_rest_targets_are_kept_when_a_card_in_play_reacts_to_resting() -> None:
    """Doritea: "when one of your Units is rested by one of your opponent's effects, ...deal
    1 damage"; the no-op target is then the better one."""
    st, rested, _ = amuro_rests(reactor="GD03-128")
    assert Action(A.SELECT, rested) in root_moves(st, rest_targets=True)


def test_rest_targets_are_kept_when_resting_switches_an_effect_on() -> None:
    """Freedom Gundam: "While this Unit is rested, friendly Shields can't receive battle damage
    from enemy Units". Resting the active Freedom Gundam would stop the lethal attacks that
    resting the already-rested Unit leaves open, so the rested target is not a no-op."""
    sc = Scenario()
    sc.resources(0, 4)
    zaku = sc.add(0, "GD01-035")
    sc.add(0, "GD01-013")
    amuro = sc.add(0, "ST01-010", Zone.HAND)
    rested = sc.add(1, "GD04-040", rested=True)
    freedom = sc.add(1, "GD03-070")
    sc.shields(0, *[FILLER] * 5)
    sc.shields(1, FILLER)
    st = sc.start()
    play(st, amuro, onto=zaku)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert root_moves(st, rest_targets=True) == [
        Action(A.SELECT, rested),
        Action(A.SELECT, freedom),
    ]


def test_rest_targets_are_kept_when_the_effect_uses_the_card_again() -> None:
    """G-Falcon DX: "Choose 1 enemy Unit ... Rest it. Then, ... it gets AP-2": the chosen Unit
    is used after the Rest, so a rested one is not a no-op."""
    sc = Scenario()
    sc.resources(0, 7)
    falcon = sc.add(0, "ST14-005", Zone.HAND)
    rested = sc.add(1, "GD04-040", rested=True)
    sc.add(1, "GD01-035")
    st = sc.start()
    play(st, falcon)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert Action(A.SELECT, rested) in root_moves(st, rest_targets=True)


def test_rest_targets_are_kept_when_if_you_do_follows() -> None:
    """ "Rest it. If you do, draw 1": choosing a rested Unit skips the draw. The same Command
    without the "If you do" part is pruned."""
    enemy_unit = d.Sel(d.Side.ENEMY, d.Loc.BATTLE)
    choose_and_rest = (d.Choose("t1", enemy_unit), d.Rest(d.Var("t1")))
    for steps, kept in (
        (choose_and_rest, False),
        ((*choose_and_rest, d.IfYouDo((d.Draw(),))), True),
    ):
        with command_script("ST01-014", *steps):
            sc = Scenario()
            sc.resources(0, 3)
            card = sc.add(0, "ST01-014", Zone.HAND)
            rested = sc.add(1, "GD04-040", rested=True)
            sc.add(1, "GD01-035")
            sc.deck(0, *[FILLER] * 5)
            st = sc.start()
            play(st, card)
            assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
            assert (Action(A.SELECT, rested) in root_moves(st, rest_targets=True)) is kept


def test_rest_targets_are_kept_while_a_once_per_turn_effect_is_unused() -> None:
    """Ribo Colony: "【Once per Turn】When you pair a Pilot that is Lv.3 or lower ..., choose 1
    enemy Unit with 3 or less HP. Rest it." Resting nothing leaves the effect unused this
    turn, so the rested target is not a no-op."""
    sc = Scenario()
    sc.resources(0, 4)
    sc.base(0, "GD03-124")
    host = sc.add(0, "GD01-013")
    pilot = sc.add(0, "GD01-089", Zone.HAND)
    rested = sc.add(1, "GD04-040", rested=True)
    sc.add(1, "GD01-035")
    st = sc.start()
    play(st, pilot, onto=host)
    assert st.pending is not None and st.pending.kind is DecisionKind.SELECT
    assert Action(A.SELECT, rested) in root_moves(st, rest_targets=True)


# ---------------------------------------------------------------------------------------------
# noop_resource_aware: the no-effect check sees resource-area changes other than payment


def age_device(*, hidden: int = 0) -> GameState:
    sc = Scenario()
    sc.resources(0, 4)
    sc.add(0, "GD02-029")  # Gundam AGE-1 Normal (AGE System)
    sc.hand(0, "GD02-103")  # AGE Device: place 1 EX Resource
    hide_enemy_cards(sc, hidden)
    sc.shields(0, FILLER, FILLER, FILLER)
    sc.shields(1, FILLER, FILLER, FILLER)
    return sc.start()


def test_resource_aware_check_keeps_placing_an_ex_resource() -> None:
    st = age_device()
    assert [a.kind for a in flagged(st)] == [A.PLAY_COMMAND]
    assert flagged(st, resource_aware=True) == []


@contextmanager
def command_script(number: str, *steps: d.Step) -> Iterator[None]:
    reg = get_registry()
    script = d.CardScript(number, abilities=(d.Command(d.Timing.MAIN, steps),))
    previous = override_script(reg, reg.db[number], script)
    core.clear_listen_cache()
    try:
        yield
    finally:
        restore_entry(reg, previous)
        core.clear_listen_cache()


def test_resource_aware_check_keeps_removing_an_enemy_ex_resource() -> None:
    """A removed EX Resource ceases to exist, so the baseline signature cannot see it."""
    enemy_ex = d.Sel(d.Side.ENEMY, d.Loc.RESOURCE_AREA, (d.IsKind((d.CardKind.EX_RESOURCE,)),))
    with command_script("ST01-014", d.Choose("t1", enemy_ex), d.Exile(d.Var("t1"))):
        sc = Scenario()
        sc.resources(0, 3)
        sc.hand(0, "ST01-014")
        sc.resources(1, 2, ex=1)
        sc.shields(0, FILLER, FILLER, FILLER)
        sc.shields(1, FILLER, FILLER, FILLER)
        st = sc.start()
        assert [a.kind for a in flagged(st)] == [A.PLAY_COMMAND]
        assert flagged(st, resource_aware=True) == []


def test_resource_aware_check_still_prunes_removal_that_should_wait() -> None:
    st = holds_removal()
    assert [a.kind for a in flagged(st, resource_aware=True)] == [A.PLAY_COMMAND]


# ---------------------------------------------------------------------------------------------
# merge_equivalent_moves: interchangeable field cards give one root move


def merged(st: GameState) -> list[Action]:
    dec = st.pending
    assert dec is not None
    return [a for _, a in merge_equivalent(st, dec, canonical(st, dec))]


def attacks(moves: list[Action]) -> list[tuple[int, int]]:
    return [(a.a, a.b) for a in moves if a.kind is A.ATTACK]


def identical_attackers(*, hidden: int = 0) -> tuple[GameState, int, int, int, int]:
    """Two GMs facing two rested Strike Daggers."""
    sc = Scenario()
    gm1 = sc.add(0, "ST01-005")
    gm2 = sc.add(0, "ST01-005")
    dagger1 = sc.add(1, "ST04-005", rested=True)
    dagger2 = sc.add(1, "ST04-005", rested=True)
    hide_enemy_cards(sc, hidden)
    sc.shields(1, FILLER, FILLER, FILLER)
    return sc.start(), gm1, gm2, dagger1, dagger2


def test_two_identical_attackers_give_one_attack_move() -> None:
    st, gm1, gm2, dagger1, dagger2 = identical_attackers()
    dec = st.pending
    assert dec is not None
    every = [(gm, target) for gm in (gm1, gm2) for target in (PLAYER_TARGET, dagger1, dagger2)]
    assert sorted(attacks([a for _, a in canonical(st, dec)])) == sorted(every)
    assert attacks(merged(st)) == [(gm1, PLAYER_TARGET), (gm1, dagger1)]


def test_a_uid_specific_difference_keeps_both_attackers() -> None:
    """Two Gundam Heavyarms Custom (EW), one of which used its 【Once per Turn】 ability."""
    sc = Scenario()
    used = sc.add(0, "GD05-079")
    fresh = sc.add(0, "GD05-079")
    target = sc.add(1, "ST04-005", rested=True)
    sc.shields(1, FILLER, FILLER, FILLER)
    st = sc.start()
    activate(st, used)
    select_if_asked(st, target)
    assert has_action(st, A.ACTIVATE, fresh) and not has_action(st, A.ACTIVATE, used)
    assert {a for a, b in attacks(merged(st)) if b == PLAYER_TARGET} == {used, fresh}


def test_merging_keeps_both_moves_on_different_cards() -> None:
    sc = Scenario()
    sc.add(0, "ST01-005")
    sc.add(0, "ST01-005", damage=1)
    sc.shields(1, FILLER, FILLER, FILLER)
    st = sc.start()
    assert len(attacks(merged(st))) == 2


# ---------------------------------------------------------------------------------------------
# information-set honesty


ROOT_SETTINGS = config(
    noop_branch_choices=True,
    noop_resource_aware=True,
    strict_attack_prune=True,
    prune_noop_rest_targets=True,
    merge_equivalent_moves=True,
)


def decision(st: GameState, cfg: SearchConfig, seed: int) -> tuple[object, object]:
    assert st.pending is not None
    agent = make_agent("mcts", seed=seed, config=cfg, weights=BASELINE_WEIGHTS, log=True)
    action = agent.choose(st, st.pending.player)
    return action, agent.decision_log()[-1]["alternatives"]


SETTING_SCENARIOS: dict[str, Callable[[], GameState]] = {
    "noop_branch_choices": lambda: rests_the_blocker(rested_decoy=True, hidden=4),
    "noop_resource_aware": lambda: age_device(hidden=4),
    "strict_attack_prune": lambda: zero_ap_scenario(active_resources=2, hidden=4)[0],
    "prune_noop_rest_targets": lambda: amuro_rests(hidden=4)[0],
    "merge_equivalent_moves": lambda: identical_attackers(hidden=4)[0],
}


@pytest.mark.parametrize("setting", list(SETTING_SCENARIOS))
def test_each_setting_is_invariant_under_hidden_permutation_where_it_applies(
    setting: str,
) -> None:
    st = SETTING_SCENARIOS[setting]()
    assert st.pending is not None
    player = st.pending.player
    cfg = config(**{setting: True})
    expected = decision(st, cfg, SEEDS[0])
    assert expected != decision(st, BASE, SEEDS[0])
    variants = [permute_hidden(st, player, s) for s in PERMUTATION_SEEDS]
    assert any(_hidden_ids(v, player) != _hidden_ids(st, player) for v in variants)
    for variant in variants:
        assert information_set_key(variant, player) == information_set_key(st, player)
        assert decision(variant, cfg, SEEDS[0]) == expected


@pytest.mark.parametrize("index", range(len(GAME_SEEDS) * STATES_PER_GAME))
def test_root_settings_keep_decisions_invariant_under_hidden_permutation(index: int) -> None:
    st = midgame_states()[index]
    assert st.pending is not None
    player = st.pending.player
    seed = AGENT_SEEDS[index % len(AGENT_SEEDS)]
    expected = decision(st, ROOT_SETTINGS, seed)
    for s in PERMUTATION_SEEDS:
        assert decision(permute_hidden(st, player, s), ROOT_SETTINGS, seed) == expected
