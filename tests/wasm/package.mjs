// What the Node tests of the WebAssembly package share: the built package, the native extension's answers
// (oracle.py writes them) and the comparison of two arrays of numbers.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

/** The package built for Node (`wasm-bindgen --target nodejs`). */
export const { Patch, things } = require("../../pkg/node/bullterrier_control_wasm.js");

/** What the native extension answered: `times`, `cases` (patch, bundle, nodes, signals), `things`, `failures`. */
export const native = JSON.parse(readFileSync(new URL("../../pkg/oracle.json", import.meta.url), "utf-8"));

/** The index of the first element where `actual` is not `expected` bit for bit (-0 is not 0), or -1 if they are the
 *  same and as long. Compared by index, so a mismatch never builds a diff of the whole array. */
export function firstDifference(expected, actual) {
  if (expected.length !== actual.length) {
    return Math.min(expected.length, actual.length);
  }
  return expected.findIndex((value, at) => !Object.is(value, actual[at]));
}

/** The `Float64Array` a list of numbers is, with `null` as NaN (the oracle's JSON cannot hold NaN). */
export const floats = (numbers) => Float64Array.from(numbers, (number) => number ?? Number.NaN);
