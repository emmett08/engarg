"""Synthetic dependency/API smoke check, not an engineering evidence collector."""
import json
from importlib.metadata import version

import numpy as np
import pandas as pd
from causallearn.search.ConstraintBased.PC import pc
from dowhy import CausalModel

rng = np.random.default_rng(17)
samples = 256
confounder = rng.normal(size=samples)
treatment = 0.5 * confounder + rng.normal(size=samples)
outcome = 2.0 * treatment + confounder + rng.normal(scale=0.1, size=samples)
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
print(json.dumps({
    "scope": "synthetic software API smoke check only",
    "dowhy": version("dowhy"),
    "causal_learn": version("causal-learn"),
    "estimate": float(estimate.value),
    "refuted_estimate": float(refutation.new_effect),
    "discovered_graph_shape": list(discovered.G.graph.shape),
    "causal_assumptions_validated_for_external_data": False,
}))
