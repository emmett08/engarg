//! Compile with `rustc --edition 2024 -O benchmarks/selection_profile.rs`.
//! Isolates beam expansion/sorting; excludes contracts, schemas and providers.
#[path = "../src/route_planning/selection/beam.rs"]
mod beam;
use std::{collections::BTreeSet, time::Instant};

#[derive(Clone, Default)]
struct PreviousState {
    choices: Vec<Option<String>>,
    sources: BTreeSet<String>,
    capabilities: BTreeSet<String>,
    costs: Vec<(usize, u128)>,
}
fn previous(rows: usize, options: usize) -> (Vec<usize>, Vec<(usize, u128)>, usize, bool) {
    let mut frontier = vec![PreviousState {
        costs: vec![(0, 0); 3],
        ..Default::default()
    }];
    let mut complete = true;
    for row in 0..rows {
        let choices = (0..options)
            .map(|index| {
                (
                    format!("capability-{index:04}"),
                    format!("{row:032x}{index:032x}"),
                )
            })
            .collect::<Vec<_>>();
        let mut next = Vec::new();
        complete &= choices.len() <= 64;
        for state in &frontier {
            for (capability, source) in choices.iter().take(64) {
                let mut candidate = state.clone();
                candidate.choices.push(Some(capability.clone()));
                let new_source = candidate.sources.insert(source.clone());
                let new_capability = candidate.capabilities.insert(capability.clone());
                for dimension in 0..3 {
                    if if dimension == 0 {
                        new_capability
                    } else {
                        new_source
                    } {
                        candidate.costs[dimension].1 += 1;
                    }
                }
                next.push(candidate);
            }
        }
        next.sort_by(|a, b| {
            a.costs
                .cmp(&b.costs)
                .then_with(|| a.choices.cmp(&b.choices))
        });
        complete &= next.len() <= 32;
        next.truncate(32);
        frontier = next;
    }
    let best = frontier.remove(0);
    (
        best.choices
            .into_iter()
            .map(|choice| choice.unwrap()[11..].parse().unwrap())
            .collect(),
        best.costs,
        best.sources.len(),
        complete,
    )
}
fn current(rows: usize, options: usize) -> (Vec<usize>, Vec<(usize, u128)>, usize, bool) {
    let mut search = beam::Search::new(vec![true, false, false]);
    for row in 0..rows {
        search.advance(
            &(0..options)
                .map(|index| beam::OptionCost {
                    capability: index,
                    source: row * options + index,
                    maximum_calls: 1,
                    exact_source_count: true,
                    estimates: vec![Some(1); 3],
                })
                .collect::<Vec<_>>(),
        );
    }
    let best = search.finish();
    std::hint::black_box((best.maximum_calls, best.dynamic_sources, best.considered));
    (
        best.choices.into_iter().map(Option::unwrap).collect(),
        best.costs,
        best.exact_source_count.unwrap(),
        best.complete,
    )
}
fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    let mode = arguments.get(1).map(String::as_str).unwrap_or("current");
    let rows = arguments
        .get(2)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(256);
    let options = arguments
        .get(3)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(64);
    assert!(rows > 0 && options > 0);
    let start = Instant::now();
    let (choices, costs, sources, complete) = match mode {
        "previous" => previous(rows, options),
        "current" => current(rows, options),
        _ => panic!("mode must be previous or current"),
    };
    // Both implementations must retain the same lexical incumbent and costs.
    assert!(choices.iter().all(|choice| *choice == 0));
    assert_eq!(costs, vec![(0, 1), (0, rows as u128), (0, rows as u128)]);
    assert_eq!(sources, rows);
    assert_eq!(
        complete,
        (0..rows)
            .try_fold(1usize, |count, _| count.checked_mul(options))
            .is_some_and(|count| count <= 32)
    );
    println!(
        "mode={mode} rows={rows} options={options} elapsed_ms={:.3} incumbent=all_zero costs={costs:?} sources={sources} search_complete={complete}",
        start.elapsed().as_secs_f64() * 1000.0
    );
}
