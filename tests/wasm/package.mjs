// What the Node tests of the WebAssembly package share: the built package, the native extension's answers
// (oracle.py writes them) and the comparison of two arrays of numbers.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

/** The package built for Node (`wasm-bindgen --target nodejs`). */
export const { Patch, things } = require("../../pkg/node/bullterrier_control_wasm.js");

/** What the native extension answered: `times`, `cases` (patch, bundle, nodes, signals), `things`, `failures`. */
export const native = JSON.parse(readFileSync(new URL("../../pkg/oracle.json", import.meta.url), "utf-8"));

/** How far apart two values of one signal may be, relative to max(1, |value|). The same number as `TOLERANCE` in
 *  `crates/core/tests/it/common/compare.rs`, where the reason is given. */
export const TOLERANCE = 1e-9;

const close = (expected, actual) =>
  Object.is(expected, actual) ||
  expected === actual ||
  (Number.isFinite(expected) &&
    Number.isFinite(actual) &&
    Math.abs(expected - actual) <= TOLERANCE * Math.max(1, Math.abs(expected), Math.abs(actual)));

/** The index of the first element where `actual` is not `expected` within `TOLERANCE` (NaN matches NaN), or -1 if they
 *  agree and are as long. Compared by index, so a mismatch never builds a diff of the whole array. */
export function firstDifference(expected, actual) {
  if (expected.length !== actual.length) {
    return Math.min(expected.length, actual.length);
  }
  return expected.findIndex((value, at) => !close(value, actual[at]));
}

/** The `Float64Array` a list of numbers is, with `null` as NaN (the oracle's JSON cannot hold NaN). */
export const floats = (numbers) => Float64Array.from(numbers, (number) => number ?? Number.NaN);
