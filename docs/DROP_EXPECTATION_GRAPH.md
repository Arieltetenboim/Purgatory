# Expected drop graph

Item Lab WHERE USED and the Mob Lab DROPS graph both call `tools/authoring_chart.js`. The graph is per monster, per item, per row. It is not a global drop chance. Presets are 100, 1,000, 10,000, and 100,000 kills. Inspect N evaluates the formula, including values such as 0.01% of quantity 1–2, and does not read the drawn pixels.

For chance `p` and quantity bounds `a` and `b`:

- mean quantity `m = (a + b) / 2`
- expected successes after `N` eligible kills: `N * p`
- expected units after `N` eligible kills: `N * p * m`

At 10% and quantity 1, `N = 1000` expects 100 successes and 100 units. At 10% and quantity 1–3, the same `N` expects 100 successes and 200 units. At 100% and quantity 2, `N = 100` expects 100 successes and 200 units. At 0%, both values stay 0.

The line is analytic. Hover and the numeric `N` field snap to an integer kill and evaluate the formula there. Changing the graph does not save the monster. Unsaved row edits are labeled as a draft and update the line immediately.

## Future runtime simulator

Not implemented. When it exists, it must call the production Rust loot evaluator through a narrow development adapter. Sampler analysis, without minting items or writing the world or database, is labeled sampler analysis and does not prove the death hook. A headless integration mode drives eligible deaths through plan and manifestation in an isolated world and counts opportunities, successes, manifested units, and allocation errors separately.

A 10% row does not have to drop on exactly 100 of 1,000 kills. The simulator compares the observed success rate with the expectation using a statistical interval, including a 95% interval for the Bernoulli rate. One result outside that interval is a warning, not a failing build. Correctness tests stay deterministic. Distribution checks are predeclared diagnostics and do not fail because one seed moved.
