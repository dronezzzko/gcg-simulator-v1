# Statistics used by gcg-sim reports

## Wilson score interval (95%)

For `k` wins in `n` games, `p̂ = k/n`, `z = 1.96`:

```
centre = (p̂ + z²/(2n)) / (1 + z²/n)
half   = z · sqrt(p̂(1−p̂)/n + z²/(4n²)) / (1 + z²/n)
interval = [centre − half, centre + half]
```

Wilson intervals stay inside [0, 1] and behave well for small `n` and extreme rates, unlike the
normal approximation. Draws count as non-wins in the win rate; they are reported separately.

## Sample size for a target precision

Half-width `h` at `p ≈ 0.5`: `n ≈ z² · 0.25 / h²` → ±10 points: 97 games; ±7: 196; ±5: 385; ±3: 1,068.

## Sample size to compare two variants

Two-sided test at 95% with 80% power (`z_β = 0.84`), difference `Δ` near 50%:
`n per variant ≈ (z + z_β)² · 2 · 0.25 / Δ²` → Δ = 10 points: 393; 7 points: 801; 5 points: 1,570.

## Match vs game rates

A BO3 match is won by the first player to two game wins; match win rate amplifies game win rate
(e.g. 55% per game ≈ 57.5% per match). Games within a match are correlated (same decks, loser
chooses first player), so use game-level counts for sample-size planning and match-level rates
for the headline.
