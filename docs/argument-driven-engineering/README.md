# Argument-driven engineering

Article template and presentation prepared on 26 October 2026.

| File | Contents |
| --- | --- |
| [argument-driven-engineering-article.pdf](argument-driven-engineering-article.pdf) | Six-page IEEE-style article with a method proposal and prospective evaluation. |
| [argument-driven-engineering-latex.zip](argument-driven-engineering-latex.zip) | Editable LaTeX source, BibTeX references, TikZ workflow figure and IEEEtran support files. |
| [argument-driven-engineering-presentation.pptx](argument-driven-engineering-presentation.pptx) | 21-slide presentation with editable diagrams, speaker notes and references. |

The article and presentation connect evidence-based engineering and the software development lifecycle to a proposed process in which arguments guide AI-assisted engineering work.

## Build the article

Extract the source archive and run the following command from its `argument-driven-engineering` directory.

```sh
latexmk -pdf -interaction=nonstopmode -halt-on-error main.tex
```

A TeX distribution with TikZ and `latexmk` is required.

## Prepare for submission

Select the intended IEEE publication and apply its author instructions. Replace the affiliation and contact placeholder. The evaluation is prospective. Add empirical results after conducting and analysing the study, then recheck the cited repository revisions.
