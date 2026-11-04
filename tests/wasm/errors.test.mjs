// What the WebAssembly package refuses, and how: the error is a JavaScript `Error` named as the Python package names
// its exception (oracle.py runs each failure through the native extension and records the class), and its message says
// what is wrong. A value of the wrong JavaScript type is a `TypeError`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Patch, floats, native } from "./package.mjs";

const HEADER = 'format = "bullterrier-control/patch"\nversion = 1\n';
const SQUARE = HEADER + '[level]\nuse = "constant"\nvalue = 1\n[also]\nuse = "select"\nevents = "events.kick"\n';

/** The call a failure describes: read the patch, build it, and read the node at its times. */
function attempt({ toml, bundle, node, times }) {
  const built = Patch.fromToml(toml).build(bundle ?? undefined);
  if (node) {
    built.sample(node, floats(times));
  }
}

/** A function that checks an error is an `Error` of this name whose message holds `says`. */
const refusal = (name, says) => (error) => {
  assert.ok(error instanceof Error);
  assert.equal(error.name, name);
  assert.ok(error.message.includes(says), `${error.message} does not say ${says}`);
  return true;
};

for (const failure of native.failures) {
  test(`${failure.id}: refused with the native ${failure.name}`, () => {
    assert.throws(() => attempt(failure), refusal(failure.name, failure.says));
  });
}

test("a node that gives events is not a signal", () => {
  const bundle = JSON.stringify({
    format: "bullterrier-control/bundle",
    version: 1,
    duration: 2,
    events: { kick: [{ t: 0.5 }] },
  });
  const built = Patch.fromToml(SQUARE).build(bundle);
  assert.throws(() => built.sample("also", new Float64Array(1)), refusal("InvalidError", "also: gives events"));
});

const BUILT = Patch.fromToml(HEADER + '[level]\nuse = "constant"\nvalue = 1\n').build();
const WRONG_TYPES = {
  "a TOML text that is not a string": () => Patch.fromToml(3),
  "a bundle that is not a string": () => Patch.fromToml(HEADER).build(5),
  "a node name that is not a string": () => BUILT.sample(1, new Float64Array(1)),
  "times that are an array, not a Float64Array": () => BUILT.sample("level", [0]),
  "times that are a Float32Array": () => BUILT.sample("level", new Float32Array(1)),
};
for (const [id, call] of Object.entries(WRONG_TYPES)) {
  test(`${id}: refused with a TypeError`, () => {
    assert.throws(call, (error) => error instanceof TypeError && /expected/.test(error.message));
  });
}
