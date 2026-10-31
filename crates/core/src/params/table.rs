//! The table of a thing's parameters: its declarations, checked once, and the values and description read from it.

use serde_json::{Map, Value};

use super::{ParamError, ParamUnit, check};
use crate::formats::{Problem, Problems};

/// What kind of value a field holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A finite decimal (`f64`); a JSON integer is read as one.
    Float,
    /// A whole number (`i64`): a JSON integer, never a decimal such as `1.0`.
    Whole,
    /// `true` or `false`, never `1`.
    Switch,
    /// Any text.
    Text,
    /// One of these names.
    Choice(Vec<String>),
}

/// One field's declaration: what [`ParamTable::new`] checks.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// The field's name, as a value object spells it.
    pub name: String,
    /// What kind of value it holds.
    pub kind: Kind,
    /// The value it takes when none is given, as JSON (a decimal `0.5`, a name `"sine"`).
    pub default: Value,
    /// What it does: its unit and range in words, and what raising it does.
    pub doc: String,
    /// What a number is measured in; `None` for any other kind.
    pub unit: Option<ParamUnit>,
    /// The lowest value of a number, inclusive; `None` for any other kind.
    pub min: Option<f64>,
    /// The highest value of a number, inclusive; `None` for any other kind.
    pub max: Option<f64>,
    /// It tunes the technique rather than choosing the shape: listed after the basic fields.
    pub advanced: bool,
    /// A phase: any finite number is accepted and wrapped into its one-cycle range.
    pub cyclic: bool,
}

impl Param {
    /// A number's range as help text shows it, `min to max` (`0.5 to 3600`); `None` for a field that is not a number.
    /// A whole number's ends print in full (`0 to 4294967295`); a decimal's like C's `%g`, at most six significant
    /// digits and an exponent only when it is below `1e-4` or from `1e6` (`-1e+06 to 1e+06`).
    ///
    /// ```
    /// let table = bullterrier_control::parameters("noise")?;
    /// let ranges: Vec<Option<String>> = table.params().iter().map(|param| param.range_text()).collect();
    /// assert_eq!(ranges[0], None); // shape
    /// assert_eq!(ranges[1].as_deref(), Some("0.001 to 1000")); // rate
    /// assert_eq!(ranges[2].as_deref(), Some("0 to 4294967295")); // seed
    /// assert_eq!(ranges[3].as_deref(), Some("-1e+06 to 1e+06")); // low
    /// # Ok::<(), bullterrier_control::Error>(())
    /// ```
    #[must_use]
    pub fn range_text(&self) -> Option<String> {
        let (min, max) = (self.min?, self.max?);
        let end = |value: f64| match self.kind {
            Kind::Float => general(value),
            _ => check::bound_json(&self.kind, value).to_string(),
        };
        Some(format!("{} to {}", end(min), end(max)))
    }
}

/// `value` as C's `%g` writes it (and Python's `format(value, "g")`): six significant digits, trailing zeros dropped,
/// an exponent (`e+06`, at least two digits) when the decimal exponent is below -4 or at least 6.
fn general(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    // the exponent after rounding to six digits: `{:.5e}` is `d.ddddde<exponent>`
    let scientific = format!("{value:.5e}");
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return scientific;
    };
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let trimmed = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text.to_owned()
        }
    };
    if (-4..6).contains(&exponent) {
        let places = usize::try_from(5 - exponent).unwrap_or(0);
        trimmed(&format!("{value:.places$}"))
    } else {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", trimmed(mantissa), exponent.abs())
    }
}

/// The parameters of one source, shaper or operation, every declaration checked. Built by [`ParamTable::new`] only.
#[derive(Debug, Clone)]
pub struct ParamTable {
    params: Vec<Param>,
    rule: Option<Rule>,
}

/// Two tables are equal when their declarations are and both have a rule between fields or neither does (function
/// pointers have no meaningful equality).
impl PartialEq for ParamTable {
    fn eq(&self, other: &Self) -> bool {
        self.params == other.params && self.rule.is_some() == other.rule.is_some()
    }
}

/// A rule between fields, which a table runs on values that passed every field's own rules: the problems it finds, each
/// under the field it is about.
pub type Rule = fn(&Values) -> Vec<Problem>;

/// Values a [`ParamTable`] accepted: one per field, each a valid value of its kind (a default where none was given, a
/// cyclic number wrapped, a decimal held as a JSON decimal).
#[derive(Debug, Clone, PartialEq)]
pub struct Values(Map<String, Value>);

impl Values {
    /// The value of the field `name`; `None` if the table has no such field.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    /// The values as a JSON object, by name.
    #[must_use]
    pub fn into_json(self) -> Value {
        Value::Object(self.0)
    }
}

impl ParamTable {
    /// The table of `params`, in their order, if every declaration follows the rules (see [the module](super)).
    /// A default is kept as the value it reads as (a decimal declared `1` is `1.0`).
    ///
    /// ```
    /// use bullterrier_control::params::{Kind, Param, ParamTable, ParamUnit};
    ///
    /// let rate = Param {
    ///     name: "rate".to_owned(),
    ///     kind: Kind::Float,
    ///     default: 1.into(),
    ///     doc: "Cycles per second.".to_owned(),
    ///     unit: Some(ParamUnit::Hertz),
    ///     min: Some(0.0),
    ///     max: Some(10.0),
    ///     advanced: false,
    ///     cyclic: false,
    /// };
    /// assert!(ParamTable::new(vec![rate.clone()]).is_ok());
    /// let strength = Param { name: "strength".to_owned(), ..rate };
    /// let refused = ParamTable::new(vec![strength]).map(|_| ()).map_err(|error| error.to_string());
    /// assert_eq!(refused, Err(r#"strength: do not name a parameter "strength"; use depth or amount"#.to_owned()));
    /// ```
    ///
    /// # Errors
    /// A [`ParamError`] listing every rule broken, each under the name of its field.
    pub fn new(mut params: Vec<Param>) -> Result<Self, ParamError> {
        let mut problems = Problems::new();
        check::declarations(&mut params, &mut problems);
        match ParamError::from_problems(problems) {
            Some(error) => Err(error),
            None => Ok(Self { params, rule: None }),
        }
    }

    /// The table with `rule` added: a rule between fields (`high` is not below `low`), which [`parse`](Self::parse) runs
    /// once every field has passed its own rules, so it only ever sees valid values. A table has no rule unless one
    /// is added; `#[derive(Params)]` adds the one named by `#[param(rule = path)]` on the struct.
    #[must_use]
    pub fn with_rule(mut self, rule: Rule) -> Self {
        self.rule = Some(rule);
        self
    }

    /// The values `given` sets, every field included: `given` is a JSON object of values by name, and a field it
    /// leaves out takes its default. Read strictly: a number is a JSON number (not `true`, not `"1"`), a whole number
    /// a JSON integer (not `1.0`), a switch `true` or `false` (not `1`); a number lies in its range, a choice is one of
    /// its names, and a field the table does not have is refused. A cyclic number outside its range is wrapped into it
    /// (1.25 on 0 to 1 is 0.25; an integer exactly, however large); one inside it, either end included, is kept.
    ///
    /// A `serde_json::Value` cannot hold NaN or an infinity (`serde_json::Number` refuses both), so none reaches
    /// here: whoever turns a float into a `Value` refuses them first (`Value::from(f64::NAN)` is `null`, which is
    /// refused as a number).
    ///
    /// ```
    /// # use bullterrier_control::params::{Kind, Param, ParamTable, ParamUnit};
    /// # use serde_json::json;
    /// # let phase = Param { name: "phase".to_owned(), kind: Kind::Float, default: 0.0.into(),
    /// #     doc: "Where the cycle starts.".to_owned(), unit: Some(ParamUnit::Cycles), min: Some(0.0),
    /// #     max: Some(1.0), advanced: false, cyclic: true };
    /// let table = ParamTable::new(vec![phase])?;
    /// assert_eq!(table.parse(&json!({"phase": 1.25}))?.into_json(), json!({"phase": 0.25}));
    /// assert_eq!(table.parse(&json!({}))?.into_json(), json!({"phase": 0.0}));
    /// let refused = table.parse(&json!({"phase": true, "speed": 2})).map_err(|error| error.to_string());
    /// assert_eq!(refused, Err("phase: must be a number (got true); speed: unknown field".to_owned()));
    /// # Ok::<(), bullterrier_control::params::ParamError>(())
    /// ```
    ///
    /// # Errors
    /// A [`ParamError`] listing every rule broken: each field's in table order, then each unknown field; a `given`
    /// that is not an object is one problem at the empty path. The rule between fields ([`with_rule`](Self::with_rule))
    /// is checked only when no other rule is broken.
    pub fn parse(&self, given: &Value) -> Result<Values, ParamError> {
        let Value::Object(given) = given else {
            return Err(ParamError::at(
                "",
                format!("must be an object of parameter values (got {})", check::shown(given)),
            ));
        };
        let mut problems = Problems::new();
        let mut values = Map::new();
        for param in &self.params {
            let value = match given.get(&param.name) {
                None => Ok(param.default.clone()),
                Some(value) => check::value(param, value),
            };
            match value {
                Ok(value) => {
                    values.insert(param.name.clone(), value);
                }
                Err(rule) => problems.push(Problem {
                    path: param.name.clone(),
                    rule,
                }),
            }
        }
        for name in given.keys() {
            if !self.params.iter().any(|param| param.name == *name) {
                problems.push_with(|| Problem {
                    path: name.clone(),
                    rule: "unknown field".to_owned(),
                });
            }
        }
        let values = Values(values);
        if let (true, Some(rule)) = (problems.is_empty(), self.rule) {
            for problem in rule(&values) {
                problems.push(problem);
            }
        }
        match ParamError::from_problems(problems) {
            Some(error) => Err(error),
            None => Ok(values),
        }
    }

    /// The fields as a consumer reads them: a JSON object by name (in the shape of a JSON Schema's `properties`), each
    /// field `type` (`number`, `integer`, `boolean` or `string`) with `enum` for a choice (`const` for a choice of
    /// one), `default`, `description`, `title` (the name in title case), `unit` (`null` but for a number), `advanced`
    /// and `cyclic`, and for a number `minimum` and `maximum` (decimals for a decimal, integers for a whole number).
    /// A cyclic field keeps its one-cycle range although any number is accepted: a consumer wraps a value first.
    ///
    /// ```
    /// # use bullterrier_control::params::{Kind, Param, ParamTable};
    /// # use serde_json::json;
    /// let on = Param { name: "on".to_owned(), kind: Kind::Switch, default: true.into(),
    ///     doc: "Whether it runs.".to_owned(), unit: None, min: None, max: None, advanced: false, cyclic: false };
    /// let described = ParamTable::new(vec![on])?.describe();
    /// assert_eq!(described, json!({"on": {"type": "boolean", "default": true, "description": "Whether it runs.",
    ///     "title": "On", "unit": null, "advanced": false, "cyclic": false}}));
    /// # Ok::<(), bullterrier_control::params::ParamError>(())
    /// ```
    #[must_use]
    pub fn describe(&self) -> Value {
        Value::Object(
            self.params
                .iter()
                .map(|param| (param.name.clone(), described(param)))
                .collect(),
        )
    }

    /// The declarations, in the order they were given, each default as it reads (see [`new`](Self::new)).
    #[must_use]
    pub fn params(&self) -> &[Param] {
        &self.params
    }

    /// The declarations in the order a reader is shown them (`help()`, a listing): the basic fields, then the
    /// advanced ones, each in declaration order.
    ///
    /// ```
    /// # use bullterrier_control::params::{Kind, Param, ParamTable};
    /// let switch = |name: &str, advanced| Param { name: name.to_owned(), kind: Kind::Switch, default: true.into(),
    ///     doc: "Whether it runs.".to_owned(), unit: None, min: None, max: None, advanced, cyclic: false };
    /// let table = ParamTable::new(vec![switch("fine", true), switch("on", false), switch("loud", false)])?;
    /// let names: Vec<&str> = table.listed().map(|param| param.name.as_str()).collect();
    /// assert_eq!(names, ["on", "loud", "fine"]);
    /// # Ok::<(), bullterrier_control::params::ParamError>(())
    /// ```
    pub fn listed(&self) -> impl Iterator<Item = &Param> {
        let basic = self.params.iter().filter(|param| !param.advanced);
        basic.chain(self.params.iter().filter(|param| param.advanced))
    }
}

/// The description of one field (see [`ParamTable::describe`]).
fn described(param: &Param) -> Value {
    let mut field = Map::new();
    let kind = match &param.kind {
        Kind::Float => "number",
        Kind::Whole => "integer",
        Kind::Switch => "boolean",
        Kind::Text | Kind::Choice(_) => "string",
    };
    field.insert("type".to_owned(), kind.into());
    match &param.kind {
        // a choice of one is a constant, as pydantic writes it
        Kind::Choice(names) if names.len() == 1 => {
            field.insert("const".to_owned(), names[0].clone().into());
        }
        Kind::Choice(names) => {
            field.insert("enum".to_owned(), names.clone().into());
        }
        Kind::Float | Kind::Whole | Kind::Switch | Kind::Text => {}
    }
    field.insert("default".to_owned(), param.default.clone());
    field.insert("description".to_owned(), param.doc.clone().into());
    field.insert("title".to_owned(), title(&param.name).into());
    field.insert("unit".to_owned(), param.unit.map(ParamUnit::as_str).into());
    if let Some(min) = param.min {
        field.insert("minimum".to_owned(), check::bound_json(&param.kind, min));
    }
    if let Some(max) = param.max {
        field.insert("maximum".to_owned(), check::bound_json(&param.kind, max));
    }
    field.insert("advanced".to_owned(), param.advanced.into());
    field.insert("cyclic".to_owned(), param.cyclic.into());
    Value::Object(field)
}

/// `name` as a title, as pydantic writes one: title case (a letter after one is lower case, any other upper case),
/// underscores as spaces (`start_phase` is `Start Phase`, `name_2x` is `Name 2X`).
fn title(name: &str) -> String {
    let mut title = String::new();
    let mut after_letter = false;
    for character in name.chars() {
        let cased = character.is_lowercase() || character.is_uppercase();
        if !cased {
            title.push(if character == '_' { ' ' } else { character });
        } else if after_letter {
            title.extend(character.to_lowercase());
        } else {
            title.extend(character.to_uppercase());
        }
        after_letter = cased;
    }
    title.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::{Value, json};

    use super::{Kind, Param, ParamTable, Values};
    use crate::formats::{Problem, check_problems};
    use crate::params::{ParamError, ParamUnit};

    /// The problems wanted: each a path and a short part of its rule.
    type Expected = &'static [(&'static str, &'static str)];

    /// A decimal `amount` in ratio, 0 to 2, default 1: valid, so each row breaks one thing.
    fn amount() -> Param {
        Param {
            name: "amount".to_owned(),
            kind: Kind::Float,
            default: json!(1.0),
            doc: "Some number.".to_owned(),
            unit: Some(ParamUnit::Ratio),
            min: Some(0.0),
            max: Some(2.0),
            advanced: false,
            cyclic: false,
        }
    }

    /// A field that is not a number: `kind` with `default`, no unit or range.
    fn plain(name: &str, kind: Kind, default: Value) -> Param {
        Param {
            name: name.to_owned(),
            kind,
            default,
            doc: "Some field.".to_owned(),
            unit: None,
            min: None,
            max: None,
            advanced: false,
            cyclic: false,
        }
    }

    /// A cyclic decimal `phase` in cycles from `min` to `min + 1`, default `min`.
    fn phase(name: &str, min: f64) -> Param {
        Param {
            name: name.to_owned(),
            default: json!(min),
            unit: Some(ParamUnit::Cycles),
            min: Some(min),
            max: Some(min + 1.0),
            cyclic: true,
            ..amount()
        }
    }

    fn choices(names: &[&str]) -> Kind {
        Kind::Choice(names.iter().map(|name| (*name).to_owned()).collect())
    }

    fn check(id: &str, result: Result<ParamTable, ParamError>, expected: Expected) {
        match result {
            Ok(table) => assert!(expected.is_empty(), "{id}: accepted {table:?}, wanted {expected:?}"),
            Err(error) => {
                assert!(!expected.is_empty(), "{id}: refused with {error}");
                check_problems(id, &error.problems, expected);
            }
        }
    }

    #[test]
    fn a_declaration_is_accepted_only_when_it_follows_every_rule() {
        let whole = Param {
            name: "seed".to_owned(),
            kind: Kind::Whole,
            default: json!(0),
            unit: Some(ParamUnit::Count),
            min: Some(0.0),
            max: Some(4_294_967_295.0),
            ..amount()
        };
        #[rustfmt::skip]
        let rows: Vec<(&str, Vec<Param>, Expected)> = vec![
            ("a decimal with unit, range and default", vec![amount()], &[]),
            ("a default at the lowest end", vec![Param { default: json!(0.0), ..amount() }], &[]),
            ("a default at the highest end", vec![Param { default: json!(2.0), ..amount() }], &[]),
            ("a decimal's default written as an integer", vec![Param { default: json!(1), ..amount() }], &[]),
            ("a whole number with unit and range", vec![whole.clone()], &[]),
            ("a switch, text and a choice", vec![
                plain("on", Kind::Switch, json!(true)),
                plain("label", Kind::Text, json!("kick")),
                plain("shape", choices(&["sine", "saw"]), json!("saw")),
            ], &[]),
            ("a cyclic decimal in cycles, one cycle wide", vec![phase("phase", 0.0), phase("centred", -0.5)], &[]),
            ("no fields at all", vec![], &[]),
            ("an empty description", vec![Param { doc: String::new(), ..amount() }], &[("amount", "needs a description")]),
            ("a blank description", vec![Param { doc: " \n ".to_owned(), ..amount() }], &[("amount", "needs a description")]),
            ("a number without a unit", vec![Param { unit: None, ..amount() }], &[("amount", "needs a unit, one of seconds, beats, hertz")]),
            ("a whole number without a unit", vec![Param { unit: None, ..whole.clone() }], &[("seed", "needs a unit")]),
            ("a number without a range", vec![Param { min: None, max: None, ..amount() }], &[("amount", "both min and max (got min none, max none)")]),
            ("a number with half a range", vec![Param { max: None, ..amount() }], &[("amount", "both min and max (got min 0.0, max none)")]),
            ("a range with no top", vec![Param { max: Some(f64::INFINITY), ..amount() }], &[("amount", "finite numbers (got min 0.0, max Infinity)")]),
            ("a range with a NaN end", vec![Param { min: Some(f64::NAN), ..amount() }], &[("amount", "finite numbers (got min NaN, max 2.0)")]),
            ("a default below its range", vec![Param { default: json!(-1.0), ..amount() }], &[("amount", "the default -1.0 must lie in the range 0.0 to 2.0")]),
            ("a default above its range", vec![Param { default: json!(3.0), ..amount() }], &[("amount", "the default 3.0 must lie in the range 0.0 to 2.0")]),
            ("a range upside down", vec![Param { min: Some(2.0), max: Some(0.0), ..amount() }], &[("amount", "must lie in the range 2.0 to 0.0")]),
            ("a whole default past a bound beyond 2^53, exactly", vec![Param {
                default: json!(9_007_199_254_740_993_i64), min: Some(0.0), max: Some(9_007_199_254_740_992.0), ..whole.clone()
            }], &[("seed", "the default 9007199254740993 must lie in the range 0 to 9007199254740992")]),
            ("a switch with a unit", vec![Param { unit: Some(ParamUnit::Count), ..plain("on", Kind::Switch, json!(true)) }], &[("on", "takes no unit, min or max")]),
            ("a choice with a range", vec![Param { min: Some(0.0), max: Some(1.0), ..plain("shape", choices(&["a", "b"]), json!("a")) }], &[("shape", "takes no unit, min or max")]),
            ("text with a minimum", vec![Param { min: Some(0.0), ..plain("label", Kind::Text, json!("a")) }], &[("label", "takes no unit, min or max")]),
            ("no default", vec![Param { default: Value::Null, ..amount() }], &[("amount", "needs a default")]),
            ("a decimal whose default is true", vec![Param { default: json!(true), ..amount() }], &[("amount", "the default is not valid: must be a number (got true)")]),
            ("a whole number whose default is 1.5", vec![Param { default: json!(1.5), ..whole.clone() }], &[("seed", "the default is not valid: must be a whole number (got 1.5)")]),
            ("a switch whose default is 1", vec![plain("on", Kind::Switch, json!(1))], &[("on", "the default is not valid: must be true or false (got 1)")]),
            ("text whose default is a number", vec![plain("label", Kind::Text, json!(3))], &[("label", "the default is not valid: must be text (got 3)")]),
            ("a choice whose default is not offered", vec![plain("shape", choices(&["a", "b"]), json!("zzz"))], &[("shape", r#"the default is not valid: must be one of "a", "b" (got "zzz")"#)]),
            ("a cyclic number in seconds", vec![Param { unit: Some(ParamUnit::Seconds), ..phase("phase", 0.0) }], &[("phase", "cyclic is for a decimal in cycles whose range is exactly one cycle (max = min + 1); got unit seconds, min 0.0, max 1.0")]),
            ("a cyclic range of two cycles", vec![Param { max: Some(2.0), ..phase("phase", 0.0) }], &[("phase", "got unit cycles, min 0.0, max 2.0")]),
            ("a cyclic range of half a cycle", vec![Param { max: Some(0.5), ..phase("phase", 0.0) }], &[("phase", "exactly one cycle")]),
            ("a cyclic whole number", vec![Param { cyclic: true, unit: Some(ParamUnit::Cycles), max: Some(1.0), ..whole.clone() }], &[("seed", "cyclic is for a decimal")]),
            ("a cyclic switch", vec![Param { cyclic: true, ..plain("on", Kind::Switch, json!(true)) }], &[("on", "got unit none, min none, max none")]),
            ("the name strength", vec![Param { name: "strength".to_owned(), ..amount() }], &[("strength", r#"do not name a parameter "strength"; use depth or amount"#)]),
            ("the name intensity", vec![Param { name: "intensity".to_owned(), ..amount() }], &[("intensity", "use depth or amount")]),
            ("the name factor", vec![Param { name: "factor".to_owned(), ..amount() }], &[("factor", "use ratio, depth or amount")]),
            ("the name val", vec![Param { name: "val".to_owned(), ..amount() }], &[("val", "use value")]),
            ("the name k", vec![Param { name: "k".to_owned(), ..amount() }], &[("k", "use a name that says what it is")]),
            ("a name that is a unit", vec![Param { name: "seconds".to_owned(), ..amount() }], &[("seconds", r#"do not name a parameter after its unit ("seconds"); say what it is"#)]),
            ("a name declared twice", vec![amount(), Param { default: json!(2.0), ..amount() }], &[("amount", "declared more than once")]),
            ("no name", vec![Param { name: String::new(), ..amount() }], &[("", "needs a name")]),
            ("a choice of one", vec![plain("shape", choices(&["sine"]), json!("sine"))], &[]),
            ("a choice offered twice", vec![plain("shape", choices(&["sine", "saw", "sine"]), json!("saw"))], &[("shape", r#"the choice "sine" is offered more than once"#)]),
            ("a choice offered three times, said once", vec![plain("shape", choices(&["saw", "saw", "saw"]), json!("saw"))], &[("shape", r#"the choice "saw" is offered"#)]),
            ("a choice of nothing", vec![plain("shape", choices(&[]), json!("sine"))], &[("shape", "a choice needs at least one name"), ("shape", "the default is not valid")]),
            ("every rule a field breaks", vec![Param { doc: String::new(), unit: None, default: json!(5.0), ..amount() }], &[
                ("amount", "needs a description"), ("amount", "needs a unit"), ("amount", "the default 5.0 must lie"),
            ]),
            ("every field's broken rules, in order", vec![
                Param { name: "k".to_owned(), ..amount() },
                plain("on", Kind::Switch, json!(true)),
                Param { unit: None, ..amount() },
            ], &[("k", "use a name"), ("amount", "needs a unit")]),
        ];
        for (id, params, expected) in rows {
            check(id, ParamTable::new(params), expected);
        }
    }

    #[test]
    fn a_choice_of_fifty_thousand_names_is_checked_in_linear_time() {
        let names: Vec<String> = (0..50_000).map(|index| format!("name{index}")).collect();
        let mut repeated = names.clone();
        repeated.push("name7".to_owned());
        let started = std::time::Instant::now();
        let accepted = ParamTable::new(vec![plain("shape", Kind::Choice(names), json!("name0"))]);
        let refused = ParamTable::new(vec![plain("shape", Kind::Choice(repeated), json!("name0"))]);
        let elapsed = started.elapsed();
        check("fifty thousand names", accepted, &[]);
        check(
            "fifty thousand names and a repeat",
            refused,
            &[("shape", r#"the choice "name7" is offered more than once"#)],
        );
        assert!(elapsed < std::time::Duration::from_secs(1), "took {elapsed:?}");
    }

    #[test]
    fn every_unit_can_measure_a_number() {
        for unit in ParamUnit::ALL {
            let table = ParamTable::new(vec![Param {
                unit: Some(unit),
                ..amount()
            }]);
            check(unit.as_str(), table, &[]);
        }
    }

    #[test]
    fn a_default_is_kept_as_the_value_it_reads_as() -> Result<(), ParamError> {
        let table = ParamTable::new(vec![Param {
            default: json!(1),
            ..amount()
        }])?;
        assert_eq!(table.params()[0].default, json!(1.0));
        assert_eq!(table.parse(&json!({}))?.into_json(), json!({"amount": 1.0}));
        Ok(())
    }

    /// A table of every kind: `rate` 0 to 10 Hz, `seed` 0 to 4294967295, a switch, text, a choice and a phase.
    fn every_kind() -> Result<ParamTable, ParamError> {
        ParamTable::new(vec![
            Param {
                name: "rate".to_owned(),
                unit: Some(ParamUnit::Hertz),
                max: Some(10.0),
                ..amount()
            },
            Param {
                name: "seed".to_owned(),
                kind: Kind::Whole,
                default: json!(3),
                unit: Some(ParamUnit::Count),
                min: Some(0.0),
                max: Some(4_294_967_295.0),
                ..amount()
            },
            plain("on", Kind::Switch, json!(false)),
            plain("label", Kind::Text, json!("a")),
            plain("shape", choices(&["sine", "saw"]), json!("sine")),
            phase("phase", 0.0),
        ])
    }

    /// What a parse should give: the values, or the problems.
    enum Outcome {
        Accepted(Value),
        Refused(Expected),
    }

    #[test]
    fn values_are_read_strictly_and_every_broken_rule_is_listed_with_its_path() -> Result<(), ParamError> {
        use Outcome::{Accepted, Refused};
        let table = every_kind()?;
        let long = "x".repeat(100);
        #[rustfmt::skip]
        let rows: Vec<(&str, Value, Outcome)> = vec![
            ("nothing given: every default", json!({}),
                Accepted(json!({"rate": 1.0, "seed": 3, "on": false, "label": "a", "shape": "sine", "phase": 0.0}))),
            ("every field given", json!({"rate": 2.5, "seed": 7, "on": true, "label": "b", "shape": "saw", "phase": 0.5}),
                Accepted(json!({"rate": 2.5, "seed": 7, "on": true, "label": "b", "shape": "saw", "phase": 0.5}))),
            ("an integer for a decimal is a decimal", json!({"rate": 4}),
                Accepted(json!({"rate": 4.0, "seed": 3, "on": false, "label": "a", "shape": "sine", "phase": 0.0}))),
            ("both ends of each range", json!({"rate": 10, "seed": 4_294_967_295_u64}),
                Accepted(json!({"rate": 10.0, "seed": 4_294_967_295_u64, "on": false, "label": "a", "shape": "sine", "phase": 0.0}))),
            ("the lowest ends", json!({"rate": 0.0, "seed": 0}),
                Accepted(json!({"rate": 0.0, "seed": 0, "on": false, "label": "a", "shape": "sine", "phase": 0.0}))),
            ("true for a number", json!({"rate": true}), Refused(&[("rate", "must be a number (got true)")])),
            ("a numeric string for a number", json!({"rate": "1"}), Refused(&[("rate", r#"must be a number (got "1")"#)])),
            ("null for a number", json!({"rate": null}), Refused(&[("rate", "must be a number (got null)")])),
            ("NaN as a Value is null, refused", json!({"rate": Value::from(f64::NAN)}), Refused(&[("rate", "must be a number (got null)")])),
            ("an infinity as a Value is null, refused", json!({"phase": Value::from(f64::INFINITY)}), Refused(&[("phase", "must be a number (got null)")])),
            ("1.0 for a whole number", json!({"seed": 1.0}), Refused(&[("seed", "must be a whole number (got 1.0)")])),
            ("true for a whole number", json!({"seed": true}), Refused(&[("seed", "must be a whole number (got true)")])),
            ("a whole number beyond 64 bits signed", json!({"seed": u64::MAX}), Refused(&[("seed", "within 64 bits (got 18446744073709551615)")])),
            ("1 for a switch", json!({"on": 1}), Refused(&[("on", "must be true or false (got 1)")])),
            ("a string for a switch", json!({"on": "true"}), Refused(&[("on", r#"must be true or false (got "true")"#)])),
            ("a number for text", json!({"label": 3}), Refused(&[("label", "must be text (got 3)")])),
            ("a name not offered", json!({"shape": "tri"}), Refused(&[("shape", r#"must be one of "sine", "saw" (got "tri")"#)])),
            ("a number for a choice", json!({"shape": 1}), Refused(&[("shape", "must be one of")])),
            ("a long value is shown cut short", json!({"shape": long}), Refused(&[("shape", r#"(got "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx...)"#)])),
            ("above the range", json!({"rate": 10.5}), Refused(&[("rate", "must be <= 10.0 (got 10.5)")])),
            ("below the range", json!({"rate": -1}), Refused(&[("rate", "must be >= 0.0 (got -1)")])),
            ("a whole number below its range", json!({"seed": -1}), Refused(&[("seed", "must be >= 0 (got -1)")])),
            ("a whole number above its range", json!({"seed": 4_294_967_296_u64}), Refused(&[("seed", "must be <= 4294967295 (got 4294967296)")])),
            ("an unknown field", json!({"speed": 2}), Refused(&[("speed", "unknown field")])),
            ("an array", json!([1]), Refused(&[("", "must be an object of parameter values (got [1])")])),
            ("a bare number", json!(1), Refused(&[("", "must be an object")])),
            ("every broken rule, fields in table order, unknown ones last",
                json!({"bogus": 2, "rate": true, "seed": 1.0, "on": 1, "shape": "x", "label": 3, "phase": "x"}),
                Refused(&[("rate", "number"), ("seed", "whole"), ("on", "true or false"), ("label", "text"),
                    ("shape", "one of"), ("phase", "number"), ("bogus", "unknown field")])),
        ];
        for (id, given, outcome) in rows {
            let result = table.parse(&given);
            match outcome {
                Accepted(expected) => assert_eq!(result.map(Values::into_json), Ok(expected), "{id}"),
                Refused(expected) => {
                    assert!(result.is_err(), "{id}: accepted {result:?}");
                    if let Err(error) = result {
                        check_problems(id, &error.problems, expected);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn past_100_problems_the_rest_are_counted() -> Result<(), ParamError> {
        let table = every_kind()?;
        let given: serde_json::Map<String, Value> = (0..150).map(|index| (format!("x{index:03}"), json!(1))).collect();
        let refused = table.parse(&Value::Object(given));
        assert!(refused.is_err(), "accepted {refused:?}");
        let Err(error) = refused else {
            return Ok(());
        };
        assert_eq!(error.problems.len(), 100);
        assert_eq!(error.omitted(), 50);
        assert!(error.to_string().ends_with("; ... and 50 more"), "{error}");
        Ok(())
    }

    #[test]
    fn a_cyclic_number_outside_its_range_is_wrapped_into_it_and_one_inside_kept() -> Result<(), ParamError> {
        let table = ParamTable::new(vec![phase("phase", 0.0), phase("centred", -0.5), phase("offset", 0.25)])?;
        #[rustfmt::skip]
        let rows: Vec<(&str, Value, &str, f64)> = vec![
            ("inside: kept", json!({"phase": 0.25}), "phase", 0.25),
            ("the lowest end: kept", json!({"phase": 0.0}), "phase", 0.0),
            ("the highest end: kept, not wrapped to the lowest", json!({"phase": 1.0}), "phase", 1.0),
            ("the highest end as an integer: kept", json!({"phase": 1}), "phase", 1.0),
            ("a centred range's lowest end: kept", json!({"centred": -0.5}), "centred", -0.5),
            ("a centred range's highest end: kept", json!({"centred": 0.5}), "centred", 0.5),
            ("a quarter past one cycle", json!({"phase": 1.25}), "phase", 0.25),
            ("a quarter before the start", json!({"phase": -0.25}), "phase", 0.75),
            ("many cycles on", json!({"phase": 7.5}), "phase", 0.5),
            ("an integer past the range", json!({"phase": 2}), "phase", 0.0),
            ("past a centred range", json!({"centred": 0.75}), "centred", -0.25),
            ("an integer past a centred range: half a cycle from its start", json!({"centred": 3}), "centred", 0.0),
            ("a negative integer onto a centred range", json!({"centred": -7}), "centred", 0.0),
            ("1e300: no fraction left", json!({"phase": 1e300}), "phase", 0.0),
            ("1e300 onto a centred range: no fraction left after adding a half", json!({"centred": 1e300}), "centred", -0.5),
            ("an integer far beyond f64 exactness, exactly", json!({"centred": u64::MAX}), "centred", 0.0),
            ("the lowest 64-bit integer, exactly", json!({"centred": i64::MIN}), "centred", 0.0),
            ("a decimal past an off-centre range", json!({"offset": 1.5}), "offset", 0.5),
            ("an integer past an off-centre range: three quarters from its start", json!({"offset": 5}), "offset", 1.0),
            ("a negative integer onto an off-centre range", json!({"offset": -3}), "offset", 1.0),
        ];
        for (id, given, field, expected) in rows {
            let values = table.parse(&given)?;
            assert_eq!(values.get(field), Some(&json!(expected)), "{id}");
        }
        for not_a_number in [
            json!(true),
            json!("0.5"),
            Value::from(f64::NAN),
            Value::from(f64::NEG_INFINITY),
        ] {
            let refused = table.parse(&json!({"phase": not_a_number}));
            assert!(
                matches!(&refused, Err(error) if error.problems[0].rule.starts_with("must be a number")),
                "{not_a_number}: {refused:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_table_describes_each_field_as_a_consumer_reads_it() -> Result<(), ParamError> {
        let numbers = ParamTable::new(vec![
            Param {
                name: "start_phase".to_owned(),
                default: json!(0.25),
                doc: "Where it starts.".to_owned(),
                advanced: true,
                ..phase("start_phase", -0.5)
            },
            Param {
                name: "rate".to_owned(),
                default: json!(1),
                doc: "How fast.".to_owned(),
                unit: Some(ParamUnit::Hertz),
                max: Some(10.0),
                ..amount()
            },
            Param {
                name: "seed".to_owned(),
                kind: Kind::Whole,
                default: json!(7),
                doc: "Which noise.".to_owned(),
                unit: Some(ParamUnit::Count),
                min: Some(0.0),
                max: Some(4_294_967_295.0),
                ..amount()
            },
        ])?;
        assert_eq!(
            numbers.describe(),
            json!({
                "start_phase": {"type": "number", "default": 0.25, "description": "Where it starts.", "title": "Start Phase",
                    "unit": "cycles", "minimum": -0.5, "maximum": 0.5, "advanced": true, "cyclic": true},
                "rate": {"type": "number", "default": 1.0, "description": "How fast.", "title": "Rate",
                    "unit": "hertz", "minimum": 0.0, "maximum": 10.0, "advanced": false, "cyclic": false},
                "seed": {"type": "integer", "default": 7, "description": "Which noise.", "title": "Seed",
                    "unit": "count", "minimum": 0, "maximum": 4_294_967_295_u64, "advanced": false, "cyclic": false},
            })
        );
        let others = ParamTable::new(vec![
            Param {
                doc: "The wave.".to_owned(),
                ..plain("shape", choices(&["sine", "saw-down"]), json!("saw-down"))
            },
            Param {
                doc: "The one way.".to_owned(),
                advanced: true,
                ..plain("only", choices(&["one"]), json!("one"))
            },
            Param {
                doc: "A name.".to_owned(),
                ..plain("label_2x", Kind::Text, json!("hi"))
            },
            Param {
                doc: "Whether it runs.".to_owned(),
                ..plain("on", Kind::Switch, json!(true))
            },
        ])?;
        assert_eq!(
            others.describe(),
            json!({
                "shape": {"type": "string", "enum": ["sine", "saw-down"], "default": "saw-down", "description": "The wave.",
                    "title": "Shape", "unit": null, "advanced": false, "cyclic": false},
                "only": {"type": "string", "const": "one", "default": "one", "description": "The one way.",
                    "title": "Only", "unit": null, "advanced": true, "cyclic": false},
                "label_2x": {"type": "string", "default": "hi", "description": "A name.",
                    "title": "Label 2X", "unit": null, "advanced": false, "cyclic": false},
                "on": {"type": "boolean", "default": true, "description": "Whether it runs.",
                    "title": "On", "unit": null, "advanced": false, "cyclic": false},
            })
        );
        Ok(())
    }

    #[test]
    fn a_rule_between_fields_runs_after_the_field_rules_and_its_problems_are_capped() -> Result<(), ParamError> {
        // refuses a value of 1.5 or more, once for each of 150 imaginary findings
        fn rule(values: &Values) -> Vec<Problem> {
            let amount = values.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
            if amount < 1.5 {
                return Vec::new();
            }
            (0..150)
                .map(|index| Problem {
                    path: "amount".to_owned(),
                    rule: format!("finding {index}"),
                })
                .collect()
        }
        let plain = ParamTable::new(vec![amount()])?;
        let ruled = plain.clone().with_rule(rule);
        assert_ne!(plain, ruled);
        assert_eq!(
            plain.parse(&json!({"amount": 1.75}))?.into_json(),
            json!({"amount": 1.75})
        );
        assert_eq!(
            ruled.parse(&json!({"amount": 1.25}))?.into_json(),
            json!({"amount": 1.25})
        );
        let Err(refused) = ruled.parse(&json!({"amount": 1.75})) else {
            return Err(ParamError::at("amount", "the rule was not run".to_owned()));
        };
        assert_eq!((refused.problems.len(), refused.omitted()), (100, 50));
        assert_eq!(refused.problems[0].rule, "finding 0");
        // a field that breaks its own rule (past 2.0) is reported alone: the rule never sees the invalid value
        let Err(broken) = ruled.parse(&json!({"amount": 2.5})) else {
            return Err(ParamError::at("amount", "accepted".to_owned()));
        };
        check_problems("broken field", &broken.problems, &[("amount", "<= 2.0")]);
        Ok(())
    }

    #[test]
    fn a_numbers_range_reads_as_help_prints_it() {
        let number = |kind: Kind, min: f64, max: f64| Param {
            kind,
            min: Some(min),
            max: Some(max),
            unit: Some(ParamUnit::Ratio),
            ..amount()
        };
        // expected: Python's `format(x, "g")` for decimals (computed there), whole numbers in full
        for (kind, min, max, expected) in [
            (Kind::Float, 0.0, 1.0, "0 to 1"),
            (Kind::Float, 0.5, 3600.0, "0.5 to 3600"),
            (Kind::Float, 0.001, 48000.0, "0.001 to 48000"),
            (Kind::Float, -1e6, 1e6, "-1e+06 to 1e+06"),
            (Kind::Float, 0.0, 123_456.0, "0 to 123456"),
            (Kind::Float, 0.0, 1_234_567.0, "0 to 1.23457e+06"),
            (Kind::Float, 0.0001, 12345.678, "0.0001 to 12345.7"),
            (Kind::Float, 1e-5, 2.5e-7, "1e-05 to 2.5e-07"),
            (Kind::Float, 0.01, 999_999.5, "0.01 to 1e+06"),
            (Kind::Float, -0.25, 0.99, "-0.25 to 0.99"),
            (Kind::Whole, 0.0, 4_294_967_295.0, "0 to 4294967295"),
            (Kind::Whole, 1.0, 64.0, "1 to 64"),
            (Kind::Whole, -1_000_000.0, 1_000_000.0, "-1000000 to 1000000"),
        ] {
            assert_eq!(
                number(kind, min, max).range_text().as_deref(),
                Some(expected),
                "{min} {max}"
            );
        }
        for kind in [Kind::Switch, Kind::Text, choices(&["a", "b"])] {
            assert_eq!(plain("x", kind.clone(), json!(null)).range_text(), None, "{kind:?}");
        }
    }

    /// A valid field `f<index>` of any kind, its default inside its range.
    fn any_param(index: usize) -> impl Strategy<Value = Param> {
        let name = format!("f{index}");
        prop_oneof![
            (-1e6..1e6_f64, 0.0..1e6_f64, 0.0..=1.0_f64).prop_map({
                let name = name.clone();
                move |(min, width, at)| Param {
                    name: name.clone(),
                    default: json!((min + width * at).clamp(min, min + width)),
                    min: Some(min),
                    max: Some(min + width),
                    ..amount()
                }
            }),
            (-1_000_000_i64..1_000_000, 0_i64..1_000_000, 0.0..=1.0_f64).prop_map({
                let name = name.clone();
                #[expect(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    reason = "small numbers"
                )]
                move |(min, width, at)| Param {
                    name: name.clone(),
                    kind: Kind::Whole,
                    default: json!(min + (width as f64 * at) as i64),
                    unit: Some(ParamUnit::Count),
                    min: Some(min as f64),
                    max: Some((min + width) as f64),
                    ..amount()
                }
            }),
            any::<bool>().prop_map({
                let name = name.clone();
                move |on| plain(&name, Kind::Switch, json!(on))
            }),
            (1_usize..4, 0_usize..4).prop_map(move |(count, at)| {
                let names: Vec<String> = (0..count).map(|choice| format!("c{choice}")).collect();
                let default = json!(names[at % count]);
                plain(&name, Kind::Choice(names), default)
            }),
        ]
    }

    fn any_table() -> impl Strategy<Value = Vec<Param>> {
        (0_usize..6).prop_flat_map(|count| (0..count).map(any_param).collect::<Vec<_>>())
    }

    proptest! {
        #[test]
        fn the_defaults_a_table_describes_are_accepted_as_they_are(params in any_table()) {
            let table = ParamTable::new(params).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let described = table.describe();
            let defaults: serde_json::Map<String, Value> = table
                .params()
                .iter()
                .map(|param| (param.name.clone(), described[&param.name]["default"].clone()))
                .collect();
            let given = Value::Object(defaults);
            let parsed = table.parse(&given).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(parsed.into_json(), given);
        }

        #[test]
        fn a_cyclic_number_lands_in_its_range_at_the_same_position(
            min in -1e3..1e3_f64,
            given in prop_oneof![-1e9..1e9_f64, any::<f64>().prop_filter("finite", |value| value.is_finite())],
        ) {
            let table = ParamTable::new(vec![phase("phase", min)]).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let values = table.parse(&json!({"phase": given})).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let wrapped = values.get("phase").and_then(Value::as_f64).unwrap_or(f64::NAN);
            prop_assert!(min <= wrapped && wrapped <= min + 1.0, "{given} on {min}: {wrapped}");
            if min <= given && given <= min + 1.0 {
                prop_assert_eq!(values.get("phase"), Some(&json!(given)));
            }
            if given.abs() < 1e6 {
                // whole cycles apart: the same position
                let cycles = wrapped - given;
                prop_assert!((cycles - cycles.round()).abs() < 1e-6, "{given} on {min}: {wrapped}");
            }
        }

        #[test]
        fn a_whole_number_lands_in_a_cyclic_range_whole_cycles_from_its_start(min in -1e3..1e3_f64, given in any::<i64>()) {
            let table = ParamTable::new(vec![phase("phase", min)]).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let values = table.parse(&json!({"phase": given})).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let wrapped = values.get("phase").and_then(Value::as_f64).unwrap_or(f64::NAN);
            prop_assert!(min <= wrapped && wrapped <= min + 1.0, "{given} on {min}: {wrapped}");
            // an integer is whole cycles from 0, so it lands on a whole number (to the rounding of `min`'s fraction)
            prop_assert!((wrapped - wrapped.round()).abs() < 1e-9, "{given} on {min}: {wrapped}");
            #[expect(clippy::cast_precision_loss, reason = "compared only when small, where it is exact")]
            let small = (given.unsigned_abs() < 1 << 20).then_some(given as f64);
            if let Some(given) = small.filter(|given| min <= *given && *given <= min + 1.0) {
                prop_assert_eq!(values.get("phase"), Some(&json!(given)));
            }
        }
    }
}
