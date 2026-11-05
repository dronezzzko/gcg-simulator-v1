# Blinded audit of AI play (Gundam Card Game, 1v1)

You are an independent auditor. You did not write the AI and did not generate these games. Your
job is to find the implausible plays a competent Gundam Card Game player would call clear
mistakes. Both players in every game are the same AI. You are not told which AI version played
which game, and you must not try to find out.

## What you may read
- Only the `.txt` files in your own packet directory (given in your task), this file, and:
  - card text and rulings: run `uv run python -m gcg_sim.tools.explain <CARD_NUMBER>` from the
    repository root `/Users/drone/dev/src/personal/gcg-simulator-v1` (read-only);
  - the rules: `/Users/drone/dev/src/personal/gcg-simulator-v1/gundam-card-game-comprehensive-rules.md`.
- Do NOT open any other file in the scratchpad (other auditors' packets, key or assignment files),
  nor anything under `docs/port/eval/`, `crates/`, `build/` or `target/`, nor run any game, AI or
  benchmark command. The audit is blinded.

## Transcript format
Each decision is a block `#k turn T Pn <kind> prompt: ...` with the decider's state (shields,
resources active/total, EX resources, deck, trash, base, units with Lv/AP/HP, rested or active,
keywords in braces, paired pilots after `+`), the opponent's public state, the decider's hand,
`options:` (every legal action) and `PLAYED:` (the action taken). Lines `... (only option)` are
forced. The opponent's hand is printed as "hidden from the decider" for reference: judge every
decision only from the decider's information (its own hand, the public board, what has been
revealed so far), never from the hidden hand. Card names are followed by instance ids in
brackets. Attacks name the attacker and the target (`the player` or a unit).

Key rules for lethal: a player with no shields and no base who takes battle damage from an attack
that is not blocked loses (check the rules file for blockers, `<Blocker>`, `<First Strike>`,
`<Breach>`, `<High-Maneuver>` and the like).

## What to look for (categories)
- `missed_lethal`: from the decider's information, a sequence of legal actions this turn wins the
  game for certain (taking possible blocks, known effects and the opponent's open resources into
  account), and the AI did not take it.
- `passivity`: ending a phase or passing when an available action is clearly better (e.g. not
  deploying a unit it can afford and that has no downside, not attacking when the attack is free).
- `questionable_attack`: an attack that clearly loses material or tempo with no compensating gain.
- `wasted_effect`: a card or effect played to no effect (no valid target, an already-full effect,
  overwritten immediately) when not playing it was available.
- `bad_target`: a target choice clearly worse than an available alternative.
- `other`: any other clear mistake; describe it.

For each finding give a verdict: `clear` only when a competent player would agree it is a mistake
and you can name the better alternative and why; otherwise `arguable`. When in doubt, `arguable`.
Do not report forced decisions. Card-game AIs face hidden information: a play that is reasonable
given what the decider could know is not a mistake even if it turned out badly.

## Full transcripts and excerpts
- `game-<id>.txt` (full game): review every decision of both players that had more than one
  option.
- `excerpt-<id>-t<T>-p<P>.txt`: every decision of one turn; review only the decisions of player P
  in that turn.

## Output
Write `audit.json` in your packet directory:

```json
{"auditor": "auditor-N",
 "items": [
  {"file": "game-....txt", "decisions_reviewed": 123,
   "findings": [{"decision": "#41", "turn": 9, "player": 0, "category": "missed_lethal",
                 "verdict": "clear", "played": "...", "better": "...", "reason": "..."}],
   "notes": "anything relevant, e.g. decisions you could not judge"}]}
```

Every file of your packet must appear in `items`, also when it has no finding (empty list).
`decisions_reviewed` counts the non-forced decisions you reviewed (for excerpts: of player P).
Finish with a short message: per file, the number of clear and arguable findings.
