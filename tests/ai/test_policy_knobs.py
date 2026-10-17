"""Policy settings carried by a search configuration (``SearchConfig.policy_*``)."""

from __future__ import annotations

import dataclasses

from tests.ai.situations import burst

from gcg_sim.ai import policy
from gcg_sim.ai.baseline import BASELINE_PRESETS
from gcg_sim.ai.policy import BASELINE_KNOBS, PolicyKnobs, knobs_for
from gcg_sim.engine.types import ActionKind
from gcg_sim.rng import SplitMix64

SAMPLES = 1000


def test_baseline_configurations_use_the_baseline_policy() -> None:
    assert all(knobs_for(c) == BASELINE_KNOBS for c in BASELINE_PRESETS.values())


def test_burst_score_sets_how_often_rollouts_accept_a_burst() -> None:
    st = burst()
    dec = st.pending
    assert dec is not None

    def declines(knobs: PolicyKnobs) -> int:
        rng = SplitMix64(7)
        scores = policy.option_scores(st, dec, knobs)
        picks = (policy.pick(dec.options, scores, rng, 0.5) for _ in range(SAMPLES))
        return sum(1 for a in picks if a.kind is ActionKind.NO)

    sure = knobs_for(dataclasses.replace(BASELINE_PRESETS["standard"], policy_burst_yes_score=4.0))
    assert sure == PolicyKnobs(burst_yes_score=4.0)
    assert declines(BASELINE_KNOBS) > 80
    assert declines(sure) <= 2
