#!/usr/bin/env python3
"""Render the recorded argument examples without provider calls or tool execution."""
import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "examples/assess-claim"
CASES = (
    ("Bayesian diagnosis", "bayesian-diagnosis", "recorded/authoring-request.json",
     "revision/argument.json", "recorded/positive/execution.json"),
    ("Qualified Python dependency metadata", "python-dependencies/qualified",
     "../request.json", "revision/argument.json", "positive/assessment.json"),
    ("Historical Python dependency metadata", "python-dependencies/recorded",
     "../request.json", "argument.json", "positive/assessment.json"),
)


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def link(base, target, text):
    relative = os.path.relpath(target, base).replace(os.sep, "/")
    return f'<a href="{html.escape(relative, quote=True)}">{html.escape(text)}</a>'


def build_pages(engarg):
    pages, rows = {}, []
    with tempfile.TemporaryDirectory(prefix="engarg-example-html-") as scratch:
        for number, (title, directory, request_name, argument_name, assessment_name) in enumerate(CASES):
            base = EXAMPLES / directory
            request_path, argument_path = base / request_name, base / argument_name
            assessment_path = base / assessment_name
            request, argument = load(request_path), load(argument_path)
            recorded = load(assessment_path)
            assessment = recorded.get("result", {}).get("assessment", recorded)
            claim = lambda name: assessment["claims"]["claim_" + name.encode().hex()]["status"]
            current, stronger = claim("current_claim"), claim("stronger_target")
            stamp = assessment["assessed_at"]
            authored = argument["status"]
            report = Path(scratch) / f"report-{number}.json"
            # Rendering admits these public source objects; it does not re-assess them.
            report.write_text(json.dumps({"request": request, "argument": argument}), encoding="utf-8")
            result = subprocess.run(
                [engarg, "render", "--input", str(report), "--format", "html"],
                check=True, capture_output=True,
            )
            page = result.stdout.decode("utf-8")
            page = page.replace("<title>Engineering argument</title>",
                                f"<title>{html.escape(title)} — argument</title>", 1)
            page = page.replace("Current assessment: ", "Authored argument assessment: ", 1)
            heading = "<h1>Engineering argument</h1>"
            if page.count(heading) != 1:
                raise SystemExit("Renderer heading changed; review the presentation context insertion")
            source_links = " · ".join((
                link(base, argument_path, "Authored argument JSON"),
                link(base, request_path, "Original request"),
                link(base, assessment_path, "Recorded EAL assessment"),
                link(base, base / "result.json", "Run summary"),
                link(base, EXAMPLES / "index.html", "All recorded arguments"),
            ))
            note = (
                f'<aside class="assessment" aria-labelledby="recorded-result">'
                '<h2 id="recorded-result">Recorded EAL assessment</h2>'
                f'<p>Current claim: <b>{html.escape(current)}</b>. '
                f'Stronger target: <b>{html.escape(stronger)}</b>. '
                f'Valid: {str(assessment["valid"]).lower()}; '
                f'complete: {str(assessment["complete"]).lower()}.</p>'
                f'<p>Recorded assessment time: <time datetime="{html.escape(stamp, quote=True)}">'
                f'{html.escape(stamp)}</time>.</p>'
                '<p>The argument below preserves its authored statements, status '
                f'(<b>{html.escape(authored)}</b>), qualifications and open challenges. '
                'The dated EAL result above reports what the recorded evidence supported. '
                'Rendering performs no new collection or assessment and preserves original clocks. '
                'A later execution can have a different result when evidence expires.</p>'
                f'<p><a href="#argument-view">Jump to argument</a> · {source_links}</p>'
                '<details><summary>Presentation source identities</summary>'
                f'<p>Argument SHA-256: <code>{sha(argument_path)}</code></p>'
                f'<p>Request SHA-256: <code>{sha(request_path)}</code></p>'
                f'<p>Recorded assessment SHA-256: <code>{sha(assessment_path)}</code></p>'
                '</details></aside>'
            )
            page = page.replace(heading, f"<h1>{html.escape(title)}</h1>\n{note}", 1)
            pages[base / "argument.html"] = page.encode("utf-8")
            rows.append(
                f'<tr><th scope="row">{link(EXAMPLES, base / "argument.html", title)}</th>'
                f'<td>{html.escape(authored)}</td><td>{html.escape(current)}</td>'
                f'<td>{html.escape(stronger)}</td><td>{html.escape(stamp)}</td></tr>'
            )
    index = '''<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<title>Recorded argument examples</title>
<style>
body{margin:0;background:#f2f5f9;color:#1c2939;font:16px/1.65 system-ui,sans-serif}
main{max-width:75rem;margin:auto;padding:2rem 1.25rem}a{color:#174c8c}
a:focus-visible{outline:3px solid #174c8c;outline-offset:4px}
.table-scroll{overflow-x:auto}table{border-collapse:collapse;width:100%;background:white}
th,td{border:1px solid #ccd6e2;padding:.8rem;text-align:left;vertical-align:top}
thead th{background:#e2ebf7}caption{text-align:left;margin-bottom:.6rem}
code{overflow-wrap:anywhere}p{max-width:85ch}
</style></head><body><main><h1>Recorded argument examples</h1>
<p>Open an argument to inspect its supporting routes, qualifications, evidence
needs and challenges. Native disclosure controls expand the detail; hovering over
argument parts shows glossary definitions, with accessible links to the full entries.</p>
<p>Authored status and the dated EAL assessment are shown separately. These static
presentations preserve the original evidence and review clocks. They perform no new
collection or assessment; replay can produce different results after expiry.</p>
<div class="table-scroll" role="region" aria-label="Recorded argument results" tabindex="0">
<table><caption>Recorded positive-input assessments</caption><thead><tr>
<th scope="col">Argument</th><th scope="col">Authored status</th>
<th scope="col">EAL current claim</th><th scope="col">EAL stronger target</th>
<th scope="col">Recorded assessment time (UTC)</th></tr></thead><tbody>
'''
    index += "\n".join(rows)
    index += '''</tbody></table></div>
<p>The historical metadata argument records the earlier unsupported stage. Its
qualified successor and the Bayesian diagnosis retain their scoped AI review and
trusted-runtime conditions; supported results apply to those declared boundaries.</p>
<p>For a synthetic example, open <a href="../demo.html">the CI argument demonstration</a>.
See <a href="../../docs/PRESENTATION_API.md">presentation documentation</a> for regeneration.</p>
</main></body></html>
'''
    pages[EXAMPLES / "index.html"] = index.encode("utf-8")
    return pages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engarg", default="engarg", help="Existing EngArg CLI executable")
    parser.add_argument("--check", action="store_true", help="Compare generated files without writing")
    args = parser.parse_args()
    engarg = shutil.which(args.engarg)
    if engarg is None:
        parser.error("EngArg executable not found; build the CLI and pass --engarg /path/to/engarg")
    pages = build_pages(engarg)
    manifest_path = EXAMPLES / "bayesian-diagnosis/SHA256SUMS.json"
    manifest = load(manifest_path)
    # Only presentation files may change their hashes; protect recorded artifacts.
    for name, expected in manifest.items():
        if name not in ("argument.html", "README.md") and sha(manifest_path.parent / name) != expected:
            raise SystemExit(f"Recorded artifact checksum mismatch: {name}")
    manifest["README.md"] = sha(manifest_path.parent / "README.md")
    manifest["argument.html"] = hashlib.sha256(pages[manifest_path.parent / "argument.html"]).hexdigest()
    pages[manifest_path] = (json.dumps(dict(sorted(manifest.items())), indent=2) + "\n").encode("utf-8")
    stale = []
    for path, content in pages.items():
        if args.check:
            if not path.is_file() or path.read_bytes() != content:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.write_bytes(content)
    if stale:
        raise SystemExit("Stale presentation files: " + ", ".join(stale))
    print(f'{"Checked" if args.check else "Generated"} 3 argument presentations, index and checksums')


if __name__ == "__main__":
    main()
