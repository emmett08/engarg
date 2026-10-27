# A historical proposal for argument-driven engineering

[Documentation](../index.md) · [Current evaluation design](../EVALUATION.md)

These research artefacts were prepared on 26 October 2026. They propose a process
in which a versioned engineering case guides work towards justified decisions,
using a hypothetical retry-safe submission feature as the example. Their
comparative benefits remain hypotheses awaiting the proposed study.

The article describes repository foundations at its cited revisions. Its account
of registered assessment and future integration work is historical. Use the
[current engineer workflow](../USAGE.md) and [architecture](../EXECUTABLE_ARCHITECTURE.md)
for the implemented EAL/4 package and execution contracts. The article and slides
remain unchanged so their original claims can be read against their citations.

| Retained artefact | Purpose |
|---|---|
| [Article](argument-driven-engineering-article.pdf) | Six-page IEEE-style method proposal and prospective evaluation. |
| [LaTeX source](argument-driven-engineering-latex.zip) | Editable source, bibliography, workflow figure and IEEEtran support. |
| [Presentation](argument-driven-engineering-presentation.pptx) | 21-slide presentation with diagrams, speaker notes and references. |

To reproduce the article, extract the source archive and run this command from
its `argument-driven-engineering` directory with a TeX distribution containing
TikZ and `latexmk`:

```sh
latexmk -pdf -interaction=nonstopmode -halt-on-error main.tex
```

A new submission needs the intended publication's template, actual author details,
an updated account of the implementation and independently reviewed empirical
results. Preserve the distinction between the proposed method and measured
benefit when preparing that revision.
