"""Finite-model collector; qualification references frozen before implementation.

Independent exhaustive-state rational expectations (posterior, sensor mass):
  selected both sensors: (152/201, 201/10000)
  heartbeat not lost:   (38/479, 479/10000)
  different parameters:(6/7, 7/25)
Their uniform 15-significant-digit representations are respectively:
  (0.756218905472637, 0.0201)
  (0.0793319415448852, 0.0479)
  (0.857142857142857, 0.28)
Boolean probabilities, missing/extra keys, out-of-range/nonfinite numbers,
and zero evidence mass must fail. These references are documentation only;
qualification and independent execution are the host's responsibility.
"""

import contextlib
from datetime import datetime, timezone
from decimal import Decimal
from importlib.metadata import version
import json
import math
import sys


LIMIT = 1048576


class NumberToken(str):
    """A validated JSON numeric lexeme, never an opaque-object convention."""


def fail(message):
    raise ValueError(message)


def reject_constant(value):
    fail("Nonfinite JSON constant")


def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            fail("Duplicate object key")
        result[key] = value
    return result


def encode(value):
    """Serialize metadata without converting its numeric tokens to floats."""
    if isinstance(value, NumberToken):
        return str(value)
    if value is None:
        return "null"
    if type(value) is bool:
        return "true" if value else "false"
    if type(value) is str:
        return json.dumps(value, ensure_ascii=True)
    if type(value) in (float, int):
        return json.dumps(value, allow_nan=False)
    if type(value) is list:
        return "[" + ",".join(encode(item) for item in value) + "]"
    if type(value) is dict:
        return "{" + ",".join(
            json.dumps(key, ensure_ascii=True) + ":" + encode(item)
            for key, item in value.items()
        ) + "}"
    fail("Unsupported JSON value")


def exact_keys(value, keys):
    if type(value) is not dict or set(value) != set(keys):
        fail("Invalid object shape")


def probability(value):
    if not isinstance(value, NumberToken):
        fail("Probability must be a JSON number, not a boolean")
    exact = Decimal(str(value))
    if not exact.is_finite() or not Decimal(0) <= exact <= Decimal(1):
        fail("Probability outside finite unit interval")
    converted = float(exact)
    if not math.isfinite(converted) or not 0 <= converted <= 1:
        fail("Probability not representable by the numerical backend")
    return converted


def validate(request):
    exact_keys(request, (
        "evidence_id", "environment", "tool", "tool_version", "input", "context"
    ))
    for key in ("evidence_id", "environment", "tool", "tool_version"):
        if type(request[key]) is not str:
            fail("Invalid request string")
    if not request["evidence_id"]:
        fail("Empty evidence identity")
    if type(request["context"]) is not dict:
        fail("Context must be an object")
    supplied = request["input"]
    exact_keys(supplied, ("model", "evidence"))
    names = (
        "p_fault", "p_alarm_given_fault", "p_alarm_given_healthy",
        "p_loss_given_fault", "p_loss_given_healthy"
    )
    exact_keys(supplied["model"], names)
    parameters = tuple(probability(supplied["model"][name]) for name in names)
    exact_keys(supplied["evidence"], ("alarm", "heartbeat_lost"))
    if any(type(flag) is not bool for flag in supplied["evidence"].values()):
        fail("Evidence must contain booleans")
    return parameters, supplied["evidence"]


def finite_probability(value, positive=False):
    if not math.isfinite(value) or not 0 <= value <= 1:
        fail("Invalid inferred probability")
    if positive and value <= 0:
        fail("Zero evidence mass")


def calculate(parameters, evidence):
    # This is an abductive finite-network query using the prescribed public APIs.
    if version("pgmpy") != "1.1.2":
        fail("The prepared pgmpy 1.1.2 dependency is required")
    from pgmpy import config
    config.set_backend("numpy")
    from pgmpy.models import DiscreteBayesianNetwork
    from pgmpy.factors.discrete import TabularCPD
    from pgmpy.inference import VariableElimination

    p, a1, a0, h1, h0 = parameters
    network = DiscreteBayesianNetwork([
        ("Fault", "Alarm"), ("Fault", "HeartbeatLost")
    ])
    fault = TabularCPD(
        variable="Fault", variable_card=2, values=[[1 - p], [p]],
        state_names={"Fault": [0, 1]}
    )
    alarm = TabularCPD(
        variable="Alarm", variable_card=2,
        values=[[1 - a0, 1 - a1], [a0, a1]],
        evidence=["Fault"], evidence_card=[2],
        state_names={"Alarm": [0, 1], "Fault": [0, 1]}
    )
    heartbeat = TabularCPD(
        variable="HeartbeatLost", variable_card=2,
        values=[[1 - h0, 1 - h1], [h0, h1]],
        evidence=["Fault"], evidence_card=[2],
        state_names={"HeartbeatLost": [0, 1], "Fault": [0, 1]}
    )
    network.add_cpds(fault, alarm, heartbeat)
    if not network.check_model():
        fail("Model check failed")
    inference = VariableElimination(network)
    sensors = inference.query(
        variables=["Alarm", "HeartbeatLost"], joint=True, show_progress=False
    )
    selected_alarm = int(evidence["alarm"])
    selected_loss = int(evidence["heartbeat_lost"])
    raw_mass = float(sensors.get_value(
        Alarm=selected_alarm, HeartbeatLost=selected_loss
    ))
    finite_probability(raw_mass, positive=True)
    posterior = inference.query(
        variables=["Fault"],
        evidence={"Alarm": selected_alarm, "HeartbeatLost": selected_loss},
        joint=True, show_progress=False
    )
    raw_healthy = float(posterior.get_value(Fault=0))
    raw_fault = float(posterior.get_value(Fault=1))
    finite_probability(raw_healthy)
    finite_probability(raw_fault)
    if abs(raw_healthy + raw_fault - 1) > 1e-12:
        fail("Posterior normalization failed")

    canonical_fault = float(format(raw_fault, ".15g"))
    canonical_mass = float(format(raw_mass, ".15g"))
    finite_probability(canonical_fault)
    finite_probability(canonical_mass, positive=True)
    return {
        "posterior_fault": canonical_fault,
        "evidence_normalizer": canonical_mass,
        "calculation_complete": True,
        "collected": True
    }


def main():
    raw = sys.stdin.buffer.read(LIMIT + 1)
    if len(raw) > LIMIT:
        fail("Request exceeds bounded input size")
    request = json.loads(
        raw.decode("utf-8"), parse_int=NumberToken, parse_float=NumberToken,
        parse_constant=reject_constant, object_pairs_hook=object_pairs
    )
    parameters, evidence = validate(request)
    # Suppress any dependency chatter on the protocol channel.
    with contextlib.redirect_stdout(sys.stderr):
        value = calculate(parameters, evidence)
    observed_at = datetime.now(timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z")
    response = {
        "value": value,
        "observed_at": observed_at,
        "context": request["context"],
        "request": {
            "tool": request["tool"],
            "tool_version": request["tool_version"],
            "input": request["input"],
            "context": request["context"]
        }
    }
    output = (encode(response) + "\n").encode("ascii")
    if len(output) > LIMIT:
        fail("Response exceeds bounded output size")
    sys.stdout.buffer.write(output)
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("Acquisition failed: " + str(error), file=sys.stderr)
        sys.exit(1)
