// This probe checks prepared offline APIs, not the validity of an external model.
package main

import (
	"encoding/json"
	"fmt"
	"math"
	"net/http"

	"github.com/causalgo/causalgo/scic"
	"github.com/causalgo/causalgo/surd"
	"github.com/causalgo/causalgo/varselect"
	"github.com/crillab/gophersat/solver"
	"gonum.org/v1/gonum/mat"
	"gonum.org/v1/gonum/optimize"
	"gonum.org/v1/gonum/spatial/kdtree"
	"gonum.org/v1/gonum/stat"
	"gonum.org/v1/gonum/stat/distuv"
)

func require(ok bool, message string) {
	if !ok {
		panic(message)
	}
}

func finite(value float64) bool {
	return !math.IsNaN(value) && !math.IsInf(value, 0)
}

func statistics() {
	require(stat.Mean([]float64{1, 2, 3}, nil) == 2, "statistical mean")
	require(math.Abs((distuv.Binomial{N: 2, P: 0.5}).Prob(1)-0.5) < 1e-12, "binomial probability mass")
	require(math.Abs((distuv.Normal{Mu: 0, Sigma: 1}).CDF(0)-0.5) < 1e-12, "normal cumulative probability")
}

func linearAlgebra() {
	a := mat.NewDense(2, 2, []float64{2, 0, 0, 4})
	b := mat.NewVecDense(2, []float64{6, 8})
	var x mat.VecDense
	err := x.SolveVec(a, b)
	require(err == nil && math.Abs(x.AtVec(0)-3) < 1e-12 && math.Abs(x.AtVec(1)-2) < 1e-12, "bounded linear solve")
}

func nearestNeighbour() {
	tree := kdtree.New(kdtree.Points{{0, 0}, {2, 2}}, false)
	point, squaredDistance := tree.Nearest(kdtree.Point{0.1, 0.1})
	require(point.(kdtree.Point)[0] == 0 && math.Abs(squaredDistance-0.02) < 1e-12, "nearest neighbour")
}

func optimisation() {
	problem := optimize.Problem{Func: func(x []float64) float64 {
		return (x[0] - 2) * (x[0] - 2)
	}}
	result, err := optimize.Minimize(problem, []float64{0}, nil, &optimize.NelderMead{})
	require(err == nil && !result.Status.Early() && finite(result.F) && result.F < 1e-10 && math.Abs(result.X[0]-2) < 1e-5, "pure Go optimisation")
}

func propositionalSAT() {
	satisfiable := solver.New(solver.ParseSlice([][]int{{1, 2}, {-1, 2}}))
	require(satisfiable.Solve() == solver.Sat && satisfiable.Model()[1], "SAT witness")
	contradiction := solver.New(solver.ParseSlice([][]int{{1}, {-1}}))
	require(contradiction.Solve() == solver.Unsat, "UNSAT outcome")
}

func causalAPIs() {
	data := make([][]float64, 32)
	target := make([]float64, 32)
	predictor := make([]float64, 32)
	matrix := mat.NewDense(32, 3, nil)
	for i := range data {
		x, y := float64(i%2), float64((i/2)%2)
		data[i] = []float64{float64((i % 2) ^ ((i / 2) % 2)), x, y}
		target[i], predictor[i] = float64(i)*2, float64(i)
		matrix.Set(i, 0, float64(i))
		matrix.Set(i, 1, float64(i)*2+x)
		matrix.Set(i, 2, float64(i)*3+y)
	}
	decomposition, err := surd.DecomposeFromData(data, []int{2, 2, 2})
	require(err == nil && finite(decomposition.InfoLeak) && len(decomposition.MutualInfo) > 0, "SURD decomposition")
	config := scic.DefaultConfig()
	config.Bins = []int{4, 4}
	config.MinSamplesPerQuartile = 2
	direction, err := scic.Decompose(target, [][]float64{predictor}, config)
	require(err == nil && finite(direction.Directions["0"]) && direction.Directions["0"] > 0, "SCIC direction calculation")
	selection, err := varselect.New(varselect.Config{Workers: 1, MaxIter: 100}).Fit(matrix)
	require(err == nil && len(selection.Order) == 3 && len(selection.Adjacency) == 3, "VarSelect ordering")
}

func main() {
	_ = http.Client{}
	statistics()
	linearAlgebra()
	nearestNeighbour()
	optimisation()
	propositionalSAT()
	causalAPIs()
	encoded, err := json.Marshal(true)
	require(err == nil, "JSON encoding")
	require(string(encoded) == "true", "JSON round trip")
	fmt.Println("evidence-packages-ok")
}
