//! Shared signature and inferred compose metadata (golden path M3).
//! Native views borrow static descriptions; an owned `Signature<'static>`
//! can describe a script without leaking strings or parameter declarations.

use std::borrow::Cow;

use crate::describe::{Forms, InputDesc, OutputDesc, Params, Requirement, ShardDesc};
use crate::diagnostic::{PathStep, json_str};
use crate::{Type, Var};

/// Trusted effects of a native operation, or the union for a compiled body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effects {
  pub suspends: bool,
  pub io: bool,
  pub time: bool,
  pub random: bool,
  pub unknown: bool,
}

impl Effects {
  pub const NONE: Self = Self {
    suspends: false,
    io: false,
    time: false,
    random: false,
    unknown: false,
  };
  pub const UNKNOWN: Self = Self {
    unknown: true,
    ..Self::NONE
  };
  pub const IO: Self = Self {
    io: true,
    ..Self::NONE
  };
  pub const TIME: Self = Self {
    time: true,
    ..Self::NONE
  };
  pub const WAIT: Self = Self {
    suspends: true,
    time: true,
    ..Self::NONE
  };
  pub const IO_WAIT: Self = Self {
    suspends: true,
    io: true,
    time: true,
    ..Self::NONE
  };

  pub const fn union(self, other: Self) -> Self {
    Self {
      suspends: self.suspends || other.suspends,
      io: self.io || other.io,
      time: self.time || other.time,
      random: self.random || other.random,
      unknown: self.unknown || other.unknown,
    }
  }

  pub fn to_json(self) -> String {
    format!(
      "{{\"suspends\":{},\"io\":{},\"time\":{},\"random\":{},\"unknown\":{}}}",
      self.suspends, self.io, self.time, self.random, self.unknown
    )
  }
}

impl Default for Effects {
  fn default() -> Self {
    Self::UNKNOWN
  }
}

/// Persistent semantic state, not temporary state used while suspended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lifetime {
  Stateless,
  Stateful,
  #[default]
  Unknown,
}

impl Lifetime {
  pub fn name(self) -> &'static str {
    match self {
      Self::Stateless => "stateless",
      Self::Stateful => "stateful",
      Self::Unknown => "unknown",
    }
  }
  pub fn union(self, other: Self) -> Self {
    match (self, other) {
      (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
      (Self::Stateful, _) | (_, Self::Stateful) => Self::Stateful,
      _ => Self::Stateless,
    }
  }
}

#[derive(Clone, Debug)]
pub enum SignatureInput {
  Ignored,
  Type(Type),
}
#[derive(Clone, Debug)]
pub enum SignatureOutput {
  Type(Type),
  Passthrough,
  SameAsInput,
  Dynamic,
}
#[derive(Clone, Debug)]
pub enum ParameterRequirement {
  Required,
  Optional,
  Default(Var),
  Variadic,
}
#[derive(Clone, Debug)]
pub struct Parameter<'a> {
  pub name: Cow<'a, str>,
  pub help: Cow<'a, str>,
  pub forms: Forms,
  pub ty: Type,
  pub requirement: ParameterRequirement,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshAccess {
  pub name: String,
  pub ty: Type,
}
#[derive(Clone, Debug)]
pub struct DefinitionSource<'a> {
  pub file: Cow<'a, str>,
  pub line: u32,
  pub column: u32,
}
#[derive(Clone, Debug)]
pub struct Signature<'a> {
  pub name: Cow<'a, str>,
  pub revision: u32,
  pub input: SignatureInput,
  pub output: SignatureOutput,
  /// None means the host has not declared its parameter contract.
  pub params: Option<Vec<Parameter<'a>>>,
  pub lifetime: Lifetime,
  pub effects: Effects,
  /// Native requirements whose names depend on arguments are inferred by compose.
  pub uses: Vec<MeshAccess>,
  pub mutates: Vec<MeshAccess>,
  pub source: Option<DefinitionSource<'a>>,
  pub summary: Cow<'a, str>,
  pub help: Cow<'a, str>,
}

impl ShardDesc {
  pub fn signature(&self) -> Signature<'_> {
    Signature {
      name: self.name.into(),
      revision: self.version,
      input: match self.input {
        InputDesc::Ignored => SignatureInput::Ignored,
        InputDesc::Any => SignatureInput::Type(Type::any()),
        InputDesc::Types(ts) => SignatureInput::Type(Type::union(
          ts.iter().map(|t| t.to_type()).collect::<Vec<_>>(),
        )),
        InputDesc::Typed(t) => SignatureInput::Type(t()),
      },
      output: match self.output {
        OutputDesc::Fixed(t) => SignatureOutput::Type(t.to_type()),
        OutputDesc::Passthrough => SignatureOutput::Passthrough,
        OutputDesc::SameAsInput => SignatureOutput::SameAsInput,
        OutputDesc::Dynamic(_) => SignatureOutput::Dynamic,
      },
      params: match self.params {
        Params::Undeclared => None,
        Params::Declared(ps) => Some(
          ps.iter()
            .map(|p| Parameter {
              name: p.name.into(),
              help: p.help.into(),
              forms: p.forms,
              ty: p.ty.map_or_else(
                || {
                  if p.types.is_empty() {
                    Type::any()
                  } else {
                    Type::union(p.types.iter().map(|t| t.to_type()).collect::<Vec<_>>())
                  }
                },
                |t| t(),
              ),
              requirement: match p.requirement {
                Requirement::Required => ParameterRequirement::Required,
                Requirement::Optional => ParameterRequirement::Optional,
                Requirement::Default(d) => ParameterRequirement::Default(d.to_var()),
                Requirement::Variadic => ParameterRequirement::Variadic,
              },
            })
            .collect(),
        ),
      },
      lifetime: self.lifetime,
      effects: self.effects,
      uses: Vec::new(),
      mutates: Vec::new(),
      source: None,
      summary: self.summary.into(),
      help: self.help.into(),
    }
  }
}

/// One successfully composed occurrence. Paths are semantic, never source locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
  pub path: Vec<PathStep>,
  pub input: Type,
  pub output: Type,
  pub effects: Effects,
  pub lifetime: Lifetime,
}

/// Immutable analysis shared with compiled code. A caller prefixes the relative
/// paths when incorporating a cached callee, preserving every use site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
  pub effects: Effects,
  pub lifetime: Lifetime,
  pub uses: Vec<MeshAccess>,
  pub mutates: Vec<MeshAccess>,
  pub occurrences: Vec<Occurrence>,
}
impl Default for Analysis {
  fn default() -> Self {
    Self {
      effects: Effects::NONE,
      lifetime: Lifetime::Stateless,
      uses: Vec::new(),
      mutates: Vec::new(),
      occurrences: Vec::new(),
    }
  }
}
impl Analysis {
  pub(crate) fn access(&mut self, name: &str, ty: Type, write: bool) {
    let target = if write {
      &mut self.mutates
    } else {
      &mut self.uses
    };
    let access = MeshAccess {
      name: name.into(),
      ty,
    };
    if !target.contains(&access) {
      target.push(access);
      target.sort_by(|a, b| a.name.cmp(&b.name));
    }
  }
  pub(crate) fn include(&mut self, child: &Self, prefix: &[PathStep]) {
    self.effects = self.effects.union(child.effects);
    self.lifetime = self.lifetime.union(child.lifetime);
    for access in &child.uses {
      self.access(&access.name, access.ty, false);
    }
    for access in &child.mutates {
      self.access(&access.name, access.ty, true);
    }
    self
      .occurrences
      .extend(child.occurrences.iter().cloned().map(|mut o| {
        o.path.splice(0..0, prefix.iter().cloned());
        o
      }));
  }
  pub fn mesh_json(accesses: &[MeshAccess]) -> String {
    format!(
      "[{}]",
      accesses
        .iter()
        .map(|a| format!(
          "{{\"name\":{},\"type\":{}}}",
          json_str(&a.name),
          json_str(&a.ty.to_string())
        ))
        .collect::<Vec<_>>()
        .join(",")
    )
  }
}

impl Signature<'_> {
  /// The same representation for native and owned script signatures.
  pub fn to_json(&self) -> String {
    let input = match self.input {
      SignatureInput::Ignored => "{\"kind\":\"ignored\"}".into(),
      SignatureInput::Type(t) => format!(
        "{{\"kind\":\"type\",\"type\":{}}}",
        json_str(&t.to_string())
      ),
    };
    let output = match self.output {
      SignatureOutput::Type(t) => format!(
        "{{\"kind\":\"type\",\"type\":{}}}",
        json_str(&t.to_string())
      ),
      SignatureOutput::Passthrough => "{\"kind\":\"passthrough\"}".into(),
      SignatureOutput::SameAsInput => "{\"kind\":\"same-as-input\"}".into(),
      SignatureOutput::Dynamic => "{\"kind\":\"dynamic\"}".into(),
    };
    let params = self.params.as_ref().map_or("null".into(), |ps| {
      format!(
        "[{}]",
        ps.iter()
          .map(|p| {
            let (requirement, default) = match &p.requirement {
              ParameterRequirement::Required => ("required", String::new()),
              ParameterRequirement::Optional => ("optional", String::new()),
              ParameterRequirement::Variadic => ("variadic", String::new()),
              ParameterRequirement::Default(v) => (
                "default",
                format!(
                  ",\"default\":{{\"type\":{},\"literal\":{}}}",
                  json_str(&v.type_of().to_string()),
                  json_str(&v.to_string())
                ),
              ),
            };
            format!(
              "{{\"name\":{},\"type\":{},\"forms\":[{}],\"requirement\":{}{default},\"help\":{}}}",
              json_str(&p.name),
              json_str(&p.ty.to_string()),
              p.forms
                .names()
                .iter()
                .map(|s| json_str(s))
                .collect::<Vec<_>>()
                .join(","),
              json_str(requirement),
              json_str(&p.help)
            )
          })
          .collect::<Vec<_>>()
          .join(",")
      )
    });
    let source = self.source.as_ref().map_or("null".into(), |s| {
      format!(
        "{{\"file\":{},\"line\":{},\"column\":{}}}",
        json_str(&s.file),
        s.line,
        s.column
      )
    });
    format!(
      "{{\"name\":{},\"revision\":{},\"input\":{input},\"output\":{output},\"params\":{params},\"lifetime\":{},\"effects\":{},\"uses\":{},\"mutates\":{},\"source\":{source},\"summary\":{},\"help\":{}}}",
      json_str(&self.name),
      self.revision,
      json_str(self.lifetime.name()),
      self.effects.to_json(),
      Analysis::mesh_json(&self.uses),
      Analysis::mesh_json(&self.mutates),
      json_str(&self.summary),
      json_str(&self.help)
    )
  }
}
