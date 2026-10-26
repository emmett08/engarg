"""Evaluate graph source availability, acquisition design and collection bounds."""


def acquisition_design(planning, requirements, authored_acquisitions, gaps):
    """Classify the retained acquisition design without changing its obligations."""
    design_review = planning.get('design_review')
    if not isinstance(design_review, dict):
        design_review = None
    pending_designs = design_review.get('pending_designs', []) if design_review else []
    empirical_gap_kinds = {'source_unavailable', 'historical_unavailable', 'clarification',
                          'operation_implementation', 'transformation_implementation'}
    if design_review and design_review.get('status') == 'pending_repair':
        design_status = 'pending_repair'
    elif authored_acquisitions:
        design_status = 'represented'
    elif any(gap.get('kind') in empirical_gap_kinds for gap in gaps):
        design_status = 'pending_design'
    elif requirements:
        design_status = 'judgement_only'
    else:
        design_status = 'no_acquisition_design'
    return design_status, design_review, pending_designs


def ready_acquisition_ids(routes):
    """Resolve source and derivation dependencies, leaving cycles and unknowns blocked."""
    selected_routes = [route for route in routes['routes'] if route.get('selected')]
    ready_routes = [route for route in selected_routes if not route.get('configuration_gaps')]
    execution = routes.get('execution', {})
    steps = {step['id']: step for step in execution.get('acquisitions', [])}
    derivations = {step['id']: step for step in execution.get('derivations', [])}
    ready_ids = {route['acquisition_id'] for route in ready_routes}
    dependencies = {}
    for step_id, step in steps.items():
        references = [binding.get('output', {}) for binding in step.get('bindings', [])]
        fanout = step.get('for_each')
        if fanout:
            references.append(fanout.get('output', {}))
            witness = fanout.get('stop_on_witness') or {}
            if witness.get('equals_output'):
                references.append(witness['equals_output'])
        dependencies[step_id] = {reference.get('step_id') for reference in references}
    for step_id, step in derivations.items():
        dependencies[step_id] = {reference.get('step_id') for reference in step.get('inputs', [])}
    # Resolve source and derivation availability without recursive traversal.
    # An unknown reference or a cycle remains blocked. A downstream source needs
    # every source behind data, discovery and witness-control dependencies.
    available = set()
    nodes = set(dependencies) | ready_ids
    while True:
        before = len(available)
        for step_id in nodes - available:
            if step_id not in derivations and step_id not in ready_ids:
                continue
            if dependencies.get(step_id, set()) <= available:
                available.add(step_id)
        if len(available) == before:
            break
    ready_ids &= available
    if routes['discovery']['incomplete']:
        ready_ids.clear()
    return ready_ids


def admit_collection(budget, description):
    """Select the advertised or explicitly assumed collection limit."""
    if (not isinstance(budget, dict) or budget.get('schema') != 'engarg.collection-budget/1'
            or type(budget.get('assumed_default_limit')) is not int or budget['assumed_default_limit'] < 1
            or type(budget.get('total_emitted_evidence')) is not int or budget['total_emitted_evidence'] < 0
            or not isinstance(budget.get('claim_evidence_counts'), dict)
            or any(type(count) is not int or count < 0 for count in budget['claim_evidence_counts'].values())):
        raise SystemExit('Graph evidence plan requires valid collection-budget counts and an explicit default assumption')
    selected_limit = budget['assumed_default_limit']
    limit_source = 'exported.collection_budget.assumed_default_limit'
    limit_is_assumed = True
    # The current EAL service publishes execution_limits. Accept budgets as a
    # compatibility alias, while retaining the exact source used.
    for field in ['execution_limits', 'budgets']:
        advertised = description.get(field)
        if isinstance(advertised, dict) and 'collection_evidence' in advertised:
            selected_limit = advertised['collection_evidence']
            if type(selected_limit) is not int or selected_limit < 1:
                raise SystemExit('EAL host collection_evidence budget must be a positive integer')
            limit_source = 'eal-preflight.result.' + field + '.collection_evidence'
            limit_is_assumed = False
            break
    collection_admission = {
        'selection': 'all_emitted_evidence',
        'evidence_count': budget['total_emitted_evidence'],
        'limit': selected_limit, 'limit_source': limit_source,
        'limit_is_assumed': limit_is_assumed,
        'within_limit': budget['total_emitted_evidence'] <= selected_limit,
    }
    return collection_admission


def build_readiness(planning, exported, description):
    """Build the versioned readiness report from a validated graph export."""
    routes = planning['routes']
    requirements = planning['elaboration']['requirements']
    acquisitions = routes['routes']
    authored_acquisitions = planning['elaboration'].get('acquisitions', [])
    gaps = routes['gaps']
    design_status, design_review, pending_designs = acquisition_design(planning, requirements, authored_acquisitions, gaps)
    # Preserve logical_acquisitions as the existing alias for compiled routes.
    selected_routes = [route for route in acquisitions if route.get('selected')]
    ready_ids = ready_acquisition_ids(routes)
    selection_status = ('not_requested' if not acquisitions else
                        'complete' if len(selected_routes) == len(acquisitions) else
                        'partial' if selected_routes else 'unavailable')
    source_status = ('ready' if acquisitions and len(ready_ids) == len(acquisitions) else
                     'partial' if ready_ids else 'blocked' if acquisitions else 'no_acquisitions')
    budget = exported.get('collection_budget')
    collection_admission = admit_collection(budget, description)
    readiness = {
        'schema': 'engarg.graph-readiness/1',
        'case_digest': exported['case_digest'],
        'execution_plan_sha256': exported['execution_plan_sha256'],
        'requirements': len(requirements),
        'logical_acquisitions': len(acquisitions),
        'logical_acquisitions_meaning': 'compatibility alias for compiled_acquisition_routes',
        'authored_logical_acquisitions': len(authored_acquisitions),
        'compiled_acquisition_routes': len(acquisitions),
        'acquisition_design': {'status': design_status, 'review': design_review,
                               'pending_designs': len(pending_designs)},
        'capability_selection': {'status': selection_status,
                                 'selected_routes': len(selected_routes)},
        'source_collection': {'status': source_status,
                              'ready_acquisition_ids': sorted(ready_ids),
                              'blocked_acquisition_ids': sorted({route['acquisition_id'] for route in acquisitions} - ready_ids),
                              'scope': 'configured compiled source routes and their dependencies; independent judgement remains pending'},
        'selected_capabilities': sorted({route['selected'] for route in acquisitions if route.get('selected')}),
        'routes': acquisitions, 'gaps': gaps, 'pending': exported['pending'],
        'collection_budget': budget, 'collection_admission': collection_admission,
        'discovery_incomplete': routes['discovery']['incomplete'],
        'ready_for_collection': (collection_admission['within_limit'] and source_status == 'ready'
                                 and design_status not in {'pending_repair', 'pending_design'}
                                 and not any(gap.get('kind') != 'judgement' for gap in gaps)),
        'evidence_collected': False, 'evidence_truth_verified': False, 'inferential_soundness_verified': False,
        'independent_review_obligations': len(exported['independent_review_obligations']),
        'next_step': 'Resolve typed configuration, availability, qualification and clarification gaps before explicit collection; independent judgement remains separate.'
    }
    if design_status in {'pending_repair', 'pending_design'}:
        readiness['next_step'] = ('Repair the retained acquisition design before route replanning. '
            'Review pending designs and the saved transcript; completed authoring calls remain retained.')
    if not collection_admission['within_limit']:
        budget_next = ('Default collect-all selects %s emitted evidence declarations against limit %s from %s. '
            'Review the selected host budget or explicitly select a bounded claim before collection; the complete graph and independent reviews remain retained.' % (
                budget['total_emitted_evidence'], collection_admission['limit'], collection_admission['limit_source']))
        readiness['next_step'] = ((readiness['next_step'] + ' ' + budget_next)
            if design_status in {'pending_repair', 'pending_design'} else budget_next)
    return readiness
