// The WebAssembly package against the native extension: every example patch, built from the same text and bundle, must
// give the same nodes and bit for bit the same values at the same times, whatever order they are read in. The expected
// values come from oracle.py, which asks the native Python extension; run both through `uv run poe wasm-test`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { Patch, firstDifference, floats, native, things } from "./package.mjs";

const TIMES = floats(native.times);

/** The same indices in a fixed shuffled order (a seeded linear congruential generator: no clock, no randomness). */
function shuffled(length) {
  const order = Array.from({ length }, (_, at) => at);
  let state = 12345;
  for (let at = length - 1; at > 0; at -= 1) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    const other = state % (at + 1);
    [order[at], order[other]] = [order[other], order[at]];
  }
  return order;
}

for (const example of native.cases) {
  const build = () => Patch.fromToml(example.toml).build(example.bundle ?? undefined);

  test(`${example.name}: the nodes and the values are the native ones`, () => {
    const built = build();
    assert.deepEqual(built.names(), example.nodes);
    for (const [node, expected] of Object.entries(example.signals)) {
      const values = built.sample(node, TIMES);
      assert.ok(values instanceof Float64Array);
      assert.equal(firstDifference(expected, values), -1, `${node}: first value that differs`);
    }
  });

  test(`${example.name}: reading in another order or one time at a time gives the same values`, () => {
    const built = build();
    const order = shuffled(TIMES.length);
    const permuted = Float64Array.from(order, (at) => TIMES[at]);
    for (const [node, expected] of Object.entries(example.signals)) {
      const values = built.sample(node, permuted);
      assert.equal(firstDifference(order.map((at) => expected[at]), values), -1, `${node}: shuffled`);
      for (const at of order.slice(0, 25)) {
        const [one] = built.sample(node, new Float64Array([TIMES[at]]));
        assert.ok(Object.is(one, expected[at]), `${node} at ${TIMES[at]}: ${one} is not ${expected[at]}`);
      }
    }
  });
}

test("a read of no times gives no values", () => {
  const [first] = native.cases;
  const [node] = Object.keys(first.signals);
  const values = Patch.fromToml(first.toml).build(first.bundle ?? undefined).sample(node, new Float64Array(0));
  assert.ok(values instanceof Float64Array);
  assert.equal(values.length, 0);
});

test("a view of part of an array reads only that part", () => {
  const [first] = native.cases;
  const [node, expected] = Object.entries(first.signals)[0];
  const values = Patch.fromToml(first.toml).build(first.bundle ?? undefined).sample(node, TIMES.subarray(10, 20));
  assert.equal(firstDifference(expected.slice(10, 20), values), -1);
});

test("things() describes what the native registry holds, parameters included", () => {
  const described = JSON.parse(things());
  assert.deepEqual(Object.keys(described), Object.keys(native.things));
  assert.ok(Object.keys(described).length > 0);
  assert.deepEqual(described, native.things);
});

test("the web build loads too and gives the same values", async () => {
  const web = await import("../../pkg/web/bullterrier_control_wasm.js");
  web.initSync({ module: readFileSync(new URL("../../pkg/web/bullterrier_control_wasm_bg.wasm", import.meta.url)) });
  for (const example of native.cases) {
    const built = web.Patch.fromToml(example.toml).build(example.bundle ?? undefined);
    for (const [node, expected] of Object.entries(example.signals)) {
      assert.equal(firstDifference(expected, built.sample(node, TIMES)), -1, `${example.name} ${node}`);
    }
  }
  assert.deepEqual(JSON.parse(web.things()), native.things);
});
