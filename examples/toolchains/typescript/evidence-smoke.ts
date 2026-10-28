// Strict TypeScript API controls also exercise Node24 loading the ESM distance
// package from a compiled CommonJS collector. They are synthetic package checks.
import assert from "node:assert/strict";
import { add, lusolve } from "mathjs";
import { mean, sampleVariance } from "simple-statistics";
import Logic from "logic-solver";
import { distance, similarity } from "ml-distance";
import solver from "javascript-lp-solver";
import dagitty from "libdagitty/src/index.js";

assert.equal(add(2, 3), 5);
assert.deepEqual(lusolve([[2]], [8]), [[4]]);
assert.equal(mean([1, 2, 3]), 2);
assert.equal(sampleVariance([1, 2, 3]), 1);
assert.equal(distance.euclidean([0, 0], [3, 4]), 5);
const tree = similarity.tree.createTree([[0, 1, 2], [0, 1, 0]]);
assert.ok(tree);
assert.equal(tree.sum, 1);
assert.equal(tree.center, 1);
assert.equal(tree.left, null);
assert.equal(tree.right, null);
assert.equal(similarity.tree.treeSimilarity(tree, tree), 0.33);
assert.equal(similarity.tree.getFunction({ beta: 1 })(tree, tree), 1);
assert.equal(similarity.tree.createTree([[0, 1], [1, 1]], { minWindow: 2 }), null);

const constraints = new Logic.Solver();
constraints.require(Logic.exactlyOne("a", "b"));
constraints.forbid("b");
const solution = constraints.solve();
assert.ok(solution);
assert.deepEqual(solution.getTrueVars(), ["a"]);
assert.equal(solution.evaluate(Logic.and("a", Logic.not("b"))), true);
assert.equal(solution.evaluate(Logic.or(["a", "b"])), true);
assert.equal(solution.evaluate(Logic.xor("a", "b")), true);
assert.equal(solution.evaluate(Logic.atMostOne("a", "b")), true);
assert.equal(solution.evaluate(Logic.exactlyOne("a", "b")), true);
assert.equal(solution.evaluate(Logic.implies("a", Logic.not("b"))), true);
assert.equal(solution.evaluate(Logic.equiv("a", Logic.not("b"))), true);
assert.deepEqual(solution.getMap(), { a: true, b: false });
assert.equal(constraints.solveAssuming("b"), null);
constraints.require("b");
assert.equal(constraints.solve(), null);

const optimum = solver.Solve({
  optimize: "value", opType: "max",
  constraints: { capacity: { max: 2 } },
  variables: { x: { capacity: 1, value: 3 } },
});
// The upstream Solve signature returns unknown. Check the result before using
// its fields rather than discarding strict checking or asserting a broad type.
assert.ok(typeof optimum === "object" && optimum !== null
  && "feasible" in optimum && "bounded" in optimum && "result" in optimum && "x" in optimum);
assert.equal(optimum.feasible, true);
assert.equal(optimum.bounded, true);
assert.equal(optimum.result, 6);
assert.equal(optimum.x, 2);
const infeasible = solver.Solve({
  optimize: "value", opType: "max",
  constraints: { capacity: { max: 2, min: 3 } },
  variables: { x: { capacity: 1, value: 3 } },
});
assert.ok(typeof infeasible === "object" && infeasible !== null && "feasible" in infeasible);
assert.equal(infeasible.feasible, false);

const graph = dagitty.GraphParser.parseGuess(
  "dag { X [exposure]; Y [outcome]; Z -> X; Z -> Y; X -> Y }",
);
assert.equal(dagitty.GraphAnalyzer.isAdjustmentSet(graph, ["Z"]), true);
assert.equal(dagitty.GraphAnalyzer.isAdjustmentSet(graph, []), false);
const parsed = dagitty.GraphParser.parseDot("dag { X [exposure]; Y [outcome]; X -> Y }");
assert.equal(dagitty.GraphAnalyzer.isAdjustmentSet(parsed, []), true);

console.log(JSON.stringify({ kind: "synthetic_prepared_package_api_smoke", passed: true,
  marker: "evidence-packages-ok",
  packages: ["mathjs", "simple-statistics", "logic-solver", "ml-distance",
    "javascript-lp-solver", "libdagitty/src/index.js"] }));
