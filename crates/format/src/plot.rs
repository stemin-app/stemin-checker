//! The `plot` block: one fence, and everything that moves.
//!
//! A plot is a set of marks drawn against two axes, where **every coordinate is
//! an expression** in the free variable and in the named inputs. The app draws
//! it as SVG and re-evaluates it as the learner drags a slider, so a labelled
//! point tracks its sliders (CONTENT-MODEL.md §10.1).
//!
//! This module holds the **shape** of the block and nothing that draws it, so
//! `stemin check` can refuse a malformed plot in CI and the app can draw the
//! same spec in the browser.

use serde::{Deserialize, Serialize};

/// One `plot` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plot {
    /// The horizontal axis. It carries the free variable a curve is drawn
    /// against.
    pub x: Axis,
    /// The vertical axis.
    pub y: Axis,
    /// The sliders. With none of them the plot is still.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Input>,
    /// Named sub-expressions, **in order**: each one sees the ones before it.
    #[serde(
        default,
        rename = "let",
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "ordered_pairs",
        serialize_with = "as_map"
    )]
    pub lets: Vec<(String, Expr)>,
    /// The marks, back to front.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draw: Vec<Mark>,
}

impl Plot {
    /// The free variable every curve is a function of. Declared on `x`.
    #[must_use]
    pub fn var(&self) -> &str {
        self.x.var.as_deref().unwrap_or("x")
    }
}

/// One axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Axis {
    /// The free variable a curve is a function of. On `x` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub var: Option<String>,
    /// The axis text. Takes `$math$`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The extent.
    pub from: f64,
    pub to: f64,
    /// `linear`, or `log`, which is what makes a Bode plot possible.
    #[serde(default, skip_serializing_if = "Scale::is_linear")]
    pub scale: Scale,
    /// The tick spacing, or absent for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticks: Option<f64>,
    /// Faint rules at the ticks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub grid: bool,
}

/// How an axis maps a value to a position.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    #[default]
    Linear,
    Log,
}

impl Scale {
    /// Whether this is the default, so it need not be written out.
    #[must_use]
    pub const fn is_linear(&self) -> bool {
        matches!(*self, Self::Linear)
    }
}

/// One slider: a named value every expression may use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    /// The identifier the expressions use.
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    #[serde(default = "one")]
    pub step: f64,
    /// What the learner reads. The name, where it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The default slider step.
const fn one() -> f64 {
    1.0
}

/// The three style keys every mark carries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Style {
    /// The one colour beside the ink that a figure may carry.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accent: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dash: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One mark.
///
/// The YAML is a **single-key map**, `- curve: …`. Serde's own external tagging
/// would write `!curve` in YAML and `{"curve": …}` in JSON, so the two forms
/// would drift and a bundle would not read back the way an author wrote it.
/// [`Mark`] therefore reads and writes the one-key map itself, in both formats.
#[derive(Debug, Clone)]
pub enum Mark {
    /// `y = f(x)`.
    Curve(Curve),
    /// A parametric pair, `(x(s), y(s))`.
    Param(Param),
    /// `r(θ)`.
    Polar(Polar),
    /// One marked, optionally labelled point.
    Point(Point),
    /// A data series.
    Points(Points),
    /// A horizontal reference line.
    Hline(Rule),
    /// A vertical reference line.
    Vline(Rule),
    /// A filled region.
    Area(Area),
}

impl Mark {
    /// The word that names this mark in the file.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Curve(_) => "curve",
            Self::Param(_) => "param",
            Self::Polar(_) => "polar",
            Self::Point(_) => "point",
            Self::Points(_) => "points",
            Self::Hline(_) => "hline",
            Self::Vline(_) => "vline",
            Self::Area(_) => "area",
        }
    }

    /// The style keys this mark carries.
    #[must_use]
    pub const fn style(&self) -> &Style {
        match self {
            Self::Curve(m) => &m.style,
            Self::Param(m) => &m.style,
            Self::Polar(m) => &m.style,
            Self::Point(m) => &m.style,
            Self::Points(m) => &m.style,
            Self::Hline(m) | Self::Vline(m) => &m.style,
            Self::Area(m) => &m.style,
        }
    }
}

impl Serialize for Mark {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        let mut map = serializer.serialize_map(Some(1))?;
        let key = self.as_str();
        match self {
            Self::Curve(m) => map.serialize_entry(key, m)?,
            Self::Param(m) => map.serialize_entry(key, m)?,
            Self::Polar(m) => map.serialize_entry(key, m)?,
            Self::Point(m) => map.serialize_entry(key, m)?,
            Self::Points(m) => map.serialize_entry(key, m)?,
            Self::Hline(m) | Self::Vline(m) => map.serialize_entry(key, m)?,
            Self::Area(m) => map.serialize_entry(key, m)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Mark {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OneKey;
        impl<'de> serde::de::Visitor<'de> for OneKey {
            type Value = Mark;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("one mark: curve, param, polar, point, points, hline, vline or area")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Mark, M::Error> {
                let Some(key) = map.next_key::<String>()? else {
                    return Err(serde::de::Error::custom("a mark with no name"));
                };
                let mark = match key.as_str() {
                    "curve" => Mark::Curve(map.next_value()?),
                    "param" => Mark::Param(map.next_value()?),
                    "polar" => Mark::Polar(map.next_value()?),
                    "point" => Mark::Point(map.next_value()?),
                    "points" => Mark::Points(map.next_value()?),
                    "hline" => Mark::Hline(map.next_value()?),
                    "vline" => Mark::Vline(map.next_value()?),
                    "area" => Mark::Area(map.next_value()?),
                    other => {
                        return Err(serde::de::Error::unknown_variant(
                            other,
                            &[
                                "curve", "param", "polar", "point", "points", "hline", "vline",
                                "area",
                            ],
                        ));
                    }
                };
                // A mark is one thing. Two keys in one list item is an author
                // meaning two marks and getting one.
                if map.next_key::<String>()?.is_some() {
                    return Err(serde::de::Error::custom(
                        "one mark per list item: start the second with its own `-`",
                    ));
                }
                Ok(mark)
            }
        }
        deserializer.deserialize_map(OneKey)
    }
}

/// `y = f(x)`, over the whole axis or a part of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "CurveRepr")]
pub struct Curve {
    /// The expression, in the free variable.
    pub is: Expr,
    /// The extent this mark is drawn across. The x axis, where it is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over: Option<Span>,
    #[serde(flatten)]
    pub style: Style,
}

/// The two ways to write a curve: the expression alone, or the full map.
#[derive(Deserialize)]
#[serde(untagged)]
enum CurveRepr {
    Short(Expr),
    Long {
        is: Expr,
        #[serde(default)]
        over: Option<Span>,
        #[serde(flatten)]
        style: Style,
    },
}

impl From<CurveRepr> for Curve {
    fn from(repr: CurveRepr) -> Self {
        match repr {
            CurveRepr::Short(is) => Self {
                is,
                over: None,
                style: Style::default(),
            },
            CurveRepr::Long { is, over, style } => Self { is, over, style },
        }
    }
}

/// A parametric pair, swept over its own variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    /// The parameter's name, `s` by default.
    #[serde(default = "param_var")]
    pub var: String,
    pub over: Span,
    pub x: Expr,
    pub y: Expr,
    #[serde(flatten)]
    pub style: Style,
}

/// The parameter a `param` or a `polar` mark sweeps, where it names none.
fn param_var() -> String {
    "s".to_owned()
}

/// `r(θ)`, swept over an angle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Polar {
    #[serde(default = "param_var")]
    pub var: String,
    pub over: Span,
    pub r: Expr,
    #[serde(flatten)]
    pub style: Style,
}

/// One point, at a pair of expressions, so it moves with the sliders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub at: [Expr; 2],
    #[serde(flatten)]
    pub style: Style,
}

/// A data series: plain numbers, because a series is data rather than a
/// function.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "PointsRepr")]
pub struct Points {
    pub at: Vec<[f64; 2]>,
    #[serde(flatten)]
    pub style: Style,
}

/// The two ways to write a series: the bare list, or a map that styles it.
#[derive(Deserialize)]
#[serde(untagged)]
enum PointsRepr {
    Bare(Vec<[f64; 2]>),
    Styled {
        at: Vec<[f64; 2]>,
        #[serde(flatten)]
        style: Style,
    },
}

impl From<PointsRepr> for Points {
    fn from(repr: PointsRepr) -> Self {
        match repr {
            PointsRepr::Bare(at) => Self {
                at,
                style: Style::default(),
            },
            PointsRepr::Styled { at, style } => Self { at, style },
        }
    }
}

/// A reference line, horizontal or vertical, at one expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RuleRepr")]
pub struct Rule {
    pub at: Expr,
    #[serde(flatten)]
    pub style: Style,
}

/// The two ways to write a rule: the value alone, or the full map.
#[derive(Deserialize)]
#[serde(untagged)]
enum RuleRepr {
    Short(Expr),
    Long {
        at: Expr,
        #[serde(flatten)]
        style: Style,
    },
}

impl From<RuleRepr> for Rule {
    fn from(repr: RuleRepr) -> Self {
        match repr {
            RuleRepr::Short(at) => Self {
                at,
                style: Style::default(),
            },
            RuleRepr::Long { at, style } => Self { at, style },
        }
    }
}

/// A filled region: under one curve, or between two.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Area {
    /// The curve to fill under, down to the axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub under: Option<Expr>,
    /// The two curves to fill between.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<[Expr; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub over: Option<Span>,
    #[serde(flatten)]
    pub style: Style,
}

/// The extent a mark is drawn across. Both ends are expressions, so an extent
/// may follow a slider (`over: [0, 1 / (2 * f)]`).
pub type Span = [Expr; 2];

/// One expression, in the free variable and the named inputs.
///
/// It holds the source text, never a parsed tree: the app evaluates it with
/// `fasteval` and this crate carries no evaluator, so the checker stays small.
/// A bare number is an expression too, so `at: 0.5` and `at: "1/(2*f)"` are the
/// same kind of thing to an author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Expr(pub String);

impl Expr {
    /// The expression's source.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Expr {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AnyScalar;
        impl serde::de::Visitor<'_> for AnyScalar {
            type Value = Expr;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an expression, or a number")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Expr, E> {
                Ok(Expr(value.to_owned()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Expr, E> {
                Ok(Expr(value.to_string()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Expr, E> {
                Ok(Expr(value.to_string()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Expr, E> {
                Ok(Expr(value.to_string()))
            }
        }
        deserializer.deserialize_any(AnyScalar)
    }
}

/// Read a YAML mapping as ordered pairs.
///
/// `let` is a mapping in the file and a **sequence** here, because each binding
/// sees the ones before it. A `BTreeMap` would sort them and quietly break that
/// rule.
fn ordered_pairs<'de, D>(deserializer: D) -> Result<Vec<(String, Expr)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Pairs;
    impl<'de> serde::de::Visitor<'de> for Pairs {
        type Value = Vec<(String, Expr)>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a mapping of names to expressions")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut out = Vec::new();
            while let Some((name, expr)) = map.next_entry::<String, Expr>()? {
                out.push((name, expr));
            }
            Ok(out)
        }
    }
    deserializer.deserialize_map(Pairs)
}

/// Write ordered pairs back as a mapping.
///
/// Without this the pairs would be written as a list and read as a mapping, so
/// a plot with a `let` in it would compile and then fail to load out of the
/// artifact. The two halves have to agree.
fn as_map<S>(pairs: &[(String, Expr)], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap as _;
    let mut map = serializer.serialize_map(Some(pairs.len()))?;
    for (name, expr) in pairs {
        map.serialize_entry(name, expr)?;
    }
    map.end()
}

/// The most sliders one plot carries.
pub const MAX_INPUTS: usize = 16;
/// The most `let` bindings one plot carries.
pub const MAX_LETS: usize = 32;
/// The most marks one plot draws.
pub const MAX_MARKS: usize = 32;
/// The most data points one plot holds, over all of its `points` marks.
pub const MAX_POINTS: usize = 1000;
/// The longest expression, in bytes. The app evaluates each one at every
/// sample, again at each move of a slider.
pub const MAX_EXPR: usize = 512;

/// Parse one `plot` block's body, and hold it to the limits a learner's
/// browser can draw.
///
/// # Errors
/// Returns the reader's message, or the limit the plot breaks, for the author
/// to read in CI.
pub fn parse(body: &str) -> Result<Plot, String> {
    let plot: Plot = serde_yaml::from_str(body).map_err(|err| err.to_string())?;
    limit(&plot)?;
    Ok(plot)
}

/// Hold a plot to the limits: every number finite, every step and every tick
/// spacing above zero, and the counts and the expressions within their caps.
/// The app draws a plot in the learner's browser, so a plot past these is one
/// that would draw nothing, or hang the page.
fn limit(plot: &Plot) -> Result<(), String> {
    for (name, axis) in [("x", &plot.x), ("y", &plot.y)] {
        finite(&format!("`{name}.from`"), axis.from)?;
        finite(&format!("`{name}.to`"), axis.to)?;
        if let Some(ticks) = axis.ticks {
            positive(&format!("`{name}.ticks`"), ticks)?;
        }
    }
    count("sliders in `inputs`", plot.inputs.len(), MAX_INPUTS)?;
    for input in &plot.inputs {
        let name = &input.name;
        finite(&format!("the `min` of input `{name}`"), input.min)?;
        finite(&format!("the `max` of input `{name}`"), input.max)?;
        finite(&format!("the `default` of input `{name}`"), input.default)?;
        positive(&format!("the `step` of input `{name}`"), input.step)?;
    }
    count("bindings in `let`", plot.lets.len(), MAX_LETS)?;
    for (_, expr) in &plot.lets {
        expression(expr)?;
    }
    count("marks in `draw`", plot.draw.len(), MAX_MARKS)?;
    let mut points: usize = 0;
    for mark in &plot.draw {
        if let Mark::Points(series) = mark {
            points = points.saturating_add(series.at.len());
            for pair in &series.at {
                for value in pair {
                    finite("a value in a `points` mark", *value)?;
                }
            }
        }
        for expr in expressions(mark) {
            expression(expr)?;
        }
    }
    count("data points", points, MAX_POINTS)
}

/// Every expression a mark carries.
fn expressions(mark: &Mark) -> Vec<&Expr> {
    fn span(over: Option<&Span>) -> Vec<&Expr> {
        over.into_iter().flatten().collect()
    }
    match mark {
        Mark::Curve(m) => [vec![&m.is], span(m.over.as_ref())].concat(),
        Mark::Param(m) => [vec![&m.x, &m.y], span(Some(&m.over))].concat(),
        Mark::Polar(m) => [vec![&m.r], span(Some(&m.over))].concat(),
        Mark::Point(m) => m.at.iter().collect(),
        Mark::Points(_) => Vec::new(),
        Mark::Hline(m) | Mark::Vline(m) => vec![&m.at],
        Mark::Area(m) => [
            m.under.iter().collect(),
            m.between.iter().flatten().collect(),
            span(m.over.as_ref()),
        ]
        .concat(),
    }
}

/// A number that is a number: not `.nan`, not `.inf`.
fn finite(what: &str, value: f64) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!(
            "{what} is `{value}`, and it must be a finite number: write one like `0` or `-1.5`"
        ))
    }
}

/// A spacing, which must be finite and above zero.
fn positive(what: &str, value: f64) -> Result<(), String> {
    finite(what, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(format!(
            "{what} is `{value}`, and a spacing must be above zero: write one like `0.1`"
        ))
    }
}

/// A count within its cap.
fn count(what: &str, found: usize, cap: usize) -> Result<(), String> {
    if found > cap {
        Err(format!(
            "this plot holds {found} {what}, and a plot holds {cap} at most: \
             split it into more plots"
        ))
    } else {
        Ok(())
    }
}

/// An expression within its length cap.
fn expression(expr: &Expr) -> Result<(), String> {
    let length = expr.as_str().len();
    if length > MAX_EXPR {
        Err(format!(
            "an expression is {length} bytes long, and one is {MAX_EXPR} at most: \
             name its parts in `let`"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Mark, Scale, parse};

    /// The worked example from CONTENT-MODEL.md §10.1, whole. If this parses,
    /// an author can write everything the specification shows.
    const WORKED: &str = r#"
x: { var: t, label: "$t$ in seconds", from: 0, to: 10, ticks: 2, grid: true }
y: { label: "$V$", from: -1.2, to: 1.2 }

inputs:
  - { name: f, min: 0.2, max: 2,   default: 0.6,  step: 0.1,  label: frequency }
  - { name: d, min: 0,   max: 0.5, default: 0.2,  step: 0.05, label: damping }

let:
  w:   2 * pi() * f
  env: exp(-d * t)

draw:
  - area:  { under: env * sin(w * t), over: [0, 1 / (2 * f)] }
  - curve: env * sin(w * t)
  - curve: { is: env,  accent: true, dash: true, label: envelope }
  - curve: { is: -env, accent: true, dash: true }
  - hline: { at: 0 }
  - point: { at: [1 / (4 * f), env], label: first peak }
"#;

    #[test]
    fn the_worked_example_parses() {
        let plot = parse(WORKED).expect("the specification's own example parses");
        assert_eq!(plot.var(), "t");
        assert_eq!(plot.inputs.len(), 2);
        assert_eq!(plot.draw.len(), 6);
        assert!(plot.x.grid);
        assert_eq!(plot.x.ticks, Some(2.0));
    }

    /// `let` bindings run in order, and each sees the ones before it. A map
    /// that sorted its keys would evaluate `env` before `w`.
    #[test]
    fn let_bindings_keep_their_order() {
        let plot = parse(WORKED).expect("parses");
        let names: Vec<&str> = plot.lets.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["w", "env"]);
    }

    /// The short form and the long form are one mark.
    #[test]
    fn a_curve_reads_both_ways() {
        let plot = parse(
            "x: { var: x, from: 0, to: 1 }\ny: { from: 0, to: 1 }\n\
             draw:\n  - curve: x\n  - curve: { is: 2 * x, accent: true }\n",
        )
        .expect("parses");
        match (&plot.draw[0], &plot.draw[1]) {
            (Mark::Curve(short), Mark::Curve(long)) => {
                assert_eq!(short.is.as_str(), "x");
                assert!(!short.style.accent);
                assert_eq!(long.is.as_str(), "2 * x");
                assert!(long.style.accent);
            }
            other => panic!("wrong marks: {other:?}"),
        }
    }

    /// Every mark in the table is readable, so §10.1 and this module agree.
    #[test]
    fn every_mark_parses() {
        let plot = parse(
            "x: { var: t, from: 0, to: 10, scale: log }\ny: { from: 0, to: 1 }\n\
             draw:\n\
             \x20 - curve: { is: 1 / t, over: [0.5, 10] }\n\
             \x20 - param: { var: s, over: [0, 2 * pi()], x: cos(s), y: sin(s) }\n\
             \x20 - polar: { var: a, over: [0, 2 * pi()], r: 1 + cos(a) }\n\
             \x20 - point: { at: [2, 1], label: \"$t_0$\" }\n\
             \x20 - points: [[0, 0], [1, 0.8], [2, 1.4]]\n\
             \x20 - hline: { at: 0.707, label: cut, dash: true }\n\
             \x20 - vline: { at: 2.5 }\n\
             \x20 - area: { between: [sin(t), cos(t)], over: [0, 1] }\n",
        )
        .expect("parses");
        assert_eq!(plot.x.scale, Scale::Log);
        assert_eq!(plot.draw.len(), 8);
        assert!(matches!(plot.draw[4], Mark::Points(_)));
        assert!(matches!(plot.draw[6], Mark::Vline(_)));
    }

    /// A number is an expression, so `at: 0.5` and `at: "1 / 2"` are one kind
    /// of thing to an author.
    #[test]
    fn a_number_is_an_expression() {
        let plot =
            parse("x: { var: x, from: 0, to: 1 }\ny: { from: 0, to: 1 }\ndraw:\n  - hline: 0.5\n")
                .expect("parses");
        match &plot.draw[0] {
            Mark::Hline(rule) => assert_eq!(rule.at.as_str(), "0.5"),
            other => panic!("wrong mark: {other:?}"),
        }
    }

    /// A plot is written back the way it was written down. It crosses from the
    /// worker to the device as JSON and comes back out of `IndexedDB` the same
    /// way, so anything that does not round-trip is content that silently
    /// stops drawing.
    #[test]
    fn a_plot_round_trips_in_both_formats() {
        let plot = parse(WORKED).expect("parses");
        let yaml = serde_yaml::to_string(&plot).expect("writes yaml");
        let json = serde_json::to_string(&plot).expect("writes json");
        for text in [yaml, json] {
            let back = parse(&text).expect("reads back");
            assert_eq!(back.var(), "t");
            assert_eq!(back.inputs.len(), 2);
            assert_eq!(back.draw.len(), 6);
            let names: Vec<&str> = back.lets.iter().map(|(name, _)| name.as_str()).collect();
            assert_eq!(names, vec!["w", "env"], "the order is part of the meaning");
        }
    }

    /// A number the app cannot draw is refused, and so is a spacing of zero.
    #[test]
    fn a_number_that_is_not_finite_is_refused() {
        let axes = "y: { from: 0, to: 1 }\n";
        for (body, phrase) in [
            ("x: { from: .nan, to: 1 }\n", "`x.from`"),
            ("x: { from: 0, to: .inf }\n", "`x.to`"),
            ("x: { from: 0, to: 1, ticks: 0 }\n", "`x.ticks`"),
            ("x: { from: 0, to: 1, ticks: -2 }\n", "above zero"),
            (
                "x: { from: 0, to: 1 }\ninputs:\n  - { name: f, min: 0, max: 1, default: 0, step: 0 }\n",
                "the `step` of input `f`",
            ),
            (
                "x: { from: 0, to: 1 }\ninputs:\n  - { name: f, min: -.inf, max: 1, default: 0 }\n",
                "the `min` of input `f`",
            ),
            (
                "x: { from: 0, to: 1 }\ndraw:\n  - points: [[0, .nan]]\n",
                "a value in a `points` mark",
            ),
        ] {
            let why = parse(&format!("{body}{axes}")).expect_err(body);
            assert!(why.contains(phrase), "{body}: {why}");
        }
    }

    /// A plot past a cap would hang the page that draws it.
    #[test]
    fn a_plot_past_a_cap_is_refused() {
        let axes = "x: { from: 0, to: 1 }\ny: { from: 0, to: 1 }\n";
        let marks = "  - hline: 0\n".repeat(super::MAX_MARKS + 1);
        let why = parse(&format!("{axes}draw:\n{marks}")).expect_err("too many marks");
        assert!(why.contains("marks in `draw`"), "{why}");

        let points = vec!["[0, 0]"; super::MAX_POINTS + 1].join(", ");
        let why = parse(&format!("{axes}draw:\n  - points: [{points}]\n")).expect_err("points");
        assert!(why.contains("data points"), "{why}");

        let inputs = "  - { name: a, min: 0, max: 1, default: 0 }\n".repeat(super::MAX_INPUTS + 1);
        let why = parse(&format!("{axes}inputs:\n{inputs}")).expect_err("too many inputs");
        assert!(why.contains("sliders"), "{why}");

        let long = "x + ".repeat(super::MAX_EXPR);
        let why = parse(&format!("{axes}draw:\n  - curve: \"{long}x\"\n")).expect_err("long");
        assert!(why.contains("bytes long"), "{why}");
    }

    #[test]
    fn a_malformed_plot_says_what_is_wrong() {
        assert!(parse("x: { var: t }\n").is_err(), "an axis needs an extent");
        assert!(parse("draw:\n  - wiggle: x\n").is_err(), "no such mark");
    }
}
