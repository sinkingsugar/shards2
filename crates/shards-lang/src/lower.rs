//! Lowering: syntax tree to `WireDef`/`ShardDef`, against a catalog.
//!
//! Lowering resolves names and argument forms (the catalog's parameter
//! declarations decide whether `{}` is a flow or a table, and whether a
//! bare name is a variable or a wire), and records each emitted shard's
//! span in a [`SourceMap`] keyed by its occurrence path, the same path
//! compose diagnostics carry. Spans never enter the definitions, which are
//! compose cache keys.
//!
//! The top level holds declarations (`@wire`, `@mesh`, `@schedule`, `@run`);
//! loose code becomes the `root` wire when there is no `@run`
//! (docs/surface-syntax-review.md §5.10). Constructs that need shards not
//! ported yet are rejected explicitly, with what to write instead.

use std::collections::HashMap;
use std::sync::Arc;

use shards_core::describe::{self, Forms, ParamDecl};
use shards_core::diagnostic::{Diagnostic, PathStep};
use shards_core::shards::data::{SEQ_MAKE, STRING_FORMAT, TABLE_MAKE, TAKE};
use shards_core::shards::{BIND, CONST, GET, SUB};
use shards_core::{Arg, Catalog, ParamValue, ShardDef, Var, WireDef};

use crate::ast::*;
use crate::lexer::AssignOp;
use crate::problem::{Problem, closest};
use crate::source::Span;

/// The name of the wire that holds loose top-level code.
pub const ROOT_WIRE: &str = "root";

/// Where each emitted shard came from, by occurrence path.
#[derive(Default, Debug)]
pub struct SourceMap {
  /// `(wire, steps below the wire)` to the span of the source construct.
  shards: HashMap<(String, Vec<PathStep>), Span>,
  /// Each wire's declaration.
  wires: HashMap<String, Span>,
}

impl SourceMap {
  /// The source span for a compose diagnostic: the shard at the end of its
  /// path, or the closest enclosing one that is known.
  pub fn locate(&self, d: &Diagnostic) -> Option<Span> {
    let k = d
      .path
      .iter()
      .rposition(|s| matches!(s, PathStep::Wire(_)))?;
    let PathStep::Wire(wire) = &d.path[k] else {
      return None;
    };
    let mut steps = d.path[k + 1..].to_vec();
    loop {
      if let Some(span) = self.shards.get(&(wire.clone(), steps.clone())) {
        return Some(*span);
      }
      if steps.pop().is_none() {
        return self.wires.get(wire).copied();
      }
    }
  }
}

#[derive(Debug)]
pub struct MeshDecl {
  pub name: String,
  pub span: Span,
  pub scheduled: Vec<(String, Span)>,
}

#[derive(Debug)]
pub struct RunDecl {
  pub mesh: String,
  pub fps: Option<f64>,
  pub iterations: Option<i64>,
  pub span: Span,
}

/// A lowered program.
#[derive(Debug, Default)]
pub struct Lowered {
  pub wires: Vec<WireDef>,
  pub meshes: Vec<MeshDecl>,
  pub run: Option<RunDecl>,
  /// Set when loose top-level code was wrapped into [`ROOT_WIRE`].
  pub root: bool,
  pub map: SourceMap,
}

struct Lowerer<'a> {
  catalog: &'a Catalog,
  defines: &'a HashMap<String, String>,
  problems: Vec<Problem>,
  map: SourceMap,
  /// The wire being lowered.
  wire: String,
  /// Counter for the temporaries that hold computed values.
  temps: usize,
}

/// Lowers a parsed program. `defines` are the script arguments (`@name`).
pub fn lower(
  program: &Program,
  catalog: &Catalog,
  defines: &HashMap<String, String>,
) -> (Lowered, Vec<Problem>) {
  let mut l = Lowerer {
    catalog,
    defines,
    problems: Vec::new(),
    map: SourceMap::default(),
    wire: String::new(),
    temps: 0,
  };
  let mut out = Lowered::default();
  let mut loose: Vec<&Statement> = Vec::new();
  for stmt in &program.statements {
    match top_level_func(stmt) {
      Some((name, params, block)) => l.declaration(&mut out, name, params, block, stmt),
      None => loose.push(stmt),
    }
  }
  if let Some(first) = loose.first() {
    if out.run.is_some() {
      l.problems.push(
        Problem::construct(
          statement_span(first),
          "generic",
          "code-outside-wire",
          "code outside a wire: with `@run`, the top level only declares".into(),
        )
        .help("put this code in a `@wire(...)` and `@schedule` it on the mesh"),
      );
    } else if out.wires.iter().any(|w| w.name == ROOT_WIRE) {
      l.problems.push(Problem::construct(
        statement_span(first),
        "generic",
        "root-wire-name",
        format!(
          "loose code runs as the `{ROOT_WIRE}` wire, but a wire is already named `{ROOT_WIRE}`"
        ),
      ));
    } else {
      l.wire = ROOT_WIRE.to_string();
      l.temps = 0;
      let span = statement_span(first).to(statement_span(loose[loose.len() - 1]));
      l.map.wires.insert(ROOT_WIRE.into(), span);
      let flow = l.statements(loose.iter().copied(), &[]);
      out.wires.push(WireDef {
        name: ROOT_WIRE.into(),
        looped: false,
        flow,
      });
      out.root = true;
    }
  }
  l.check_references(&out);
  if out.run.is_none()
    && let Some((_, span)) = out.meshes.iter().flat_map(|m| m.scheduled.iter()).next()
  {
    l.problems.push(
      Problem::construct(
        *span,
        "generic",
        "schedule-without-run",
        "`@schedule` has no effect without `@run`".into(),
      )
      .help("add `@run(mesh)` to start the mesh, or remove the schedule"),
    );
  }
  out.map = l.map;
  (out, l.problems)
}

/// A top-level `@name(...)` statement (one block, nothing piped).
fn top_level_func(stmt: &Statement) -> Option<(&Name, Option<&Params>, &Block)> {
  let Statement::Pipeline(pipe) = stmt else {
    return None;
  };
  match &pipe.blocks[0].kind {
    BlockKind::Func { name, params }
      if matches!(
        name.node.as_str(),
        "wire" | "mesh" | "schedule" | "run" | "define" | "template"
      ) =>
    {
      Some((name, params.as_ref(), &pipe.blocks[0]))
    }
    _ => None,
  }
}

fn statement_span(stmt: &Statement) -> Span {
  match stmt {
    Statement::Assign { op_span, var, .. } => op_span.to(var.span),
    Statement::Pipeline(p) => p.span,
  }
}

/// A param's value as a plain name (`main`, or `"main"`).
fn name_of(pipe: &Pipe) -> Option<String> {
  match &pipe.blocks[..] {
    [b] => match &b.kind {
      BlockKind::Var { name, path } if path.is_empty() => Some(name.node.clone()),
      BlockKind::Literal(Literal::String(s)) => Some(s.clone()),
      _ => None,
    },
    _ => None,
  }
}

impl Lowerer<'_> {
  fn problem(&mut self, p: Problem) {
    self.problems.push(p);
  }

  fn declaration(
    &mut self,
    out: &mut Lowered,
    name: &Name,
    params: Option<&Params>,
    block: &Block,
    stmt: &Statement,
  ) {
    if let Statement::Pipeline(pipe) = stmt
      && pipe.blocks.len() > 1
    {
      self.problem(Problem::construct(
        pipe.blocks[1].span,
        "generic",
        "piped-declaration",
        format!(
          "the result of `@{}` cannot be piped: it is a declaration",
          name.node
        ),
      ));
    }
    let empty = Params {
      items: Vec::new(),
      span: block.span,
    };
    let params = params.unwrap_or(&empty);
    let (positional, named) = split_params(params);
    match name.node.as_str() {
      "wire" => self.wire_decl(out, block, &positional, &named),
      "mesh" => match positional.first().and_then(|p| name_of(p)) {
        Some(mesh) if positional.len() == 1 && named.is_empty() => {
          if out.meshes.iter().any(|m| m.name == mesh) {
            self.problem(Problem::construct(
              block.span,
              "generic",
              "duplicate-mesh",
              format!("mesh `{mesh}` is declared twice"),
            ));
          }
          out.meshes.push(MeshDecl {
            name: mesh,
            span: block.span,
            scheduled: Vec::new(),
          });
        }
        _ => self.problem(Problem::construct(
          block.span,
          "generic",
          "declaration",
          "`@mesh` takes one name: `@mesh(main)`".into(),
        )),
      },
      "schedule" => match (
        positional.first().and_then(|p| name_of(p)),
        positional.get(1).and_then(|p| name_of(p)),
      ) {
        (Some(mesh), Some(wire)) if positional.len() == 2 && named.is_empty() => {
          match out.meshes.iter_mut().find(|m| m.name == mesh) {
            Some(m) => m.scheduled.push((wire, block.span)),
            None => self.problem(Problem::construct(
              positional[0].span,
              "generic",
              "unknown-mesh",
              format!("mesh `{mesh}` is not declared (declare it first with `@mesh({mesh})`)"),
            )),
          }
        }
        _ => self.problem(Problem::construct(
          block.span,
          "generic",
          "declaration",
          "`@schedule` takes a mesh and a wire: `@schedule(main my-wire)`".into(),
        )),
      },
      "run" => self.run_decl(out, block, &positional, &named),
      other => self.problem(Problem::construct(
        name.span,
        "generic",
        "unsupported",
        format!("`@{other}` is not supported yet"),
      )),
    }
  }

  fn wire_decl(
    &mut self,
    out: &mut Lowered,
    block: &Block,
    positional: &[&Pipe],
    named: &[(&Name, &Pipe)],
  ) {
    let usage = "`@wire(name { ... } looped: true)`";
    let Some(name) = positional.first().and_then(|p| name_of(p)) else {
      self.problem(Problem::construct(
        block.span,
        "generic",
        "declaration",
        format!("a wire needs a name: {usage}"),
      ));
      return;
    };
    let mut looped = false;
    for (pname, value) in named {
      match (pname.node.as_str(), &value.blocks[..]) {
        ("looped", [b]) if matches!(b.kind, BlockKind::Literal(Literal::Bool(_))) => {
          looped = b.kind == BlockKind::Literal(Literal::Bool(true));
        }
        ("looped", _) => self.problem(
          Problem::construct(
            value.span,
            "generic",
            "declaration",
            "`looped` takes `true` or `false`".into(),
          )
          .param("looped"),
        ),
        (other, _) => self.problem(
          Problem::construct(
            pname.span,
            "generic",
            "unsupported",
            format!("wire option `{other}` is not supported (supported: looped)"),
          )
          .did_you_mean(closest(other, ["looped".to_string()], 1))
          .param(other),
        ),
      }
    }
    if positional.len() > 2 {
      self.problem(Problem::construct(
        positional[2].span,
        "generic",
        "declaration",
        format!("too many arguments: {usage}"),
      ));
    }
    if out.wires.iter().any(|w| w.name == name) {
      self.problem(Problem::construct(
        block.span,
        "generic",
        "duplicate-wire",
        format!("wire `{name}` is declared twice"),
      ));
      return;
    }
    self.wire = name.clone();
    // Temporary identities must be stable when an unrelated wire is edited.
    // Compose gives each temporary Ref occurrence a fresh slot on inlining.
    self.temps = 0;
    self.map.wires.insert(name.clone(), block.span);
    let flow = match positional.get(1).map(|p| &p.blocks[..]) {
      Some([b]) => match &b.kind {
        BlockKind::Flow(stmts) => self.statements(stmts.iter(), &[]),
        BlockKind::EmptyBraces => Vec::new(),
        _ => {
          self.problem(Problem::construct(
            b.span,
            "generic",
            "declaration",
            format!("a wire's body is a flow in braces: {usage}"),
          ));
          Vec::new()
        }
      },
      Some(_) => {
        self.problem(Problem::construct(
          positional[1].span,
          "generic",
          "declaration",
          format!("a wire's body is a flow in braces: {usage}"),
        ));
        Vec::new()
      }
      None => Vec::new(),
    };
    out.wires.push(WireDef { name, looped, flow });
  }

  fn run_decl(
    &mut self,
    out: &mut Lowered,
    block: &Block,
    positional: &[&Pipe],
    named: &[(&Name, &Pipe)],
  ) {
    let Some(mesh) = positional.first().and_then(|p| name_of(p)) else {
      self.problem(Problem::construct(
        block.span,
        "generic",
        "declaration",
        "`@run` takes a mesh: `@run(main fps: 30)`".into(),
      ));
      return;
    };
    if out.run.is_some() {
      self.problem(Problem::construct(
        block.span,
        "generic",
        "declaration",
        "only one `@run` is allowed".into(),
      ));
      return;
    }
    let mut run = RunDecl {
      mesh,
      fps: None,
      iterations: None,
      span: block.span,
    };
    for (pname, value) in named {
      let literal = match &value.blocks[..] {
        [b] => match b.kind {
          BlockKind::Literal(Literal::Int(i)) => Some(i as f64),
          BlockKind::Literal(Literal::Float(f)) => Some(f),
          _ => None,
        },
        _ => None,
      };
      match (pname.node.as_str(), literal) {
        // The frame interval must be a representable duration.
        ("fps", Some(f)) if f > 0.0 && std::time::Duration::try_from_secs_f64(1.0 / f).is_ok() => {
          run.fps = Some(f)
        }
        // Below 2^63, the exact Int range.
        ("iterations", Some(i))
          if i >= 1.0 && i.fract() == 0.0 && i < 9_223_372_036_854_775_808.0 =>
        {
          run.iterations = Some(i as i64)
        }
        ("fps" | "iterations", _) => self.problem(
          Problem::construct(
            value.span,
            "generic",
            "declaration",
            if pname.node == "fps" {
              "`fps` takes a positive number of frames per second".to_string()
            } else {
              "`iterations` takes a positive whole number".to_string()
            },
          )
          .param(&pname.node),
        ),
        (other, _) => self.problem(
          Problem::construct(
            pname.span,
            "generic",
            "unsupported",
            format!("`@run` option `{other}` is not supported (supported: fps, iterations)"),
          )
          .did_you_mean(closest(
            other,
            ["fps".to_string(), "iterations".to_string()],
            2,
          )),
        ),
      }
    }
    out.run = Some(run);
  }

  /// After all declarations: scheduled wires and the run mesh exist.
  fn check_references(&mut self, out: &Lowered) {
    let wires: Vec<String> = out.wires.iter().map(|w| w.name.clone()).collect();
    for mesh in &out.meshes {
      for (wire, span) in &mesh.scheduled {
        if !wires.contains(wire) {
          self.problems.push(
            Problem::construct(
              *span,
              "generic",
              "unknown-wire",
              format!("wire `{wire}` is not declared"),
            )
            .did_you_mean(closest(wire, wires.clone(), 3)),
          );
        }
      }
    }
    if let Some(run) = &out.run
      && !out.meshes.iter().any(|m| m.name == run.mesh)
    {
      self.problems.push(Problem::construct(
        run.span,
        "generic",
        "unknown-mesh",
        format!(
          "mesh `{}` is not declared (`@mesh({})`)",
          run.mesh, run.mesh
        ),
      ));
    }
  }

  /// Lowers statements into a flow; `prefix` is the occurrence path of the
  /// flow below its wire.
  fn statements<'s>(
    &mut self,
    stmts: impl IntoIterator<Item = &'s Statement>,
    prefix: &[PathStep],
  ) -> Vec<ShardDef> {
    let mut out = Vec::new();
    self.statements_into(&mut out, stmts, prefix);
    out
  }

  /// Appends lowered statements to a flow.
  fn statements_into<'s>(
    &mut self,
    out: &mut Vec<ShardDef>,
    stmts: impl IntoIterator<Item = &'s Statement>,
    prefix: &[PathStep],
  ) {
    for stmt in stmts {
      match stmt {
        Statement::Assign {
          op: AssignOp::Bind,
          op_span,
          var,
        } => {
          let span = op_span.to(var.span);
          self.emit(
            out,
            prefix,
            span,
            ShardDef::with_args(&BIND, vec![Arg::pos(ParamValue::Var(var.node.clone()))]),
          );
        }
        Statement::Pipeline(pipe) => {
          for block in &pipe.blocks {
            self.block(out, prefix, block);
          }
        }
      }
    }
  }

  fn emit(&mut self, out: &mut Vec<ShardDef>, prefix: &[PathStep], span: Span, def: ShardDef) {
    let mut path = prefix.to_vec();
    path.push(PathStep::Shard {
      index: out.len(),
      name: def.ty.name().to_string(),
    });
    self.map.shards.insert((self.wire.clone(), path), span);
    out.push(def);
  }

  fn unsupported(&mut self, span: Span, message: impl Into<String>) {
    self.problem(Problem::construct(
      span,
      "generic",
      "unsupported",
      message.into(),
    ));
  }

  fn block(&mut self, out: &mut Vec<ShardDef>, prefix: &[PathStep], block: &Block) {
    let span = block.span;
    match &block.kind {
      BlockKind::Shard { name, params } => self.shard(out, prefix, block, name, params.as_ref()),
      BlockKind::Var { name, path } if path.is_empty() => {
        self.emit(
          out,
          prefix,
          span,
          ShardDef::with_args(&GET, vec![Arg::pos(ParamValue::Var(name.node.clone()))]),
        );
      }
      BlockKind::Var { name, path } => {
        self.emit(
          out,
          prefix,
          name.span,
          ShardDef::with_args(&GET, vec![Arg::pos(ParamValue::Var(name.node.clone()))]),
        );
        self.take_path(out, prefix, path);
      }
      // `(a | b)` in a flow runs in place.
      BlockKind::Expr(stmts) => self.statements_into(out, stmts.iter(), prefix),
      BlockKind::Func { name, params: None } => {
        if let Some(v) = self.define(name) {
          self.emit(
            out,
            prefix,
            span,
            ShardDef::with_args(&CONST, vec![Arg::pos(ParamValue::Value(v))]),
          );
        }
      }
      BlockKind::Func {
        name,
        params: Some(_),
      } => {
        let what = if matches!(name.node.as_str(), "wire" | "mesh" | "schedule" | "run") {
          format!(
            "`@{}` is a declaration; it is only allowed at the top level",
            name.node
          )
        } else {
          format!(
            "`@{}(...)` (templates and builtins) is not supported yet",
            name.node
          )
        };
        self.unsupported(span, what);
      }
      BlockKind::Flow(_) => self.unsupported(
        span,
        "a flow in braces is a parameter value (`When({...} {...})`); on its own it does nothing",
      ),
      BlockKind::Eval(_) => self.unsupported(
        span,
        "`#(...)` (evaluation while loading) is not supported yet",
      ),
      BlockKind::Enum(t, v) => {
        self.unsupported(span, format!("enums (`{t}::{v}`) are not supported yet"))
      }
      BlockKind::FString(parts) => {
        // f"a {x} b" is Seq.Make("a " x " b") | String.Format.
        let snapshot = parts.iter().any(|p| match p {
          FPart::Expr(stmts, _) => match &stmts[..] {
            [Statement::Pipeline(pipe)] => !is_plain(pipe),
            _ => true,
          },
          FPart::Text(_) => false,
        });
        let mut items = Vec::new();
        for part in parts {
          match part {
            FPart::Text(t) => items.push(Arg::pos(ParamValue::Value(Var::string(t)))),
            FPart::Expr(stmts, part_span) => {
              match self.operand_of_statements(out, prefix, stmts, *part_span, snapshot) {
                Some(v) => items.push(Arg::pos(v)),
                None => return,
              }
            }
          }
        }
        self.emit(out, prefix, span, ShardDef::with_args(&SEQ_MAKE, items));
        self.emit(
          out,
          prefix,
          span,
          ShardDef::with_args(&STRING_FORMAT, vec![]),
        );
      }
      BlockKind::Seq(items) if !is_constant(block) => {
        let snapshot = items.iter().any(|i| !is_plain(i));
        let mut args = Vec::new();
        for item in items {
          match self.operand(out, prefix, item, snapshot) {
            Some(v) => args.push(Arg::pos(v)),
            None => return,
          }
        }
        self.emit(out, prefix, span, ShardDef::with_args(&SEQ_MAKE, args));
      }
      BlockKind::Table(entries) if !is_constant(block) => {
        let snapshot = entries.iter().any(|(_, v)| !is_plain(v));
        let mut keys = Vec::new();
        let mut args = Vec::new();
        for (key, value) in entries {
          let Some(k) = self.table_key(key, &keys) else {
            return;
          };
          keys.push(k);
          match self.operand(out, prefix, value, snapshot) {
            Some(v) => args.push(Arg::pos(v)),
            None => return,
          }
        }
        let keys = Var::Seq(Arc::new(keys.iter().map(|k| Var::string(k)).collect()));
        args.insert(0, Arg::pos(ParamValue::Value(keys)));
        self.emit(out, prefix, span, ShardDef::with_args(&TABLE_MAKE, args));
      }
      _ => {
        if let Some(v) = self.constant(block) {
          self.emit(
            out,
            prefix,
            span,
            ShardDef::with_args(&CONST, vec![Arg::pos(ParamValue::Value(v))]),
          );
        }
      }
    }
  }

  /// A table key as a string; reports non-string and repeated keys.
  fn table_key(&mut self, key: &Spanned<TableKey>, seen: &[String]) -> Option<String> {
    let text = match &key.node {
      TableKey::Name(s) | TableKey::String(s) => s.clone(),
      TableKey::Int(i) => {
        self.problem(
          Problem::construct(
            key.span,
            "generic",
            "table-key",
            format!("table keys are strings in Shards 2: write `\"{i}\"`"),
          )
          .fix(format!("\"{i}\"")),
        );
        return None;
      }
      TableKey::Value(_) => {
        self.unsupported(
          key.span,
          "table keys are strings in Shards 2 (sequence and value keys are not supported)",
        );
        return None;
      }
    };
    if seen.contains(&text) {
      self.problem(Problem::construct(
        key.span,
        "generic",
        "duplicate-key",
        format!("table key `{text}` appears twice"),
      ));
      return None;
    }
    Some(text)
  }

  /// `.key` / `.0` segments as Take shards.
  fn take_path(&mut self, out: &mut Vec<ShardDef>, prefix: &[PathStep], path: &[Spanned<PathKey>]) {
    for segment in path {
      let key = match &segment.node {
        PathKey::Key(k) => Var::string(k),
        PathKey::Index(i) => Var::Int(*i),
      };
      self.emit(
        out,
        prefix,
        segment.span,
        ShardDef::with_args(&TAKE, vec![Arg::pos(ParamValue::Value(key))]),
      );
    }
  }

  /// A fresh temporary's name. `%` cannot start a name in source, so it
  /// never collides with a user variable.
  fn temp(&mut self) -> String {
    self.temps += 1;
    format!("%{}", self.temps)
  }

  /// Emits `SubFlow({ <computation> = %n })` and returns `%n`. The
  /// computation runs on every activation right before the shard that uses
  /// the value, and the input flows past it unchanged.
  fn hoist(
    &mut self,
    out: &mut Vec<ShardDef>,
    prefix: &[PathStep],
    span: Span,
    lower: impl FnOnce(&mut Self, &mut Vec<ShardDef>, &[PathStep]),
  ) -> String {
    let name = self.temp();
    let mut inner_prefix = prefix.to_vec();
    inner_prefix.push(PathStep::Shard {
      index: out.len(),
      name: SUB.name().to_string(),
    });
    inner_prefix.push(PathStep::Param("action".into()));
    let mut body = Vec::new();
    lower(self, &mut body, &inner_prefix);
    // A `%` binding declares a fresh slot at each occurrence, so a wire inlined
    // twice by Do (even with different input types) still composes.
    self.emit(
      &mut body,
      &inner_prefix,
      span,
      ShardDef::with_args(&BIND, vec![Arg::pos(ParamValue::Var(name.clone()))]),
    );
    self.emit(
      out,
      prefix,
      span,
      ShardDef::with_args(&SUB, vec![Arg::pos(ParamValue::Flow(body))]),
    );
    name
  }

  /// A literal-or-variable operand for one element: a literal, a plain
  /// variable, or a computed value hoisted into a temporary. With `snapshot`,
  /// a plain variable is hoisted too, so it is read in source order relative
  /// to computed elements (`[n Inc(n)]` reads n before incrementing).
  fn operand(
    &mut self,
    out: &mut Vec<ShardDef>,
    prefix: &[PathStep],
    pipe: &Pipe,
    snapshot: bool,
  ) -> Option<ParamValue> {
    if let [b] = &pipe.blocks[..] {
      match &b.kind {
        BlockKind::Var { name, path } if path.is_empty() && !snapshot => {
          return Some(ParamValue::Var(name.node.clone()));
        }
        _ if is_constant(b) => return self.constant(b).map(ParamValue::Value),
        _ => {}
      }
    }
    let before = self.problems.len();
    let name = self.hoist(out, prefix, pipe.span, |l, body, p| {
      for block in &pipe.blocks {
        l.block(body, p, block);
      }
    });
    (self.problems.len() == before).then_some(ParamValue::Var(name))
  }

  /// Like [`Self::operand`], for statements (an f-string interpolation).
  fn operand_of_statements(
    &mut self,
    out: &mut Vec<ShardDef>,
    prefix: &[PathStep],
    stmts: &[Statement],
    span: Span,
    snapshot: bool,
  ) -> Option<ParamValue> {
    if let [Statement::Pipeline(pipe)] = stmts {
      return self.operand(out, prefix, pipe, snapshot);
    }
    let before = self.problems.len();
    let name = self.hoist(out, prefix, span, |l, body, p| {
      l.statements_into(body, stmts.iter(), p)
    });
    (self.problems.len() == before).then_some(ParamValue::Var(name))
  }

  /// `@name`: a script argument, as a string.
  fn define(&mut self, name: &Name) -> Option<Var> {
    match self.defines.get(&name.node) {
      Some(v) => Some(Var::string(v)),
      None => {
        let known: Vec<String> = self.defines.keys().cloned().collect();
        self.problem(
          Problem::construct(
            name.span,
            "generic",
            "unknown-define",
            format!("no value for `@{0}`: pass it as `{0}:value`", name.node),
          )
          .did_you_mean(closest(&name.node, known, 3)),
        );
        None
      }
    }
  }

  /// A literal value: numbers, strings, `none`, booleans, `@name`, and
  /// sequences and tables of them. Reports anything else.
  fn constant(&mut self, block: &Block) -> Option<Var> {
    Some(match &block.kind {
      BlockKind::Literal(Literal::None) => Var::None,
      BlockKind::Literal(Literal::Bool(b)) => Var::Bool(*b),
      BlockKind::Literal(Literal::Int(i)) => Var::Int(*i),
      BlockKind::Literal(Literal::Float(f)) => Var::Float(*f),
      BlockKind::Literal(Literal::String(s)) => Var::string(s),
      BlockKind::Func { name, params: None } => self.define(name)?,
      BlockKind::EmptyBraces => Var::table(Vec::<(&str, Var)>::new()),
      BlockKind::Seq(items) => {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
          values.push(self.constant_pipe(item)?);
        }
        Var::Seq(Arc::new(values))
      }
      BlockKind::Table(entries) => {
        let mut values: Vec<(String, Var)> = Vec::with_capacity(entries.len());
        let mut seen: Vec<String> = Vec::new();
        for (key, value) in entries {
          let key = self.table_key(key, &seen)?;
          seen.push(key.clone());
          values.push((key, self.constant_pipe(value)?));
        }
        Var::table(values)
      }
      _ => {
        self.unsupported(block.span, "expected a literal value here");
        return None;
      }
    })
  }

  fn constant_pipe(&mut self, pipe: &Pipe) -> Option<Var> {
    match &pipe.blocks[..] {
      [b] => self.constant(b),
      _ => {
        self.unsupported(
          pipe.span,
          "computed elements in sequences and tables (`[a | F]`) are not supported yet; compute them into variables first",
        );
        None
      }
    }
  }

  fn shard(
    &mut self,
    out: &mut Vec<ShardDef>,
    prefix: &[PathStep],
    block: &Block,
    name: &Name,
    params: Option<&Params>,
  ) {
    let Some(ty) = self.catalog.get(&name.node) else {
      // Suggest aliases too, so `Ad` suggests `Add` as written in scripts.
      let all: Vec<String> = self.catalog.names().iter().map(|n| n.to_string()).collect();
      let suggestions = closest(&name.node, all, 3);
      if let Some(help) = match name.node.as_str() {
        "Set" => Some(
          "declare a mutable variable with `value | Var(name)`, assign it with `value | Update(name)`",
        ),
        "Ref" => Some("bind an immutable name with `value = name`"),
        _ => None,
      } {
        self.problem(
          Problem::construct(
            name.span,
            "unknown-shard",
            "removed-shard",
            format!("`{}` is removed in Shards 2: {help}", name.node),
          )
          .shard(&name.node),
        );
        return;
      }
      if matches!(name.node.as_str(), "And" | "Or") {
        let (word, all) = if name.node == "And" {
          ("And", "All")
        } else {
          ("Or", "Any")
        };
        self.problem(
          Problem::construct(
            name.span,
            "unknown-shard",
            "and-or",
            format!("`{word}` is not a shard in Shards 2: combine conditions with `{all}(...)`"),
          )
          .help(format!("`If({{a | {word} | b}} ...)` becomes `If({all}(a b) ...)`; conditions can be Bool variables or flows"))
          .did_you_mean(vec![all.to_string()])
          .shard(&name.node),
        );
        return;
      }
      let message = if name.node.starts_with(|c: char| c.is_ascii_lowercase()) {
        format!(
          "`{}` is not a shard: shard names start with an uppercase letter",
          name.node
        )
      } else {
        format!("unknown shard `{}`", name.node)
      };
      self.problem(
        Problem::construct(name.span, "unknown-shard", "unknown-shard", message)
          .did_you_mean(suggestions)
          .shard(&name.node),
      );
      return;
    };
    let decls: Option<&'static [ParamDecl]> = match ty.desc.params {
      describe::Params::Declared(d) => Some(d),
      describe::Params::Undeclared => None,
    };
    let mut resolved: Vec<(&Param, Option<&ParamDecl>)> = Vec::new();
    let mut variadic_items: Vec<usize> = Vec::new();
    let mut ok = true;
    let mut position = 0;
    for param in params.map(|p| &p.items[..]).unwrap_or(&[]) {
      let decl = match (&param.name, decls) {
        (Some(pname), Some(decls)) => match decls.iter().find(|d| d.name == pname.node) {
          Some(d) => Some(d),
          None => {
            let known: Vec<String> = decls.iter().map(|d| d.name.to_string()).collect();
            self.problem(
              Problem::construct(
                pname.span,
                "generic",
                "unknown-argument",
                format!(
                  "{} has no parameter `{}` (parameters: {})",
                  ty.name(),
                  pname.node,
                  known.join(", ")
                ),
              )
              .did_you_mean(closest(&pname.node, known.clone(), 3))
              .shard(ty.name())
              .param(&pname.node),
            );
            ok = false;
            continue;
          }
        },
        (None, Some(decls)) => {
          // Past the end, a variadic last parameter takes the rest.
          let d = decls.get(position).or_else(|| {
            decls
              .last()
              .filter(|d| d.requirement == describe::Requirement::Variadic)
          });
          position += 1;
          d
        }
        (_, None) => None,
      };
      resolved.push((param, decl));
      // Variadic arguments are numbered for occurrence paths (`Item`).
      if decl.is_some_and(|d| d.requirement == describe::Requirement::Variadic) {
        variadic_items.push(resolved.len() - 1);
      }
    }
    // Computed values first: they are hoisted into Subs placed before the
    // shard, which moves the shard's index (nested flows depend on it).
    let mut values: Vec<Option<ParamValue>> = Vec::with_capacity(resolved.len());
    // When something is computed, plain variables read as values are
    // snapshotted too, in order, so they are read before later computations.
    let snapshot = resolved.iter().any(|(p, d)| !is_direct(&p.value, *d));
    for (param, decl) in &resolved {
      let read_operand =
        decl.is_some_and(|d| d.forms.contains(Forms::LITERAL) && d.forms.contains(Forms::VARIABLE));
      if snapshot && read_operand && is_plain_var(&param.value) {
        let v = self.operand(out, prefix, &param.value, true);
        ok &= v.is_some();
        values.push(v);
        continue;
      }
      if is_direct(&param.value, *decl) {
        values.push(None);
        continue;
      }
      if decl.is_some_and(|d| !d.forms.contains(Forms::VARIABLE)) {
        let d = decl.expect("checked");
        self.problem(
          Problem::construct(
            param.value.span,
            "generic",
            "computed-literal",
            format!(
              "{}.{} takes a literal; a computed value cannot be passed here",
              ty.name(),
              d.name
            ),
          )
          .shard(ty.name())
          .param(d.name),
        );
        ok = false;
        values.push(None);
        continue;
      }
      let v = self.operand(out, prefix, &param.value, false);
      ok &= v.is_some();
      values.push(v);
    }
    let index = out.len();
    let mut args = Vec::new();
    for (n, ((param, decl), hoisted)) in resolved.iter().zip(values).enumerate() {
      let value = match hoisted {
        Some(v) => Some(v),
        None if !is_direct(&param.value, *decl) => None,
        None => {
          let mut nested = prefix.to_vec();
          nested.push(PathStep::Shard {
            index,
            name: ty.name().to_string(),
          });
          if let Some(d) = decl {
            nested.push(PathStep::Param(d.name.to_string()));
            if let Some(item) = variadic_items.iter().position(|i| *i == n) {
              nested.push(PathStep::Item(item));
            }
          }
          self.param_value(&param.value, *decl, &nested)
        }
      };
      match value {
        Some(value) => args.push(match &param.name {
          Some(n) => Arg::named(&n.node, value),
          None => Arg::pos(value),
        }),
        None => ok = false,
      }
    }
    if ok {
      self.emit(out, prefix, block.span, ShardDef::with_args(ty, args));
    }
  }

  /// One argument, in the form its declaration accepts. Where a flow is
  /// accepted, anything that is not otherwise a valid argument becomes a
  /// flow: `If(All(a b) ...)` is `If({All(a b)} ...)`, `When(far ...)` is
  /// `When({far} ...)`, re-evaluated each time the shard runs it.
  fn param_value(
    &mut self,
    pipe: &Pipe,
    decl: Option<&ParamDecl>,
    nested: &[PathStep],
  ) -> Option<ParamValue> {
    let accepts = |f: Forms| decl.is_none_or(|d| d.forms.contains(f));
    let flow_accepted = decl.is_some_and(|d| d.forms.contains(Forms::FLOW));
    let as_flow = |l: &mut Self| {
      let mut flow = Vec::new();
      for block in &pipe.blocks {
        l.block(&mut flow, nested, block);
      }
      Some(ParamValue::Flow(flow))
    };
    let [b] = &pipe.blocks[..] else {
      return if flow_accepted {
        as_flow(self)
      } else {
        unreachable!("computed values are hoisted before param_value")
      };
    };
    match &b.kind {
      BlockKind::Seq(items) if decl.is_some_and(|d| d.forms.contains(Forms::CASES)) => {
        self.cases(items, b.span, nested)
      }
      BlockKind::Flow(stmts) => Some(ParamValue::Flow(self.statements(stmts.iter(), nested))),
      BlockKind::EmptyBraces if flow_accepted => Some(ParamValue::Flow(Vec::new())),
      BlockKind::Var { name, path } if path.is_empty() => {
        if accepts(Forms::WIRE) && !accepts(Forms::VARIABLE) {
          Some(ParamValue::Wire(name.node.clone()))
        } else if accepts(Forms::VARIABLE) || !flow_accepted {
          Some(ParamValue::Var(name.node.clone()))
        } else {
          as_flow(self)
        }
      }
      _ if is_constant(b) && (accepts(Forms::LITERAL) || !flow_accepted) => {
        self.constant(b).map(ParamValue::Value)
      }
      _ if flow_accepted => as_flow(self),
      _ => self.constant(b).map(ParamValue::Value),
    }
  }
}

impl Lowerer<'_> {
  /// `[value {flow} value {flow}]` as cases; `none` matches anything.
  fn cases(&mut self, items: &[Pipe], span: Span, nested: &[PathStep]) -> Option<ParamValue> {
    if !items.len().is_multiple_of(2) {
      self.problem(Problem::construct(
        span,
        "generic",
        "cases",
        "cases come in pairs: `[value {flow} value {flow}]`".into(),
      ));
      return None;
    }
    let mut cases = Vec::with_capacity(items.len() / 2);
    for (k, pair) in items.chunks(2).enumerate() {
      let value = match &pair[0].blocks[..] {
        [b] if is_constant(b) => self.constant(b)?,
        _ => {
          self.problem(Problem::construct(
            pair[0].span,
            "generic",
            "cases",
            "a case's value must be a literal (`none` matches anything)".into(),
          ));
          return None;
        }
      };
      let mut path = nested.to_vec();
      path.push(PathStep::Item(k));
      let flow = match &pair[1].blocks[..] {
        [b] => match &b.kind {
          BlockKind::Flow(stmts) => self.statements(stmts.iter(), &path),
          BlockKind::EmptyBraces => Vec::new(),
          _ => {
            self.problem(Problem::construct(
              pair[1].span,
              "generic",
              "cases",
              "each case value is followed by a flow in braces".into(),
            ));
            return None;
          }
        },
        _ => {
          self.problem(Problem::construct(
            pair[1].span,
            "generic",
            "cases",
            "each case value is followed by a flow in braces".into(),
          ));
          return None;
        }
      };
      cases.push((value, flow));
    }
    Some(ParamValue::Cases(cases))
  }
}

/// Positional and named parameters, in order.
fn split_params(params: &Params) -> (Vec<&Pipe>, Vec<(&Name, &Pipe)>) {
  let mut positional = Vec::new();
  let mut named = Vec::new();
  for p in &params.items {
    match &p.name {
      Some(n) => named.push((n, &p.value)),
      None => positional.push(&p.value),
    }
  }
  (positional, named)
}

/// Whether a block is a literal value: a literal, `@name`, `{}`, or a
/// sequence or table of them.
fn is_constant(block: &Block) -> bool {
  let single = |pipe: &Pipe| matches!(&pipe.blocks[..], [b] if is_constant(b));
  match &block.kind {
    BlockKind::Literal(_) | BlockKind::EmptyBraces | BlockKind::Func { params: None, .. } => true,
    BlockKind::Seq(items) => items.iter().all(single),
    BlockKind::Table(entries) => entries.iter().all(|(_, v)| single(v)),
    _ => false,
  }
}

/// Whether a parameter value is passed as it is (a flow, `{}`, a plain
/// variable or wire name, or a literal) rather than computed first.
fn is_direct(pipe: &Pipe, decl: Option<&ParamDecl>) -> bool {
  // A flow parameter takes anything, as a flow (see `param_value`).
  if decl.is_some_and(|d| d.forms.contains(Forms::FLOW)) {
    return true;
  }
  match &pipe.blocks[..] {
    [b] => match &b.kind {
      BlockKind::Flow(_) | BlockKind::EmptyBraces => true,
      BlockKind::Seq(_) if decl.is_some_and(|d| d.forms.contains(Forms::CASES)) => true,
      BlockKind::Var { path, .. } => path.is_empty(),
      _ => is_constant(b),
    },
    _ => false,
  }
}

/// A plain variable read (no path).
fn is_plain_var(pipe: &Pipe) -> bool {
  matches!(&pipe.blocks[..], [b] if matches!(&b.kind, BlockKind::Var { path, .. } if path.is_empty()))
}

/// A literal or a plain variable: needs no computation.
fn is_plain(pipe: &Pipe) -> bool {
  is_plain_var(pipe) || matches!(&pipe.blocks[..], [b] if is_constant(b))
}
