"""Validate collection and assessment results for the selected current claim."""

import json
import pathlib


def collection_id(report_path):
    """Print the successful collection identifier for the assessment command."""
    result = json.loads(pathlib.Path(report_path).read_text())
    identifier = result.get('result', {}).get('collection_id')
    if result.get('is_error') is not False or not isinstance(identifier, str) or not identifier:
        raise SystemExit('EAL collection did not return a collection identifier. Inspect evidence-collection.json.')
    print(identifier)


def write_result(run_directory):
    """Retain formal assessment and independent catalogue observation checks."""
    directory = pathlib.Path(run_directory)
    bundle = json.loads((directory/'argument.bundle.json').read_text())
    collection = json.loads((directory/'evidence-collection.json').read_text())
    envelope = json.loads((directory/'eal-assessment.json').read_text())
    assessment = envelope.get('result', {})
    claim_name = bundle['claim_names']['current_claim']
    claim = assessment.get('claims', {}).get(claim_name)
    if envelope.get('is_error') is not False or not isinstance(claim, dict):
        raise SystemExit('EAL assessment did not return the current claim. Inspect eal-assessment.json.')
    valid = assessment.get('valid') is True
    complete = assessment.get('complete') is True
    supported = valid and complete and claim.get('status') == 'supported'
    reference_check = None
    if json.loads((directory/'input-identity.json').read_text()).get('simple_example'):
        from catalogue_evidence_example import verify_observations
        try:
            reference_check = verify_observations(directory)
        except (OSError, KeyError, ValueError, TypeError) as error:
            reference_check = {'passed': False, 'reason': str(error)}
            supported = False
    execution = {
        'schema': 'engarg.catalog-sync-execution/1',
        'status': 'supported' if supported else 'unresolved',
        'valid': valid, 'complete': complete, 'claim_status': claim.get('status'),
        'claim_name': claim_name, 'claim': claim,
        'collection_id': collection['result']['collection_id'],
        'assessment_id': assessment.get('assessment_id'),
        'independent_reference_check': reference_check,
        'evidence_collected': any(record.get('status') == 'ok' for record in collection['result'].get('records', {}).values()),
        'measured_negative_evidence': [name for name, record in collection['result'].get('records', {}).items()
                                       if (record.get('value') or {}).get('criterion_satisfied') is False],
        'unavailable_evidence': [name for name, record in collection['result'].get('records', {}).items()
                                 if record.get('status') != 'ok'],
        'scope': 'The declared formal method, evidence bindings, assumptions and selected observation set. This outcome does not establish arbitrary prose or external facts beyond those contracts.',
        'artifacts': {'collection': str(directory/'evidence-collection.json'), 'assessment': str(directory/'eal-assessment.json')},
    }
    (directory/'execution-result.json').write_text(json.dumps(execution, indent=2)+'\n')
