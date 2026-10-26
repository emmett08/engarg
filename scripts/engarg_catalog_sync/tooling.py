"""Admit typed implementation gaps and preserve authored analysis during enrichment."""

import json
import pathlib


def select_gaps(analysis_path, selection_path):
    """Write eligible implementation gaps after complete capability discovery."""
    report = json.loads(pathlib.Path(analysis_path).read_text())
    planning = report.get('evidence_planning')
    routes = planning.get('routes') if isinstance(planning, dict) else None
    if (not isinstance(planning, dict) or planning.get('schema') != 'engarg.support-planning/1'
            or not isinstance(routes, dict) or routes.get('schema') != 'engarg.evidence-routes/1'
            or not isinstance(routes.get('gaps'), list)
            or not isinstance(routes.get('discovery'), dict)
            or not isinstance(routes['discovery'].get('incomplete'), bool)):
        raise SystemExit('Tool implementation requires the typed graph route plan from analysis.')
    kinds = {'operation_implementation', 'transformation_implementation', 'configuration',
             'source_unavailable', 'historical_unavailable', 'qualification', 'capability_mismatch',
             'clarification', 'judgement', 'discovery_incomplete'}
    for gap in routes['gaps']:
        if (not isinstance(gap, dict) or not isinstance(gap.get('kind'), str) or gap['kind'] not in kinds
                or gap.get('implementation') is not None and not isinstance(gap['implementation'], dict)):
            raise SystemExit('Tool implementation route gaps have an invalid type or request.')
    incomplete = routes['discovery']['incomplete'] or any(gap['kind'] == 'discovery_incomplete' for gap in routes['gaps'])
    eligible = [] if incomplete else [
        {key: gap.get(key) for key in ['acquisition_id', 'requirement_id', 'kind']}
        for gap in routes['gaps'] if gap['kind'] in {'operation_implementation', 'transformation_implementation'}
        and isinstance(gap.get('implementation'), dict)
    ]
    pathlib.Path(selection_path).write_text(json.dumps({
        'schema': 'engarg.tooling-selection/1', 'discovery_incomplete': incomplete,
        'eligible_operation_gaps': eligible, 'eligible_count': len(eligible),
        'note': 'Configuration, credentials, historical availability, qualification and judgement do not admit implementation.'
    }, indent=2) + '\n')


def merge_analysis(original_path, enriched_path, final_path):
    """Accept only added tooling metadata, preserving exact JSON number tokens."""
    original, enriched, final = map(pathlib.Path, (original_path, enriched_path, final_path))
    def exact(path):
        # A tuple distinguishes number tokens from all JSON arrays/objects/strings.
        return json.loads(path.read_text(), parse_int=lambda value: ('number', value),
                          parse_float=lambda value: ('number', value))
    expected, actual = exact(original), exact(enriched)
    tooling = actual.pop('tooling', None)
    if (actual != expected or not isinstance(tooling, dict)
            or tooling.get('schema') != 'engarg.capability-tooling/1'):
        raise SystemExit('Tool implementation changed the original analysis or omitted capability metadata.')
    with final.open('xb') as output:
        output.write(enriched.read_bytes())


def extract_case(run_directory):
    """Retain the argument only when its request matches the admitted request."""
    directory = pathlib.Path(run_directory)
    report = json.loads((directory/'analysis.json').read_text())
    expected = json.loads((directory/'request.json').read_text())
    if report['request'] != expected:
        raise SystemExit('Saved analysis changed the supplied request')
    (directory/'case.json').write_text(json.dumps(report['argument'], indent=2)+'\n')
