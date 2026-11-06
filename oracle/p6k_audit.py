"""Generate the P6-K audit tables under docs/dev/audit/ (docs/dev/PLAN.md §12.2 P6-K).

- ``interpretations.json`` / ``.md`` (W-56, W-58): every recorded rules-interpretation decision
  the Rust engine must keep, with its ASSUMPTIONS/CONFLICTS anchor (the interpretation id), the tags
  that pin it (rules, cards, rulings, FAQ entries; the cards of its conflict ids are added
  from ``data/conflicts.json``), the Python tests carrying those tags
  (``oracle/corpus/v1/tier1/tests/tags.json``), the per-test traces that replay them through
  the Rust engine, and the re-review snapshot: the gcg-api rulings on its cards and the FAQ
  entries it names. ``crates/gcg-engine/tests/p6k_audit.rs`` checks every row against the
  corpus and lists every ruling that is not in a snapshot for re-review.
- ``regressions.json`` / ``.md`` (W-57, W-70): every historical strict xfail of the Python
  suite (an AST scan of the parent and the closing commit of 102e72b, 7c47b86, ddb9f4f,
  0e1d3e8 and 0ee591d) and every fixed bug the two entries list, each with the Python tests
  that cover it and their Rust evidence (per-test traces, or named native Rust tests).
- ``pilot-routing.md`` (W-59): for every Pilot card, the abilities the Pilot card keeps and the
  abilities the paired Unit gains (``compiler.route_abilities``, ``compiler/__init__.py:
  32-49``), summarised from the Python registry. ``crates/gcg-effects/tests/p6k_ir_scans.rs``
  rebuilds the same file from the IR byte for byte.

    uv run python oracle/p6k_audit.py --write
    uv run python oracle/p6k_audit.py --check    # exit 1 if a committed table is stale

``--check`` needs the git history of the five commits (the xfail scan reads them).
"""

from __future__ import annotations

import argparse
import ast
import json
import re
import subprocess
import sys
from collections.abc import Iterable
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "dev" / "audit"
CORPUS = ROOT / "oracle" / "corpus" / "v1" / "tier1" / "tests"
GENERATOR = "oracle/p6k_audit.py"
XFAIL_COMMITS = ("102e72b", "7c47b86", "ddb9f4f", "0e1d3e8", "0ee591d")

# (id, topic, anchor, conflict ids, rules, cards, rulings, faqs, test names)
INTERPRETATIONS: tuple[
    tuple[
        str,
        str,
        str,
        tuple[str, ...],
        tuple[str, ...],
        tuple[str, ...],
        tuple[str, ...],
        tuple[str, ...],
        tuple[str, ...],
    ],
    ...,
] = (
    (
        "I-01",
        "Bases deal no battle damage to the attacking Unit (every Base has 0 AP)",
        "docs/ASSUMPTIONS.md:54",
        ("rules-internal:base-ap",),
        ("8-5-2-4", "3-5-4"),
        (),
        (),
        (),
        (),
    ),
    (
        "I-02",
        "Exile = removal area; tokens leave the game; an EX Resource paid as a cost counts "
        "as exiled (GD05-018)",
        "docs/ASSUMPTIONS.md:56",
        ("rules-internal:remove-exile-terminology",),
        ("5-12", "5-17-2-5"),
        ("GD05-018",),
        (),
        (),
        (
            "test_exiled_cards_are_placed_into_the_removal_area",
            "test_removed_cards_go_face_up_to_their_owners_removal_area",
        ),
    ),
    (
        "I-03",
        "Placing beyond 15 Resources / 5 EX Resources does nothing",
        "docs/ASSUMPTIONS.md:59",
        ("faq:Q19",),
        ("4-4-2", "4-4-2-1"),
        (),
        (),
        ("Q19",),
        (),
    ),
    (
        "I-04",
        "Lasting 'all your Units' effects apply only to Units present at resolution",
        "docs/ASSUMPTIONS.md:60",
        ("faq:Q105",),
        (),
        (),
        (),
        ("Q105",),
        (),
    ),
    (
        "I-05",
        "uids are assigned after the seeded shuffle and cut",
        "docs/ASSUMPTIONS.md:61",
        (),
        (),
        (),
        (),
        (),
        (
            "test_decks_are_shuffled_and_placed_face_down",
            "test_trash_cards_shuffled_into_the_deck_lose_their_position",
        ),
    ),
    (
        "I-06",
        "Owner-ordered simultaneous placement into private zones; random orders use the seeded RNG",
        "docs/ASSUMPTIONS.md:63",
        (),
        ("4-1-6", "4-1-7"),
        (),
        (),
        (),
        (),
    ),
    (
        "I-07",
        "【Once per Turn】 is used up only when the effect performs an action",
        "docs/ASSUMPTIONS.md:65",
        ("ambiguous:once-per-turn-declined-may",),
        ("10-1-3", "10-3-3-1"),
        (),
        ("GD02-002:Q197",),
        (),
        (),
    ),
    (
        "I-08",
        "Delayed 'During this turn, when ...' triggers repeat",
        "docs/ASSUMPTIONS.md:66",
        (),
        ("10-1-6-1-1",),
        ("GD04-002",),
        (),
        (),
        (),
    ),
    (
        "I-09",
        "'During this battle' effects outside a battle do nothing",
        "docs/ASSUMPTIONS.md:67",
        ("ambiguous:during-this-battle-without-battle",),
        ("8-2-3", "8-6-1"),
        (),
        (),
        (),
        (),
    ),
    (
        "I-10",
        "Destruction cause: effect damage destroys 'with damage', 'destroyed by an effect' "
        "means destroy effects only",
        "docs/ASSUMPTIONS.md:68",
        (),
        (),
        (),
        ("GD05-035:Q361", "ST12-001:Q437", "ST12-003:Q439", "GD04-101:Q287", "GD05-054:Q368"),
        (),
        (),
    ),
    (
        "I-11",
        "Modal Commands need a mode whose targets can all be chosen",
        "docs/ASSUMPTIONS.md:69",
        (),
        (),
        (),
        ("ST12-015:Q450", "ST12-015:Q451", "ST14-013:Q469"),
        (),
        (),
    ),
    (
        "I-12",
        "Attack-target constraint priority (GD04-107 outranks 'if possible' attractors)",
        "docs/ASSUMPTIONS.md:70",
        (),
        (),
        ("GD04-107",),
        ("GD05-086:Q388",),
        (),
        (),
    ),
    (
        "I-13",
        "'Deploy it as an (APx･HPy) Unit'",
        "docs/ASSUMPTIONS.md:71",
        (),
        (),
        (),
        ("GD05-089:Q389", "GD05-089:Q390"),
        (),
        (),
    ),
    (
        "I-14",
        "Removal-area cards are not targets (10-2-2-1, read literally)",
        "src/gcg_sim/engine/interp.py:1051-1055",
        (),
        ("10-2-2-1",),
        (),
        (),
        (),
        (),
    ),
    (
        "I-15",
        "The last placed Shield is on top",
        "docs/CONFLICTS.md (faq:Q8)",
        ("faq:Q8",),
        (),
        (),
        (),
        ("Q8",),
        ("test_six_shields_are_placed_from_the_top_of_the_deck",),
    ),
    (
        "I-16",
        "A 0-AP attack on a player with no Base and no Shields does not defeat",
        "src/gcg_sim/engine/core.py:18-19",
        ("rules-internal:zero-ap-player-damage",),
        (),
        (),
        (),
        (),
        (
            "test_zero_ap_attack_on_a_player_with_empty_shield_area_deals_no_damage",
            "test_zero_ap_unit_may_attack_but_cannot_destroy_a_shield",
        ),
    ),
    (
        "I-17",
        "【When Linked】 triggers only on pairing",
        "docs/CONFLICTS.md (rules-internal:when-linked-set)",
        ("rules-internal:when-linked-set",),
        ("13-2-11-1",),
        (),
        (),
        (),
        (
            "test_when_linked_activates_when_a_link_pilot_is_paired",
            "test_when_linked_does_not_activate_for_a_non_link_pilot",
        ),
    ),
    (
        "I-18",
        "<Suppression> with a Base present does nothing",
        "docs/CONFLICTS.md (ambiguous:suppression-with-base)",
        ("ambiguous:suppression-with-base",),
        (),
        (),
        (),
        (),
        ("test_suppression_does_not_apply_while_a_base_takes_the_damage",),
    ),
    (
        "I-19",
        "The old <Breach> reminder wording is ignored",
        "docs/CONFLICTS.md (ambiguous:breach-reminder)",
        ("ambiguous:breach-reminder",),
        (),
        (),
        (),
        (),
        (),
    ),
    (
        "I-20",
        "'Pay its cost to deploy it' skips the Lv. check",
        "docs/CONFLICTS.md (ambiguous:pay-its-cost-to-deploy)",
        ("ambiguous:pay-its-cost-to-deploy",),
        (),
        (),
        (),
        (),
        (),
    ),
    (
        "I-21",
        "'If ...' / 'If you do' govern a following 'Then'",
        "docs/ASSUMPTIONS.md, rulings Q175, Q233",
        (),
        (),
        (),
        ("GD02-021:Q175", "GD03-072:Q233"),
        (),
        (
            "test_leading_condition_also_governs_the_then_part",
            "test_if_you_do_also_governs_the_following_then_part",
        ),
    ),
    (
        "I-22",
        "Resolutions that remove a player decision (ST14-015, GD03-129, GD03-076)",
        "src/gcg_sim/engine/interp.py:1033-1038",
        (),
        (),
        ("ST14-015", "GD03-129", "GD03-076"),
        (),
        (),
        (),
    ),
    (
        "I-23",
        "Multiplayer wording in 1v1 ('each enemy player' = the opponent)",
        "docs/ASSUMPTIONS.md:57",
        ("ambiguous:multiplayer-vocabulary",),
        (),
        (),
        (),
        (),
        (),
    ),
    (
        "I-24",
        "Pilot text routing: 【Burst】, name-alias, play-modifier and hand/trash lines stay "
        "with the Pilot",
        "docs/ASSUMPTIONS.md:58",
        (),
        ("3-3-9-1", "3-3-9-2"),
        (),
        (),
        (),
        (),
    ),
    (
        "I-25",
        "ST12-001 errata applied through an override (major)",
        "docs/CONFLICTS.md (errata-unapplied:ST12-001:news-02_193)",
        ("errata-unapplied:ST12-001:news-02_193",),
        (),
        (),
        (),
        (),
        (),
    ),
    (
        "I-26",
        "GD02-053 [Suppression] read as <Suppression> (major)",
        "docs/CONFLICTS.md (markers:GD02-053:unbracketed-keyword)",
        ("markers:GD02-053:unbracketed-keyword",),
        (),
        (),
        (),
        (),
        (),
    ),
    (
        "I-27",
        "ST13-013 simultaneous 1-to-2 token deploys",
        "docs/CONFLICTS.md (ambiguous:ST13-013:deploy-1-to-2-tokens)",
        ("ambiguous:ST13-013:deploy-1-to-2-tokens",),
        (),
        (),
        (),
        (),
        (),
    ),
)

# (bug text, entry, Python test names, native Rust tests "path::fn", not-applicable reason)
BUGS: tuple[tuple[str, str, tuple[str, ...], tuple[str, ...], str], ...] = (
    (
        "battle LKI and Base damage",
        "W-57",
        (
            "test_gd02_001_breach_destroying_the_base_recovers",
            "test_gd04_042_breach_destroying_the_base_triggers",
            "test_gd05_035_breach_destroying_the_base_triggers",
        ),
        (),
        "",
    ),
    (
        "<Breach> amount read from the destroyed host",
        "W-57",
        (
            "test_breach_activates_when_both_units_are_destroyed",
            "test_q55_breach_activates_when_both_units_are_destroyed",
            "test_t026_breach_still_activates_when_both_units_are_destroyed",
            "test_st12_004_breach_when_both_units_are_destroyed",
        ),
        (),
        "",
    ),
    (
        "【Destroyed】 ignoring the last-known paired Pilot",
        "W-57",
        (
            "test_destroyed_effect_returns_the_last_known_paired_pilot",
            "test_q69_destroyed_effect_finds_the_pilot_paired_before_destruction",
        ),
        (),
        "",
    ),
    (
        "HP filters using printed HP (Q96)",
        "W-57",
        ("test_q96_hp_in_card_text_is_current_hp",),
        (),
        "",
    ),
    (
        "<Support> bonus lost with its source (Q106)",
        "W-57",
        ("test_q106_support_bonus_outlives_the_support_unit",),
        (),
        "",
    ),
    ("no concession entry point", "W-57", ("test_either_player_may_concede_at_any_time",), (), ""),
    (
        "'All players draw 1' defeating only the active player",
        "W-57",
        (
            "test_all_players_draw_empties_both_decks_and_defeats_both",
            "test_a_symmetric_draw_that_empties_both_decks_defeats_both_players",
        ),
        (),
        "",
    ),
    (
        "new_game accepting non-conforming decks",
        "W-57",
        ("test_decks_that_do_not_conform_to_deck_construction_are_rejected",),
        (),
        "",
    ),
    (
        "phase/step labels advancing early",
        "W-57",
        (
            "test_end_phase_step_is_reported_until_its_effects_have_resolved",
            "test_start_phase_is_reported_until_start_step_effects_have_resolved",
        ),
        (),
        "",
    ),
    (
        "deck placement ordering and information leaks (4-1-6, 4-1-7, 4-2-2)",
        "W-57",
        (
            "test_owner_orders_cards_placed_into_a_location_together",
            "test_order_of_public_cards_placed_into_a_private_location_is_hidden",
            "test_cards_returned_randomly_to_the_deck_do_not_reveal_their_order",
        ),
        (),
        "",
    ),
    (
        "'during this battle' effects never expiring outside a battle",
        "W-57",
        (
            "test_during_this_battle_effect_outside_a_battle_does_not_outlast_the_turn",
            "test_eb01_070_outside_a_battle_the_bonus_does_not_outlive_the_turn",
            "test_gd01_058_outside_a_battle_the_bonus_does_not_last_into_later_turns",
            "test_gd01_082_used_in_end_phase_does_not_outlast_the_turn",
            "test_gd02_106_played_in_end_phase_does_not_protect_later_battles",
            "test_st12_006_suppression_activated_outside_a_battle_does_not_persist",
        ),
        (),
        "",
    ),
    (
        "a Unit trashed by an effect not treated as destroyed",
        "W-57",
        ("test_effect_placing_a_field_unit_into_the_trash_destroys_it",),
        (),
        "",
    ),
    (
        "play modifiers skipped",
        "W-57",
        (
            "test_eb01_039_plays_as_lv3_cost3_with_three_enemy_units",
            "test_gd01_002_free_play_ignores_level_and_resources",
            "test_gd03_085_free_pairing_offered_with_no_active_resources",
            "test_st10_014_alt_play_offered_with_only_two_resources",
        ),
        (),
        "",
    ),
    (
        "missing COST_PAID",
        "W-57",
        (
            "test_gd04_100_q286_paying_to_deploy_with_x_divider_counts",
            "test_gd04_129_q297_paying_to_deploy_with_x_divider_counts",
            "test_gd05_101_paying_to_deploy_with_x_divider_counts",
        ),
        (),
        "",
    ),
    (
        "Once per Turn keys and LKI zone_seq",
        "W-57",
        (
            "test_st12_001_once_per_turn_when_destroyed_in_the_second_battle",
            "test_eb01_004_once_per_turn_is_not_used_when_no_target_could_be_chosen",
            "test_gd03_076_once_per_turn_not_used_up_when_nothing_was_returned",
        ),
        (),
        "",
    ),
    (
        "missing command-activation events",
        "W-57",
        (
            "test_gd04_066_burst_activating_a_command_main_triggers",
            "test_gd05_068_main_activated_by_an_effect_also_grants_suppression",
            "test_gd05_089_counts_main_activated_on_a_paired_special_move_command",
            "test_gd05_097_activation_counts_as_activating_a_special_move_main",
        ),
        (),
        "",
    ),
    (
        "effect-damage destruction attribution",
        "W-57",
        (
            "test_st12_001_q437_triggers_when_destroying_with_effect_damage",
            "test_st12_003_q439_triggers_when_destroying_with_effect_damage",
        ),
        (),
        "",
    ),
    (
        "one-shot delayed triggers",
        "W-57",
        (
            "test_gd04_002_delayed_effect_fires_for_every_destruction_this_turn",
            "test_gd04_035_draws_when_both_units_are_destroyed",
        ),
        (),
        "",
    ),
    (
        "modal Commands with no legal mode",
        "W-57",
        (
            "test_st12_015_not_playable_without_a_legal_mode",
            "test_st14_013_q469_not_playable_without_enemy_units",
        ),
        (),
        "",
    ),
    (
        "shield damage prevention and thresholds",
        "W-57",
        (
            "test_gd03_070_rested_freedom_protects_shields_from_enemy_battle_damage",
            "test_gd05_123_three_enemy_effect_damage_is_received",
            "test_st11_006_reduces_six_breach_damage_to_one",
        ),
        (),
        "",
    ),
    (
        "paired Pilot destruction",
        "W-57",
        (
            "test_gd03_110_destroys_the_pilot_and_keeps_the_unit",
            "test_gd03_110_link_unit_deployed_this_turn_can_no_longer_attack",
        ),
        (),
        "",
    ),
    (
        "forced/attracted attack targets",
        "W-57",
        (
            "test_gd04_107_q301_active_chosen_unit_binds_attackers_allowed_to_hit_active",
            "test_gd04_107_q289_outranks_if_possible_attractors",
            "test_gd05_086_link_unit_attackers_are_not_attracted",
        ),
        (),
        "",
    ),
    (
        "reused rest substitutes and shared lasting keys",
        "W-57",
        (
            "test_gd05_124_replaces_only_one_of_two_units",
            "test_gd04_108_second_copy_reduces_a_later_damage",
        ),
        (),
        "",
    ),
    (
        "HasKeyword off the field",
        "W-57",
        ("test_gd04_094_when_linked_adds_purple_suppression_unit_from_trash",),
        (),
        "",
    ),
    (
        "redirected battle damage",
        "W-57",
        ("test_gd04_095_q283_unit_killed_by_redirected_damage_is_destroyed_in_battle",),
        (),
        "",
    ),
    (
        "turn-history gaps",
        "W-57",
        (
            "test_gd05_129_destruction_by_a_non_neo_zeon_effect_does_not_count",
            "test_st12_016_damage_after_paired_unit_destroys_with_battle_damage",
            "test_st12_016_q452_counts_when_both_units_are_destroyed",
            "test_st14_015_resource_set_active_by_another_effect_this_turn",
        ),
        (),
        "",
    ),
    (
        "effect-activated paired Commands",
        "W-57",
        ("test_paired_command_effect_activated_by_an_effect_resolves_as_a_command_effect",),
        (),
        "",
    ),
    (
        "effect-begun battles see battling effects (Q447, Q455)",
        "W-57",
        (
            "test_st12_013_q447_carta_gives_the_battling_unit_first_strike",
            "test_st12_013_q447_enemy_battle_effects_apply_in_effect_battles",
        ),
        (),
        "",
    ),
    (
        "simultaneous 'all players each' effects",
        "W-57",
        (
            "test_eb01_023_q319_both_players_act_before_defeat_is_checked",
            "test_eb01_023_each_player_looks_in_turn_order",
        ),
        (),
        "",
    ),
    (
        "multi-token deployments",
        "W-57",
        ("test_eb01_022_end_of_turn_trade_for_three_tokens",),
        (),
        "",
    ),
    (
        "printings counted separately in the deck check (2-1-1)",
        "W-57",
        ("test_decks_that_do_not_conform_to_deck_construction_are_rejected",),
        ("crates/gcg-data/tests/decks.rs::deck_cases_equal_python",),
        "",
    ),
    (
        "FAQ Q1-Q6 once N/A",
        "W-57",
        (),
        (
            "crates/gcg-engine/tests/traceability_deck.rs::main_deck_must_have_exactly_50_cards",
            "crates/gcg-engine/tests/traceability_deck.rs::at_most_two_colors",
            "crates/gcg-engine/tests/traceability_deck.rs::at_most_four_copies_per_card_number",
            "crates/gcg-engine/tests/traceability_deck.rs::"
            "same_name_with_different_card_numbers_counts_separately",
            "crates/gcg-engine/tests/traceability_deck.rs::resource_deck_must_have_exactly_10_cards",
            "crates/gcg-engine/tests/traceability_deck.rs::"
            "resource_deck_allows_any_number_of_one_card_number",
        ),
        "",
    ),
    (
        "unclean errors for non-UTF-8 replay files and failed report writes",
        "W-57",
        (),
        (
            "crates/gcg-data/tests/decks.rs::deck_cases_equal_python",
            "crates/gcg-cli/tests/commands.rs::command_transcripts_match_python",
            "crates/gcg-cli/tests/benchmark.rs::needs_engine_a_report_write_failure_names_the_file",
        ),
        "",
    ),
    (
        "`tuning --rounds 0` mislabelling the start weights",
        "W-57",
        (),
        (),
        "The weight-tuning tool (`ai/tuning.py`) has no Rust counterpart yet; P6-I's "
        "`gcg-eval tune` carries its own test of the round count",
    ),
    (
        "Then/If scoping (GD03-072, GD02-021)",
        "W-70",
        (
            "test_leading_condition_also_governs_the_then_part",
            "test_if_you_do_also_governs_the_following_then_part",
        ),
        (),
        "",
    ),
    (
        "'Choose 2' playable with one target",
        "W-70",
        ("test_choose_two_needs_two_choosable_targets",),
        (),
        "",
    ),
    (
        "'instead' losing 'during this turn' (ST07-009)",
        "W-70",
        ("test_substituted_event_keeps_the_replaced_duration",),
        (),
        "",
    ),
    (
        "'instead' replacing the damage step (GD03-109)",
        "W-70",
        ("test_instead_portion_replaces_only_what_it_names",),
        (),
        "",
    ),
    (
        "multi-target pre-checks",
        "W-70",
        (
            "test_triggered_effect_needs_every_target_of_its_choice",
            "test_st10_015_not_playable_with_g_generation_unit_but_no_enemy_unit",
        ),
        (),
        "",
    ),
    (
        "cards that did not compile (ST03-011, GD03-085, GD05-113, Development N)",
        "W-70",
        (
            "test_q89_char_aznable_on_an_amuro_ray_unit",
            "test_pilot_text_with_a_location_is_activated_by_the_pilot_card",
            "test_paired_command_effect_activated_by_an_effect_resolves_as_a_command_effect",
            "test_development_exiles_cards_then_performs_the_following_effect",
            "test_declining_development_skips_the_following_effect",
        ),
        (),
        "",
    ),
    (
        "deployed-rested statics (Q354)",
        "W-70",
        ("test_gd04_022_unit_is_already_rested_when_its_deploy_effect_resolves",),
        (),
        "",
    ),
    (
        "deploying a Pilot as a Unit (Q389/Q390)",
        "W-70",
        (
            "test_gd05_089_burst_may_deploy_it_as_ap3_hp3_unit",
            "test_gd05_089_deployed_as_unit_is_lv6",
            "test_gd05_089_deployed_as_unit_can_be_paired",
        ),
        (),
        "",
    ),
    (
        "start-phase freezes (EB01-001, ST14-001) as the stays-rested rule (Q208/Q288)",
        "W-70",
        (
            "test_gd04_102_q288_effects_may_still_set_it_active",
            "test_st08_009_effect_can_still_set_the_unit_active",
            "test_eb01_001_freeze_only_blocks_the_start_phase_untap",
        ),
        (),
        "",
    ),
    (
        "fragile per-card bindings repaired at integration (7c47b86)",
        "W-70",
        (
            "test_st10_015_played_outside_battle_does_not_persist_past_the_turn",
            "test_gd04_113_outside_a_battle_has_no_lasting_effect",
        ),
        ("crates/gcg-effects/src/tests/golden.rs::all_golden_files_regenerate_byte_for_byte",),
        "",
    ),
)
FAQ_BUG_IDS = ("Q1", "Q2", "Q3", "Q4", "Q5", "Q6")


def git(*args: str) -> str:
    out = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True)
    return out.stdout


def load(rel: str) -> Any:
    return json.loads((ROOT / rel).read_text(encoding="utf-8"))


def dump(obj: object) -> str:
    return json.dumps(obj, indent=1, ensure_ascii=False) + "\n"


def corpus_traces() -> dict[str, str]:
    """Every per-test trace id of the tier-1 ``tests`` set with its recorded outcome."""
    out: dict[str, str] = {}
    for path in sorted(CORPUS.rglob("*.ndjson")):
        current = None
        for line in path.read_text(encoding="utf-8").splitlines():
            rec = json.loads(line)
            if rec["t"] == "header":
                current = rec["id"]
            elif rec["t"] == "end" and current is not None:
                out[current] = rec["outcome"]
                current = None
    return out


def known_tests(traces: dict[str, str]) -> dict[str, list[str]]:
    """Python test function name -> its test ids (traced and untraced)."""
    manifest = json.loads((CORPUS / "MANIFEST-tests.json").read_text(encoding="utf-8"))
    by_name: dict[str, list[str]] = {}
    for tid in [*traces, *manifest["tests"]["untraced"]]:
        base = tid.split("[", 1)[0]
        by_name.setdefault(base.rsplit("::", 1)[-1], []).append(tid)
    return {k: sorted(set(v)) for k, v in by_name.items()}


def evidence(test_ids: Iterable[str], traces: dict[str, str]) -> list[str]:
    return sorted(t for t in test_ids if traces.get(t) == "passed")


# ---------------------------------------------------------------------------------------------
# W-56, W-58


def interpretations(traces: dict[str, str]) -> dict[str, Any]:
    tags = load("oracle/corpus/v1/tier1/tests/tags.json")["tags"]
    conflicts = {c["id"]: c for c in load("data/conflicts.json")["conflicts"]}
    rulings = load("src/gcg_sim/data/gcgapi/rulings.json")
    names = known_tests(traces)
    rows = []
    for iid, topic, anchor, cids, rules, cards, ruls, faqs, tests in INTERPRETATIONS:
        for cid in cids:
            if cid not in conflicts:
                raise SystemExit(f"p6k_audit: {iid} names unknown conflict {cid}")
        all_cards = sorted({*cards, *(n for c in cids for n in conflicts[c]["card_numbers"])})
        wanted = {("rule", r) for r in rules} | {("card", c) for c in all_cards}
        wanted |= {("ruling", r) for r in ruls} | {("faq", f) for f in faqs}
        python = sorted(
            {f"{t['path']}::{t['test']}" for t in tags if (t["mark"], t["value"]) in wanted}
        )
        for name in tests:
            if name not in names:
                raise SystemExit(f"p6k_audit: {iid} names unknown test {name}")
            python = sorted({*python, *(i.split("[", 1)[0] for i in names[name])})
        rust = sorted(
            {tid for tid in traces if tid.split("[", 1)[0] in python and traces[tid] == "passed"}
        )
        review = sorted(
            f"{r['card_number']}:{r['num']}" for r in rulings if r["card_number"] in all_cards
        )
        rows.append(
            {
                "id": iid,
                "topic": topic,
                "anchor": anchor,
                "conflicts": list(cids),
                "tags": {
                    "rule": list(rules),
                    "card": all_cards,
                    "ruling": list(ruls),
                    "faq": list(faqs),
                },
                "python_tests": python,
                "rust_traces": rust,
                "review_snapshot": {"rulings": review, "faq": sorted(faqs)},
            }
        )
    return {"version": 1, "generator": GENERATOR, "interpretations": rows}


def interpretations_md(doc: dict[str, Any]) -> str:
    out = [
        "# Interpretation ids and their Rust evidence (W-56, W-58)",
        "",
        f"Generated by `uv run python {GENERATOR} --write` from the anchors below, the Python",
        "tags (`oracle/corpus/v1/tier1/tests/tags.json`) and the tier-1 per-test traces. Each",
        "interpretation id names one recorded rules or card-text decision that the Rust engine",
        "keeps exactly. `Rust` counts the per-test traces of its Python tests that the Rust",
        "engine replays (`gcg-diff corpus --tier 1 --require-all`).",
        "`crates/gcg-engine/tests/p6k_audit.rs` checks each row and prints every gcg-api ruling",
        "on one of its cards that is not in the review snapshot (the re-review list of a data",
        "refresh).",
        "",
        "| id | decision | anchor | tags | Python tests | Rust traces |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for r in doc["interpretations"]:
        tags = [f"{m}:{v}" for m in ("rule", "ruling", "faq") for v in r["tags"][m]]
        cards = r["tags"]["card"]
        if cards:
            shown = ", ".join(cards[:6]) + (f" (+{len(cards) - 6})" if len(cards) > 6 else "")
            tags.append(f"card:{shown}")
        tags += [f"conflict:{c}" for c in r["conflicts"]]
        out.append(
            f"| {r['id']} | {r['topic']} | `{r['anchor']}` | {'; '.join(tags) or '—'} | "
            f"{len(r['python_tests'])} | {len(r['rust_traces'])} |"
        )
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------------------------
# W-57, W-70


def _xfail_tests(src: str) -> dict[str, str]:
    """Test functions of a module that carry a strict xfail mark, with the mark's source."""
    tree = ast.parse(src)
    mark_names = {
        t.id
        for node in tree.body
        if isinstance(node, ast.Assign) and "xfail" in ast.unparse(node.value)
        for t in node.targets
        if isinstance(t, ast.Name)
    }
    marks = {
        t.id: ast.unparse(node.value)
        for node in tree.body
        if isinstance(node, ast.Assign)
        for t in node.targets
        if isinstance(t, ast.Name) and t.id in mark_names
    }
    out: dict[str, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef) and node.name.startswith(
            "test"
        ):
            for dec in node.decorator_list:
                text = ast.unparse(dec)
                if "xfail" in text:
                    out[node.name] = text
                elif text in marks:
                    out[node.name] = marks[text]
    return out


def _reason(mark: str) -> str:
    m = re.search(r"reason=(['\"])(.*?)\1\s*[,)]", mark, re.S)
    return " ".join(m.group(2).split()) if m else mark


def historical_xfails() -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for commit in XFAIL_COMMITS:
        files = [
            f
            for f in git("show", "--name-only", "--format=", commit).split()
            if f.startswith("tests/") and f.endswith(".py")
        ]
        for path in files:
            try:
                before = git("show", f"{commit}^:{path}")
            except subprocess.CalledProcessError:
                continue
            try:
                after = _xfail_tests(git("show", f"{commit}:{path}"))
            except subprocess.CalledProcessError:
                after = {}
            for name, mark in sorted(_xfail_tests(before).items()):
                if name not in after:
                    rows.append(
                        {"commit": commit, "test": f"{path}::{name}", "reason": _reason(mark)}
                    )
    return rows


def regressions(traces: dict[str, str]) -> dict[str, Any]:
    names = known_tests(traces)
    tags = load("oracle/corpus/v1/tier1/tests/tags.json")["tags"]
    xfails = historical_xfails()
    for row in xfails:
        base = row["test"]
        row_traces = sorted(t for t in traces if t.split("[", 1)[0] == base)
        row["rust_traces"] = evidence(row_traces, traces)
    bugs = []
    for text, entry, tests, native, na in BUGS:
        ids: list[str] = []
        for name in tests:
            if name not in names:
                raise SystemExit(f"p6k_audit: bug {text!r} names unknown test {name}")
            ids += names[name]
        if text.startswith("FAQ Q1-Q6"):
            ids += [
                f"{t['path']}::{t['test']}"
                for t in tags
                if t["mark"] == "faq" and t["value"] in FAQ_BUG_IDS
            ]
        ids = sorted(set(ids))
        bugs.append(
            {
                "bug": text,
                "entry": entry,
                "python_tests": ids,
                "rust_traces": evidence(
                    {t for t in traces if t.split("[", 1)[0] in {i.split("[", 1)[0] for i in ids}},
                    traces,
                ),
                "rust_native": list(native),
                "not_applicable": na,
            }
        )
    return {
        "version": 1,
        "generator": GENERATOR,
        "xfail_commits": list(XFAIL_COMMITS),
        "xfails": xfails,
        "bugs": bugs,
    }


def regressions_md(doc: dict[str, Any]) -> str:
    out = [
        "# Historical Python fixes and their Rust regression tests (W-57, W-70)",
        "",
        f"Generated by `uv run python {GENERATOR} --write`. The xfail rows come from an AST",
        "scan of each closing commit and its parent; a test that lost its strict xfail mark is",
        "listed once (`_DEVELOPMENT_XFAIL` in `tests/rules/test_keywords.py` marked two tests).",
        "`Rust` lists the per-test traces that replay the Python test through the Rust engine",
        "(`gcg-diff corpus --tier 1 --require-all`) or the native Rust tests that cover it.",
        "`crates/gcg-engine/tests/p6k_audit.rs` fails if a row has no passing Rust evidence.",
        "",
        f"Historical strict xfails: {len(doc['xfails'])} tests in "
        f"{len(doc['xfail_commits'])} closing commits.",
        "",
        "| commit | Python test | xfail reason | Rust traces |",
        "| --- | --- | --- | --- |",
    ]
    for r in doc["xfails"]:
        reason = r["reason"].replace("|", "\\|")
        out.append(f"| {r['commit']} | `{r['test']}` | {reason} | {len(r['rust_traces'])} |")
    out += [
        "",
        "| entry | fixed bug | Python tests | Rust traces | native Rust tests |",
        "| --- | --- | --- | --- | --- |",
    ]
    for b in doc["bugs"]:
        native = ", ".join(f"`{n}`" for n in b["rust_native"]) or (
            f"n/a: {b['not_applicable']}" if b["not_applicable"] else "—"
        )
        out.append(
            f"| {b['entry']} | {b['bug']} | {len(b['python_tests'])} | "
            f"{len(b['rust_traces'])} | {native} |"
        )
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------------------------
# W-59


def _summary(ab: Any) -> str:
    """One ability as ``Class(part, ...)``; the IR side builds the same string from node
    fields (``crates/gcg-effects/tests/p6k_ir_scans.rs``)."""
    from gcg_sim.effects import dsl as d

    cls = type(ab).__name__
    parts: list[str] = []
    if isinstance(ab, d.Triggered):
        parts.append(ab.trigger.event.value)
    elif isinstance(ab, d.Activated | d.Command):
        parts.append(ab.timing.value)
    elif isinstance(ab, d.Keyword):
        parts.append(ab.keyword.value + (f" {ab.amount}" if ab.amount else ""))
    elif isinstance(ab, d.NameAlias):
        parts.extend(ab.names)
    elif isinstance(ab, d.Replacement):
        parts.append(ab.event.value)
    where = getattr(ab, "where", None)
    if where is not None and where is not d.Where.FIELD:
        parts.append(f"where={where.value}")
    gate = getattr(ab, "gate", None)
    if gate is not None and gate is not d.Gate.NONE:
        parts.append(f"gate={gate.value}")
    return f"{cls}({', '.join(parts)})" if parts else cls


def pilot_routing_md() -> str:
    from gcg_sim.cards.model import CardType
    from gcg_sim.effects.registry import get_registry

    reg = get_registry()
    out = [
        "# Pilot text routing audit (W-59)",
        "",
        "Generated by `uv run python oracle/p6k_audit.py --write` from the Python registry",
        "(`compiler.route_abilities`, `src/gcg_sim/effects/compiler/__init__.py:32-49`, and the",
        "bindings); `crates/gcg-effects/tests/p6k_ir_scans.rs` regenerates it from the Rust IR",
        "and requires identical bytes. A changed split, or a new Pilot card, shows up as a",
        "reviewed diff of this file. 【Burst】, name-alias, play-modifier and hand/trash",
        "abilities stay with the Pilot card; every other ability is gained by the paired Unit.",
        "",
        "| card | name | kept by the Pilot card | gained by the paired Unit |",
        "| --- | --- | --- | --- |",
    ]
    n = 0
    for cdef in reg.db:
        if cdef.card_type is not CardType.PILOT:
            continue
        script = reg.cards[cdef.def_id].script
        own = "; ".join(_summary(a) for a in script.abilities) if script else ""
        unit = "; ".join(_summary(a) for a in script.unit_abilities) if script else ""
        out.append(f"| {cdef.card_number} | {cdef.name} | {own or '—'} | {unit or '—'} |")
        n += 1
    out += ["", f"Pilot cards: {n}."]
    return "\n".join(out) + "\n"


def build() -> dict[str, str]:
    traces = corpus_traces()
    interp = interpretations(traces)
    regs = regressions(traces)
    return {
        "interpretations.json": dump(interp),
        "interpretations.md": interpretations_md(interp),
        "regressions.json": dump(regs),
        "regressions.md": regressions_md(regs),
        "pilot-routing.md": pilot_routing_md(),
    }


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Generate the P6-K audit tables.")
    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    args = ap.parse_args(argv)
    stale = []
    for name, text in build().items():
        path = OUT / name
        if args.write:
            OUT.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        elif not path.is_file() or path.read_text(encoding="utf-8") != text:
            stale.append(name)
    for name in stale:
        print(f"p6k_audit: {OUT / name} is stale; run with --write", file=sys.stderr)
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
