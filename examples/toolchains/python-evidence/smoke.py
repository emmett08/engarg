"""Synthetic offline dependency/API checks, not an engineering evidence collector."""

import json
import math
from importlib.metadata import version
from importlib.util import find_spec

import networkx as nx
import numpy as np
import pandas as pd
import sympy
import z3
from causallearn.search.ConstraintBased.PC import pc
from dowhy import CausalModel, gcm
from pgmpy import config
from pgmpy.factors.discrete import TabularCPD
from pgmpy.inference import VariableElimination
from pgmpy.models import DiscreteBayesianNetwork
from scipy.optimize import linprog
from scipy.stats import binomtest
from sklearn.linear_model import LinearRegression
from sklearn.neighbors import NearestNeighbors
from statsmodels.stats.proportion import proportion_confint


def causal_check():
    rng = np.random.default_rng(17)
    confounder = rng.normal(size=256)
    treatment = 0.5 * confounder + rng.normal(size=256)
    outcome = 2.0 * treatment + confounder + rng.normal(scale=0.1, size=256)
    data = pd.DataFrame({"X": confounder, "T": treatment, "Y": outcome})
    model = CausalModel(
        data=data,
        treatment="T",
        outcome="Y",
        graph="digraph { X -> T; X -> Y; T -> Y; }",
    )
    estimand = model.identify_effect(proceed_when_unidentifiable=False)
    estimate = model.estimate_effect(estimand, method_name="backdoor.linear_regression")
    refutation = model.refute_estimate(
        estimand,
        estimate,
        method_name="random_common_cause",
        num_simulations=2,
        random_state=17,
    )
    discovered = pc(data.to_numpy(), alpha=0.05, indep_test="fisherz", show_progress=False)
    assert np.isfinite(estimate.value) and abs(estimate.value - 2.0) < 0.1
    assert np.isfinite(refutation.new_effect)
    assert discovered.G.graph.shape == (3, 3)
    return {
        "estimate": float(estimate.value),
        "refuted_estimate": float(refutation.new_effect),
        "discovered_graph_shape": list(discovered.G.graph.shape),
    }


def posterior_check():
    config.set_backend("numpy")
    assert find_spec("torch") is None
    model = DiscreteBayesianNetwork([("H", "E")])
    model.add_cpds(
        TabularCPD("H", 2, [[0.8], [0.2]]),
        TabularCPD("E", 2, [[0.9, 0.1], [0.1, 0.9]], evidence=["H"], evidence_card=[2]),
    )
    assert model.check_model()
    inference = VariableElimination(model)
    evidence_mass = float(inference.query(["E"], show_progress=False).get_value(E=1))
    assert math.isfinite(evidence_mass) and evidence_mass > 0
    posterior = inference.query(["H"], evidence={"E": 1}, show_progress=False)
    probability = float(posterior.get_value(H=1))
    assert math.isclose(evidence_mass, 0.26, abs_tol=1e-12)
    assert math.isclose(probability, 9.0 / 13.0, abs_tol=1e-12)
    assert inference.map_query(["H"], evidence={"E": 1}, show_progress=False) == {"H": 1}

    # An impossible observation must be rejected before posterior normalisation.
    impossible = DiscreteBayesianNetwork([("H", "E")])
    impossible.add_cpds(
        TabularCPD("H", 2, [[0.8], [0.2]]),
        TabularCPD("E", 2, [[1.0, 1.0], [0.0, 0.0]], evidence=["H"], evidence_card=[2]),
    )
    assert impossible.check_model()
    zero_mass = float(
        VariableElimination(impossible).query(["E"], show_progress=False).get_value(E=1)
    )
    try:
        if not math.isfinite(zero_mass) or zero_mass <= 0:
            raise ValueError("observation has zero or nonfinite model probability")
    except ValueError:
        zero_mass_rejected = True
    else:
        raise AssertionError("impossible observation was admitted")
    return {"posterior_H1": probability, "zero_mass_rejected": zero_mass_rejected}


def counterfactual_check():
    # Authored equations: X = U_X; Y = 2*X + U_Y. No coefficient is estimated.
    model = gcm.InvertibleStructuralCausalModel(nx.DiGraph([("X", "Y")]))
    model.set_causal_mechanism("X", gcm.EmpiricalDistribution())
    model.set_causal_mechanism(
        "Y", gcm.AdditiveNoiseModel(gcm.ml.create_linear_regressor_with_given_parameters([2.0]))
    )
    gcm.config.disable_progress_bars()
    gcm.fit(model, pd.DataFrame({"X": [1.0, 2.0, 3.0], "Y": [3.0, 7.0, 8.0]}))
    factual = pd.DataFrame({"X": [2.0], "Y": [7.0]})
    noise = pd.DataFrame({"X": [2.0], "Y": [3.0]})
    intervention = {"X": lambda _: 5.0}
    reconstructed = gcm.counterfactual_samples(model, intervention, observed_data=factual)
    supplied = gcm.counterfactual_samples(model, intervention, noise_data=noise)
    assert np.allclose(reconstructed[["X", "Y"]], [[5.0, 13.0]], atol=1e-12)
    assert np.allclose(reconstructed[["X", "Y"]], supplied[["X", "Y"]], atol=1e-12)
    assert float(reconstructed.iloc[0]["Y"] - 2.0 * reconstructed.iloc[0]["X"]) == 3.0
    return {"factual_Y": 7.0, "counterfactual_Y": 13.0, "same_exogenous_Y": 3.0}


def inductive_check():
    low, high = proportion_confint(8, 10, alpha=0.05, method="wilson")
    assert math.isclose(float(low), 0.4901624715366417, abs_tol=1e-12)
    assert math.isclose(float(high), 0.9433178485456247, abs_tol=1e-12)
    test = binomtest(8, 10, p=0.5)
    assert math.isclose(test.pvalue, 0.109375, abs_tol=1e-12)
    train_x = np.array([[0.0], [1.0], [2.0], [3.0]])
    predictor = LinearRegression().fit(train_x, [1.0, 3.0, 5.0, 7.0])
    held_out = predictor.predict([[4.0], [5.0]])
    assert np.allclose(held_out, [9.0, 11.0], atol=1e-12)
    return {"synthetic_successes": 8, "synthetic_trials": 10, "wilson_95": [low, high]}


def formal_check():
    value = z3.Int("value")
    solver = z3.Solver()
    solver.add(value >= 0, value < 2)
    assert solver.check() == z3.sat
    witness = solver.model()[value].as_long()
    assert 0 <= witness < 2
    solver.add(value == 3)
    assert solver.check() == z3.unsat
    x = sympy.Symbol("x", real=True)
    assert set(sympy.solve(x**2 - 1, x)) == {-1, 1}
    return {"satisfiable_witness": witness, "contradiction": "unsat", "symbolic_roots": [-1, 1]}


def similarity_and_optimisation_check():
    neighbours = NearestNeighbors(n_neighbors=1, algorithm="brute").fit([[0.0, 0.0], [3.0, 4.0]])
    distance, index = neighbours.kneighbors([[2.9, 4.0]])
    assert int(index[0, 0]) == 1 and math.isclose(float(distance[0, 0]), 0.1, abs_tol=1e-12)
    solution = linprog([1.0, 1.0], A_ub=[[-1.0, -2.0]], b_ub=[-4.0], bounds=[(0, 3), (0, 3)])
    assert solution.success and math.isclose(float(solution.fun), 2.0, abs_tol=1e-12)
    assert np.allclose(solution.x, [0.0, 2.0], atol=1e-12)
    return {"nearest_index": 1, "linear_program_objective": float(solution.fun)}


if __name__ == "__main__":
    print(json.dumps({
        "scope": "synthetic software API smoke checks only",
        "versions": {name: version(name) for name in [
            "dowhy", "causal-learn", "pgmpy", "z3-solver", "scipy", "statsmodels", "scikit-learn", "sympy"
        ]},
        "causal": causal_check(),
        "abductive": posterior_check(),
        "counterfactual": counterfactual_check(),
        "inductive": inductive_check(),
        "formal": formal_check(),
        "similarity_and_optimisation": similarity_and_optimisation_check(),
        "external_model_assumptions_validated": False,
    }))
    print("evidence-packages-ok")
