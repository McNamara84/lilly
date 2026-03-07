---
description: "Use when processing code review feedback, evaluating PR comments, or deciding whether to apply suggested changes. Covers review triage and decision-making."
---
# Code Review Guidelines

## Evaluate Before Implementing

When points are raised in a code review, **do not blindly apply all suggestions**. For each piece of feedback:

1. **Assess validity**: Is the criticism technically correct?
2. **Check context**: Does the reviewer understand the full context of the change?
3. **Weigh trade-offs**: Does the suggested change improve correctness, security, or performance — or is it purely stylistic?
4. **Consider scope**: Is the suggestion within scope of the current PR, or does it belong in a separate change?

## Priority Order

When review points conflict, prioritize in this order:

1. **Correctness** — Does the code produce the right result?
2. **Security** — Does it follow OWASP guidelines and project security standards?
3. **Performance** — Is it efficient enough for the use case?
4. **Readability** — Can another developer understand it?
5. **Style** — Does it follow conventions?

## When to Push Back

- The suggestion introduces unnecessary complexity.
- The suggestion is based on a misunderstanding of the requirements.
- The change would break existing tests or behavior without justification.
- The suggestion is purely cosmetic and adds review churn without value.

## When to Accept

- The reviewer identified a genuine bug or security issue.
- The suggestion makes error handling more robust.
- The change improves test coverage for uncovered paths.
- The suggestion aligns code with documented project conventions.
