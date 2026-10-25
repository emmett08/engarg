use super::beam::{OptionCost, Search};

fn option(capability: usize, source: usize, cost: u64) -> OptionCost {
    OptionCost {
        capability,
        source,
        maximum_calls: 1,
        exact_source_count: true,
        estimates: vec![Some(cost)],
    }
}
#[test]
fn compact_backpointers_preserve_global_sharing_and_lexical_ties() {
    let mut search = Search::new(vec![false]);
    // The higher lexical candidate wins through reuse on the next row.
    search.advance(&[option(0, 0, 1), option(2, 2, 1)]);
    search.advance(&[option(1, 1, 1), option(2, 2, 1)]);
    search.advance(&[]);
    let selected = search.finish();
    assert_eq!(selected.choices, vec![Some(2), Some(2), None]);
    assert_eq!(selected.costs, vec![(0, 1)]);
    assert_eq!(selected.exact_source_count, Some(1));
    assert_eq!(selected.maximum_calls, 2);
    assert!(selected.complete);
    let mut search = Search::new(vec![false]);
    search.advance(&[option(2, 0, 1), option(0, 1, 1)]);
    search.advance(&[option(2, 2, 1), option(1, 3, 1)]);
    assert_eq!(search.finish().choices, vec![Some(0), Some(1)]);
}
#[test]
fn compact_search_agrees_with_exhaustive_small_plan_definition() {
    // Independent finite definition: one cost per distinct physical group and
    // one configuration cost per capability, with lexicographic path ties.
    let rows = [
        vec![(0, 0, 4), (1, 1, 1), (2, 2, 2)],
        vec![(0, 0, 4), (1, 3, 1), (2, 2, 2)],
        vec![(0, 4, 4), (1, 3, 1), (2, 2, 2)],
    ];
    let mut expected = Vec::new();
    for a in &rows[0] {
        for b in &rows[1] {
            for c in &rows[2] {
                let choices = [a, b, c];
                let mut sources = std::collections::BTreeSet::new();
                let mut capabilities = std::collections::BTreeSet::new();
                let mut calls = 0u128;
                let mut setup = 0u128;
                for (capability, source, cost) in choices {
                    if sources.insert(*source) {
                        calls += u128::from(*cost);
                    }
                    if capabilities.insert(*capability) {
                        setup += 10;
                    }
                }
                expected.push((
                    vec![(0, calls), (0, setup)],
                    choices
                        .iter()
                        .map(|value| Some(value.0))
                        .collect::<Vec<_>>(),
                ));
            }
        }
    }
    expected.sort();
    let mut search = Search::new(vec![false, true]);
    for row in rows {
        search.advance(
            &row.into_iter()
                .map(|(capability, source, cost)| OptionCost {
                    capability,
                    source,
                    maximum_calls: 1,
                    exact_source_count: true,
                    estimates: vec![Some(cost), Some(10)],
                })
                .collect::<Vec<_>>(),
        );
    }
    let result = search.finish();
    assert!(result.complete);
    assert_eq!((result.costs, result.choices), expected.remove(0));
}
#[test]
fn fanout_execution_is_unknown_but_setup_and_call_bound_remain_explicit() {
    let mut search = Search::new(vec![true, false]);
    let options = [OptionCost {
        capability: 0,
        source: 0,
        maximum_calls: 1000,
        exact_source_count: false,
        estimates: vec![Some(5), None],
    }];
    search.advance(&options);
    search.advance(&options);
    let result = search.finish();
    assert_eq!(result.costs, vec![(0, 5), (1, 0)]);
    assert_eq!(result.exact_source_count, None);
    assert_eq!(result.maximum_calls, 2000);
    assert_eq!(result.dynamic_sources, 1);
}
