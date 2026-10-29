# Domain docs

## Before exploring

This repository uses a single-context layout:

- Read `GLOSSARY.md` at the repository root when it exists.
- Read decisions in `docs/adr/` relevant to the area being explored.

If these files are absent, proceed silently. The domain-modeling skill
creates them when terms or decisions are resolved.

## Vocabulary

Use glossary terms in issue titles, proposals, hypotheses and test names.
Respect any synonyms the glossary explicitly avoids.

When a needed concept is missing, check whether it belongs to the domain.
Record a real vocabulary gap for domain-modeling.

## Decisions

Surface conflicts with existing ADRs explicitly. Identify the ADR and
explain why its decision may need to be revisited.

## Layout

- `GLOSSARY.md`: shared domain vocabulary.
- `docs/adr/NNNN-descriptive-title.md`: architecture decisions.

Create substantive documentation as needed; setup requires no empty
glossary or placeholder ADRs.
