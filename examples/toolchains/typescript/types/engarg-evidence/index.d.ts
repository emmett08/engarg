// Narrow declarations for the pinned, untyped packages exercised by the image
// smoke tests. Other packages retain their upstream declarations. Extend these
// interfaces with an actual API control when an operation needs more surface.
declare module "logic-solver" {
  namespace Logic {
    interface Formula {
      readonly __logicFormula: unique symbol;
    }
    type Term = string | number | Formula;
    type Operand = Term | readonly Operand[];
    class Solver {
      require(...formulas: Operand[]): void;
      forbid(...formulas: Operand[]): void;
      solve(): Solution | null;
      solveAssuming(formula: Term): Solution | null;
    }
    class Solution {
      private constructor();
      getTrueVars(): string[];
      getMap(): Record<string, boolean>;
      evaluate(formula: Term): boolean;
    }
    function not(formula: Term): Term;
    function or(...formulas: Operand[]): Term;
    function and(...formulas: Operand[]): Term;
    function xor(...formulas: Operand[]): Term;
    function atMostOne(...formulas: Operand[]): Term;
    function exactlyOne(...formulas: Operand[]): Term;
    function implies(left: Term, right: Term): Term;
    function equiv(left: Term, right: Term): Term;
  }
  export = Logic;
}

declare module "libdagitty/src/index.js" {
  namespace Dagitty {
    interface Graph {
      readonly __dagittyGraph: unique symbol;
    }
    const GraphParser: {
      parseGuess(source: string): Graph;
      parseDot(source: string): Graph;
    };
    const GraphAnalyzer: {
      isAdjustmentSet(graph: Graph, adjustment?: string[], selection?: string[]): boolean;
    };
  }
  export = Dagitty;
}

// ml-distance reexports this untyped transitive dependency from its otherwise
// typed ESM similarity namespace. Keep strict checking for the whole package.
declare module "ml-tree-similarity" {
  export type Spectrum = [number[], number[]];
  export interface Tree {
    sum: number;
    center: number;
    left: Tree | null;
    right: Tree | null;
  }
  export interface TreeOptions {
    minWindow?: number;
    threshold?: number;
    from?: number;
    to?: number;
  }
  export interface SimilarityOptions {
    alpha?: number;
    beta?: number;
    gamma?: number;
  }
  export function createTree(spectrum: Spectrum, options?: TreeOptions): Tree | null;
  export function treeSimilarity(
    left: Tree | Spectrum | null,
    right: Tree | Spectrum | null,
    options?: SimilarityOptions,
  ): number;
  export function getFunction(options?: SimilarityOptions): (
    left: Tree | Spectrum | null,
    right: Tree | Spectrum | null,
  ) => number;
}
