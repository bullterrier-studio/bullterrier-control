//! The WebAssembly package of bullterrier-control, for the browser and Node. Each method passes its arguments to the
//! core unchanged, so the core's checks are the only ones and every language gets the same errors; this file only
//! converts between JavaScript's values and the core's, and gives each core error its JavaScript `Error`.

use bullterrier_control as control;
use bullterrier_control::formats::Bundle;
use bullterrier_control::patch::PatchError;
use bullterrier_control::registry::{Output, Registry};
use serde_json::{Map, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// JavaScript's `Float64Array`: whether a value is one, and a copy of its numbers.
    #[wasm_bindgen(js_name = Float64Array)]
    type Floats;
    #[wasm_bindgen(js_namespace = Float64Array, js_name = from)]
    fn floats_of(array: &JsValue) -> Vec<f64>;

    /// JavaScript's `Error`, to give an error the name of its kind.
    #[wasm_bindgen(js_name = Error)]
    type Thrown;
    #[wasm_bindgen(constructor, js_class = "Error")]
    fn new(message: &str) -> Thrown;
    #[wasm_bindgen(method, setter, js_class = "Error", js_name = name)]
    fn set_name(this: &Thrown, name: &str);

    /// JavaScript's `TypeError`, for a value of a type the call cannot read.
    #[wasm_bindgen(js_name = TypeError)]
    type WrongType;
    #[wasm_bindgen(constructor, js_class = "TypeError")]
    fn new(message: &str) -> WrongType;
}

/// An `Error` named `kind` (as `bullterrier_control.errors` names the Python exceptions) with `message`.
fn thrown(kind: &str, message: &str) -> JsValue {
    let error = Thrown::new(message);
    error.set_name(kind);
    error.into()
}

/// The kind of a core error, named as the Python package names its exceptions: a signal that cannot be read a
/// `SignalError`, a text naming no easing curve an `InvalidError`, data breaking a format a `FormatError`, parameters
/// or an input breaking theirs a `ParameterError`, a name not known an `UnknownNameError`.
fn kind_of(error: &control::Error) -> &'static str {
    match error {
        control::Error::Signal { .. } | control::Error::Depth { .. } => "SignalError",
        control::Error::Ease { .. } => "InvalidError",
        control::Error::Format { .. } => "FormatError",
        control::Error::Params { .. } | control::Error::Input { .. } => "ParameterError",
        control::Error::UnknownName { .. } => "UnknownNameError",
        // `Error` is non-exhaustive: a kind this binding does not name yet is still a package error
        _ => "BullterrierControlError",
    }
}

/// The `Error` a core error becomes: its `name` is its kind and its message is the core's.
fn thrown_for(error: &control::Error) -> JsValue {
    thrown(kind_of(error), &error.to_string())
}

/// The `Error` a patch error becomes: a text that is not a patch, a patch that breaks its rules and a path into a
/// bundle that cannot serve it are a `FormatError`, a node not in the patch an `UnknownNameError`, and a thing that
/// refused its call is its own error's kind.
fn thrown_for_patch(error: &PatchError) -> JsValue {
    let kind = match error {
        PatchError::Toml { .. }
        | PatchError::Invalid { .. }
        | PatchError::Unsaved { .. }
        | PatchError::Bundle { .. } => "FormatError",
        PatchError::UnknownNode { .. } => "UnknownNameError",
        PatchError::Node { source, .. } => kind_of(source),
        // `PatchError` is non-exhaustive too
        _ => "BullterrierControlError",
    };
    thrown(kind, &error.to_string())
}

/// The `TypeError` for a value that is not what a call reads.
fn refuse(what: &str, expected: &str, value: &JsValue) -> JsValue {
    let found = value.js_typeof().as_string().unwrap_or_default();
    WrongType::new(&format!("{what}: expected {expected}, got a {found}")).into()
}

/// The text a value is, or the `TypeError` for any other value (wasm-bindgen would read any value as a string and trap).
fn text_of(what: &str, value: &JsValue) -> Result<String, JsValue> {
    value.as_string().ok_or_else(|| refuse(what, "a string", value))
}

/// A patch: sources, shapers and operations as data, read from TOML and built into signals.
#[wasm_bindgen(js_name = Patch)]
pub struct Patch(control::Patch);

#[wasm_bindgen(js_class = Patch)]
impl Patch {
    /// The patch a TOML text describes, its things looked up among the built-ins.
    ///
    /// # Errors
    /// Throws an `Error` named `FormatError` if the text is not TOML or the patch breaks its rules (the message
    /// lists every broken rule at its `node.key`), and a `TypeError` if `text` is not a string.
    #[wasm_bindgen(js_name = fromToml)]
    pub fn from_toml(#[wasm_bindgen(unchecked_param_type = "string")] text: &JsValue) -> Result<Patch, JsValue> {
        let text = text_of("text", text)?;
        control::Patch::from_toml(&text)
            .map(Self)
            .map_err(|error| thrown_for_patch(&error))
    }

    /// Builds the patch's nodes, their paths into a bundle read from the JSON text `bundle_json` (a Bundle file; left
    /// out if no node reads one).
    ///
    /// # Errors
    /// Throws a `FormatError` if the bundle is not a Bundle or holds no such path, an error named for the kind of the
    /// thing's own error (`ParameterError`, `SignalError`, ...) for a node that cannot be built, with the node's name
    /// first in the message; a `TypeError` if `bundle_json` is neither a string nor left out.
    pub fn build(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | undefined")] bundle_json: Option<JsValue>,
    ) -> Result<Built, JsValue> {
        let bundle = match bundle_json {
            None => None,
            Some(json) => Some(
                Bundle::from_json(&text_of("bundleJson", &json)?)
                    .map_err(|error| thrown("FormatError", &error.to_string()))?,
            ),
        };
        self.0
            .build(bundle.as_ref())
            .map(Built)
            .map_err(|error| thrown_for_patch(&error))
    }
}

/// What a patch built: a signal, events or a series for each node.
#[wasm_bindgen(js_name = Built)]
pub struct Built(control::patch::Built);

#[wasm_bindgen(js_class = Built)]
impl Built {
    /// The name of every node built, in file order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.0.iter().map(|(name, _)| name.to_owned()).collect()
    }

    /// The values of the signal node `name` at `times` (seconds, in any order), a `Float64Array` as long as `times`.
    ///
    /// # Errors
    /// Throws an `UnknownNameError` for a node the patch lacks, an `InvalidError` for a node that gives events or a
    /// series, not a signal, a `SignalError` for a time or a value that is not a finite number (naming it), and a
    /// `TypeError` if `name` is not a string or `times` is not a `Float64Array`.
    pub fn sample(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] name: &JsValue,
        #[wasm_bindgen(unchecked_param_type = "Float64Array")] times: &JsValue,
    ) -> Result<Vec<f64>, JsValue> {
        let name = text_of("name", name)?;
        if !times.is_instance_of::<Floats>() {
            return Err(refuse("times", "a Float64Array", times));
        }
        match self.0.get(&name) {
            Some(Output::Signal(signal)) => signal.sample(&floats_of(times)).map_err(|error| thrown_for(&error)),
            Some(other) => Err(thrown(
                "InvalidError",
                &format!("{name}: gives {}, not a signal", other.returns()),
            )),
            None => {
                let has: Vec<_> = self.0.iter().map(|(node, _)| node).collect();
                Err(thrown(
                    "UnknownNameError",
                    &format!("unknown node '{name}'; the patch built: {}", has.join(", ")),
                ))
            }
        }
    }
}

/// Every built-in source, shaper and operation as JSON text: an object of their names to what each is (`kind`,
/// `summary`, `returns`, `inputs` and `parameters`, the JSON Schema of its parameters), as `bt-control list --json`
/// prints them.
#[wasm_bindgen]
#[must_use]
pub fn things() -> String {
    let all: Map<String, Value> = Registry::builtin()
        .things()
        .map(|thing| (thing.name().to_owned(), thing.description()))
        .collect();
    Value::Object(all).to_string()
}
