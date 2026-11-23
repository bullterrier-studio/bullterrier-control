// The comparison of two arrays of values (`firstDifference` in package.mjs), proved on known-good and known-bad input.
import assert from "node:assert/strict";
import { test } from "node:test";

import { TOLERANCE, firstDifference } from "./package.mjs";

test("values inside the tolerance agree and one just over it does not", () => {
  assert.equal(firstDifference([0, 1, 2], [0, 1 + TOLERANCE * 0.5, 2]), -1);
  assert.equal(firstDifference([0, 1, 2], [0, 1, 3 + TOLERANCE * 6]), 2);
  assert.equal(firstDifference([0, 1], [0, 1 + TOLERANCE * 2]), 1);
});

test("a large value is compared relative to its size", () => {
  assert.equal(firstDifference([1e6], [1e6 + 1e-4]), -1);
  assert.equal(firstDifference([1e6], [1e6 + 1e-2]), 0);
});

test("NaN agrees with NaN only, infinity with itself only, and the signs of zero agree", () => {
  assert.equal(firstDifference([NaN], [NaN]), -1);
  assert.equal(firstDifference([NaN], [0]), 0);
  assert.equal(firstDifference([Infinity], [Infinity]), -1);
  assert.equal(firstDifference([Infinity], [Number.MAX_VALUE]), 0);
  assert.equal(firstDifference([0], [-0]), -1);
});

test("a different length is a difference", () => {
  assert.equal(firstDifference([1, 2], [1]), 1);
  assert.equal(firstDifference([1], [1, 2]), 1);
});
