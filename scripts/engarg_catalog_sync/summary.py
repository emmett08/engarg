"""Assemble retained authoring reports and print their terminal summaries."""

import json
import pathlib
import sys

from .readiness import build_readiness


def validate_host_reports(directory):
    """Require successful host operations and a valid exported argument."""
    for filename in ['eal-preflight.json','eal-validation.json','evidence-plan.json']:
        result = json.loads((directory/filename).read_text())
        if result.get('is_error') is not False:
            raise SystemExit('EAL host reported a failed operation: '+filename)
        if filename == 'eal-validation.json':
            validation = result.get('result')
            if not isinstance(validation, dict) or validation.get('valid') is not True:
                raise SystemExit('Exported EAL failed validation or returned an unsupported validation envelope: '+filename)


def retained_tooling(value):
    """Find tooling within the existing bound of 17 retained history entries."""
    for _ in range(17):
        tooling = value.get('tooling')
        if isinstance(tooling, dict):
            return tooling
        provenance = value.get('replanning')
        if not isinstance(provenance, dict):
            return None
        value = provenance.get('previous_extensions', {})
    raise SystemExit('Tooling provenance exceeds its retained history bound')


def common_summary(directory, report, identity, completed_execution):
    """Assemble fields shared by graph and flat planning reports."""
    return {
        'evidence_collected': completed_execution['evidence_collected'] if completed_execution else False,
        'eal_execution': completed_execution,
        'argument_status': report['argument']['status'],
        'recorded_model_calls': len(report.get('transcript', [])),
        'model_calls_this_run': 0 if identity['analysis_reuse'] else len(report.get('transcript', [])),
        'analysis_reuse': identity['analysis_reuse'],
        'presentations': {name: str(directory / filename) for name, filename in {
            'json': 'analysis.json', 'markdown': 'argument.md', 'text': 'argument.txt',
            'html': 'argument.html', 'prose': 'prose.md'
        }.items()},
    }


def validated_graph_export(directory, report, bundle):
    """Require matching argument, support contracts and execution-plan identity."""
    planning = report.get('evidence_planning')
    try:
        exported = json.loads((directory/'argument.evidence-plan.json').read_text())
    except (OSError, ValueError):
        raise SystemExit('Graph evidence plan is missing or invalid: argument.evidence-plan.json')
    if (not isinstance(planning, dict) or planning.get('schema') != 'engarg.support-planning/1'
            or bundle.get('schema_version') != 'engarg.support-export/1'
            or exported.get('schema') != 'engarg.support-evidence-plan/1'
            or exported.get('case_digest') != bundle.get('authored', {}).get('revision')
            or exported.get('execution_plan_sha256') != bundle.get('execution_plan_sha256')
            or exported.get('support_elaboration') != planning.get('elaboration')
            or exported.get('routes') != planning.get('routes')):
        raise SystemExit('Graph evidence planning must retain the exact argument, requirements and execution-plan identity')
    return planning, exported


def graph_summary(report, identity, completed_execution, summary_common, readiness):
    """Describe graph planning and retained tool implementation results."""
    design_status = readiness['acquisition_design']['status']
    selection_status = readiness['capability_selection']['status']
    source_status = readiness['source_collection']['status']
    summary = {
        **summary_common,
        'completed_phase': ('retained argument route replanning and deterministic export validation' if identity['analysis_reuse']
                            else 'argument authoring, independent support extraction, qualified route planning and deterministic export validation'),
        'evidence_planning': 'graph', 'graph_readiness': readiness,
        'stage_status': {'argument_validation': 'passed', 'artifact_generation': 'passed',
                         'acquisition_design': design_status, 'capability_selection': selection_status,
                         'source_collection': source_status,
                         'evidence_assessment': completed_execution['status'] if completed_execution else 'not_performed'},
        'next_step': readiness['next_step'],
        'tool_implementation': {'enabled': identity['tool_implementation']['enabled']},
    }
    summary['stage_status']['tool_implementation'] = 'not_requested'
    if identity['tool_implementation']['enabled']:
        tooling = retained_tooling(report)
        if not isinstance(tooling, dict) or tooling.get('schema') != 'engarg.capability-tooling/1':
            raise SystemExit('Graph implementation was enabled but the analysis has no capability tooling report')
        summary['stage_status']['tool_implementation'] = (
            'function_contracts_verified' if tooling['capabilities'] else
            'deferred' if tooling['deferred_implementation_gaps'] else 'no_eligible_implementation')
        summary['tool_implementation'].update(
            language=tooling['language'], model_calls=tooling['model_calls'],
            verified_packages=len(tooling['capabilities']),
            reused_packages=sum(package['reused'] for package in tooling['capabilities']),
            skipped_non_implementation_gaps=tooling['skipped_non_implementation_gaps'],
            deferred_implementation_gaps=tooling['deferred_implementation_gaps'],
            audit_path=tooling['audit_path'], note=tooling['note'])
    return summary


def flat_summary(directory, report, bundle, identity, summary_common):
    """Describe flat collector preparation and retained implementation results."""
    plan = bundle.get('acquisition_plan') or {}
    entries = plan.get('entries', [])
    summary = {
        **summary_common,
        'completed_phase': 'argument authoring and deterministic export validation',
        'selected_collectors': sorted({entry['tool_id'] for entry in entries}),
        'unresolved_arguments': [{'need_id': entry['need_id'], 'arguments': entry['unresolved_arguments']}
            for entry in entries if entry['unresolved_arguments']],
        'pending_review_obligations': len(bundle.get('obligations', [])),
        'next_step': 'Review exact-query compatibility and operator bindings before explicit collection and assessment.'
    }
    preparation_path = directory/'collector-inputs/collector-preparation.json'
    try:
        preparation = json.loads(preparation_path.read_text())
    except (OSError, ValueError):
        raise SystemExit('Current-case collector preparation report is missing or invalid: '+str(preparation_path))
    if (not isinstance(preparation, dict)
            or preparation.get('schema') != 'engarg.collector-preparation/1'
            or not isinstance(bundle.get('revision'), str)
            or len(bundle['revision']) != 64
            or any(character not in '0123456789abcdef' for character in bundle['revision'])
            or preparation.get('case_digest') != bundle.get('revision')
            or preparation.get('evidence_collected') is not False):
        raise SystemExit('Collector preparation must describe this exact export without claiming collection')
    summary['collector_preparation'] = preparation
    summary['next_step'] = (
        'Resolve each reported configuration or evidence gap before explicit collection and independent assessment.'
    )
    implementation = identity['tool_implementation']
    summary['tool_implementation'] = {'enabled': implementation['enabled']}
    if implementation['enabled']:
        tooling = report.get('tooling')
        if not isinstance(tooling, dict) or tooling.get('schema') != 'engarg.tooling/1':
            raise SystemExit('Implementation was enabled but the analysis has no tooling report')
        summary['tool_implementation'].update(
            language=tooling['language'], model_calls=tooling['model_calls'],
            verified_packages=len(tooling['packages']),
            reused_packages=sum(package['reused'] for package in tooling['packages']),
            audit_path=tooling['audit_path'], catalogue_path=str(directory/'tooling/collectors.json'))
        if tooling.get('coverage') is not None:
            summary['tool_implementation']['coverage'] = tooling['coverage']
    return summary


def main(argv):
    """Write summary artefacts for the retained run directory."""
    directory = pathlib.Path(argv[0])
    validate_host_reports(directory)
    report = json.loads((directory/'analysis.json').read_text())
    bundle = json.loads((directory/'argument.bundle.json').read_text())
    identity = json.loads((directory/'input-identity.json').read_text())
    execution_path = directory/'execution-result.json'
    completed_execution = json.loads(execution_path.read_text()) if execution_path.exists() else None
    summary_common = common_summary(directory, report, identity, completed_execution)
    if identity['evidence_planning']['mode'] == 'graph':
        planning, exported = validated_graph_export(directory, report, bundle)
        description = json.loads((directory/'eal-preflight.json').read_text())['result']
        readiness = build_readiness(planning, exported, description)
        summary = graph_summary(report, identity, completed_execution, summary_common, readiness)
        (directory/'graph-readiness.json').write_text(json.dumps(readiness, indent=2)+'\n')
    else:
        summary = flat_summary(directory, report, bundle, identity, summary_common)
    (directory/'run-summary.json').write_text(json.dumps(summary, indent=2)+'\n')


def execution_status(argv):
    """Print the retained execution outcome."""
    result = json.loads(pathlib.Path(argv[0]).read_text())
    print('EAL execution: %s; valid: %s; complete: %s; current claim: %s. Full collection and assessment are retained.' % (
        result['status'], result['valid'], result['complete'], result['claim_status']))
    if result['status'] != 'supported':
        raise SystemExit(3)


def graph_status(argv):
    """Print the retained graph outcome."""
    with open(argv[0], encoding='utf-8') as source:
        readiness = json.load(source)
    print('Graph readiness: %s requirements; %s authored logical acquisitions; %s compiled acquisition routes; %s typed gaps; ready for collection: %s. Evidence truth and inferential soundness remain unverified.' % (
        readiness['requirements'], readiness['authored_logical_acquisitions'], readiness['compiled_acquisition_routes'], len(readiness['gaps']), readiness['ready_for_collection']))
    print('Acquisition design: %s; capability selection: %s; source collection: %s; independent reviews pending: %s.' % (
        readiness['acquisition_design']['status'], readiness['capability_selection']['status'],
        readiness['source_collection']['status'], readiness['independent_review_obligations']))
    if readiness['acquisition_design']['status'] in {'pending_repair', 'pending_design'}:
        print('engarg: warning: acquisition planning is incomplete. Retained designs and completed model calls are available for targeted repair; inspect analysis.json and graph-readiness.json.', file=sys.stderr)

    budget = readiness['collection_admission']
    print('Collection budget for all emitted evidence: %s/%s; source: %s; assumed limit: %s.' % (
        budget['evidence_count'], budget['limit'], budget['limit_source'], budget['limit_is_assumed']))


def preparation_status(argv):
    """Print the retained preparation outcome."""
    with open(argv[0], encoding='utf-8') as source:
        preparation = json.load(source)
    summary = preparation['summary']
    print('Collector preparation: %s bound; %s unresolved; %s retained-artifact inputs verified. Evidence acquisition remains a separate step.' % (
        summary['bound'], summary['unresolved'], summary['retained_artifacts_verified']))
