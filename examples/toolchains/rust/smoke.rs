//! Offline API checks for the prepared image; no external evidence is inferred.
use good_lp::{Solution, SolverModel, default_solver, variable, variables};
use linfa_nn::{CommonNearestNeighbour, NearestNeighbour, distance::L2Dist};
use nalgebra::{DMatrix, DVector};
use ndarray::{arr1, array};
use statrs::distribution::{Binomial, Discrete};
use varisat::{ExtendFormula, Solver};

#[derive(serde::Deserialize, serde::Serialize)]
struct Probe {
    ok: bool,
}

fn main() {
    let _client = reqwest::blocking::Client::new();
    assert_eq!(
        serde_json::to_string(&Probe { ok: true }).unwrap(),
        "{\"ok\":true}"
    );
    let raw: Box<serde_json::value::RawValue> =
        serde_json::from_str("{\"$serde_json::private::Number\":\"literal\"}").unwrap();
    assert_eq!(raw.get(), "{\"$serde_json::private::Number\":\"literal\"}");

    let dag = why_rs::dag::DAG::new().node("X").node("Y").edge("X", "Y");
    assert_eq!(dag.get_parents(&"Y".to_owned()), vec!["X"]);
    assert_eq!(dag.sort(), vec!["X", "Y"]);
    let table = polars::df!("X" => &[0.0, 1.0, 2.0, 3.0], "Y" => &[1.0, 3.0, 5.0, 7.0]).unwrap();
    let discovery = why_rs::pc::PC::new(vec!["X".to_owned(), "Y".to_owned()], table);
    assert_eq!(discovery.graph.node_count(), 2);
    assert_eq!(discovery.graph.edge_count(), 2);

    let binomial = Binomial::new(0.5, 2).unwrap();
    assert!((binomial.pmf(1) - 0.5).abs() < 1e-12);
    let matrix = DMatrix::from_row_slice(2, 2, &[1.0, 0.0, -2.0, 1.0]);
    let solved = matrix
        .lu()
        .solve(&DVector::from_row_slice(&[3.0, 1.0]))
        .unwrap();
    assert!((solved[1] - 7.0_f64).abs() < 1e-12);

    let mut sat = Solver::new();
    let literal = sat.new_lit();
    sat.add_clause(&[literal]);
    assert!(sat.solve().unwrap());
    sat.assume(&[!literal]);
    assert!(!sat.solve().unwrap());

    let points = array![[0.0, 0.0], [3.0, 4.0]];
    let neighbours = CommonNearestNeighbour::KdTree
        .from_batch(&points, L2Dist)
        .unwrap();
    let query = arr1(&[2.9, 4.0]);
    assert_eq!(neighbours.k_nearest(query.view(), 1).unwrap()[0].1, 1);

    let mut variables = variables!();
    let x = variables.add(variable().min(0.0).max(3.0));
    let optimum = variables.maximise(x).using(default_solver).solve().unwrap();
    assert!((optimum.value(x) - 3.0).abs() < 1e-12);
    println!("evidence-packages-ok");
}
