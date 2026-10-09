//! Flow parameters against the native loop (docs/metaprogramming.md §3.4):
//! summing a sequence's elements into a caller variable with a `While`
//! written in place, with a function running a block per element whose
//! call is inlined with its block (`Each`, which takes the length as a
//! parameter so its body is straight-line code), and with the same loop as
//! a function that counts the sequence itself (`ForEach`: `Count` has no VM
//! form, so the call keeps its frame and each element runs the block in a
//! frame of its own).
//!
//! Usage: cargo run --release -p shards-lang --example bench_flow_params
//!
//! Prints nanoseconds per element, from the slope between two repetition
//! counts (so loading and composing cancel out), the fastest of `RUNS`.

use std::collections::HashMap;
use std::time::Instant;

use shards_core::Catalog;
use shards_lang::{Program, Source};

const RUNS: usize = 5;
const WIDTHS: [usize; 3] = [8, 64, 256];
/// Elements per measurement at the smaller repetition count.
const ELEMENTS: usize = 400_000;

const FUNCTIONS: &str = r#"@fn(Each input: [Int] output: [Int] params: {n: Int action: Flow(input: Int)} {
  = xs
  0 | Var(i)
  While({i | IsLess(n)} { xs | Take(i) | Run(action) Inc(i) })
  xs
})
@fn(ForEach input: [Int] output: [Int] params: {action: Flow(input: Int)} {
  = xs
  0 | Var(i)
  xs | Count = n
  While({i | IsLess(n)} { xs | Take(i) | Run(action) Inc(i) })
  xs
})
"#;

fn program(variant: &str, width: usize, repeat: usize) -> String {
  let block = "Math.Add(total) | Update(total)";
  let body = match variant {
    "native" => {
      format!("0 | Var(i) While({{i | IsLess({width})}} {{ xs | Take(i) | {block} Inc(i) }})")
    }
    "inlined" => format!("xs | Each(n: {width} action: {{{block}}})"),
    "framed" => format!("xs | ForEach(action: {{{block}}})"),
    _ => unreachable!(),
  };
  let items: Vec<String> = (0..width).map(|i| i.to_string()).collect();
  format!(
    "{FUNCTIONS}[{}] = xs\n0 | Var(total)\nRepeat({{ {body} }} times: {repeat})\ntotal",
    items.join(" ")
  )
}

fn seconds(text: &str, catalog: &Catalog) -> f64 {
  let program = match Program::load(Source::new("bench.shs", text), catalog, &HashMap::new()) {
    Ok(p) => p,
    Err((_, d)) => panic!("load failed: {d:?}"),
  };
  let start = Instant::now();
  let report = program
    .run()
    .unwrap_or_else(|d| panic!("run failed: {d:?}"));
  let elapsed = start.elapsed().as_secs_f64();
  assert!(report.succeeded());
  elapsed
}

fn main() {
  let catalog = Catalog::new(&[shards_core::shards::CATALOG]).unwrap();
  println!(
    "{:>6} {:>12} {:>12} {:>12} {:>9} {:>9}",
    "width", "native ns", "inlined ns", "framed ns", "inlined/", "framed/"
  );
  for width in WIDTHS {
    let repeat = ELEMENTS / width;
    let mut ns = Vec::new();
    for variant in ["native", "inlined", "framed"] {
      let (short, long) = (
        program(variant, width, repeat),
        program(variant, width, 2 * repeat),
      );
      let best = (0..RUNS)
        .map(|_| {
          let (a, b) = (seconds(&short, &catalog), seconds(&long, &catalog));
          (b - a) * 1e9 / (repeat * width) as f64
        })
        .fold(f64::INFINITY, f64::min);
      ns.push(best);
    }
    println!(
      "{width:>6} {:>12.2} {:>12.2} {:>12.2} {:>9.2} {:>9.2}",
      ns[0],
      ns[1],
      ns[2],
      ns[1] / ns[0],
      ns[2] / ns[0]
    );
  }
}
