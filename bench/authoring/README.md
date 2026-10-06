# Authoring eval

Spec: [docs/golden-path.md §8](../../docs/golden-path.md). It measures how well a model writes Shards from a reference alone, so syntax and API disputes are settled with data (decision D12).

## Pieces

- `tasks/*.task`: small tasks, each with a `prompt`, the `expected` log lines and a hidden `reference` solution. `cargo test -p authoring-eval` runs every reference in-process and requires exactly the expected log, so the tasks stay solvable as the language changes (M2 rewrites the references with the rest of the suite). The tree-fold task arrives with recursion (M7).
- `reference/<variant>.md`: the language primer for a syntax variant. The model gets the primer followed by `shards2 catalog` and `shards2 describe` for every shard (`authoring-eval reference` prints it). `current` is the pre-M2 syntax.
- `src/main.rs`: the runner. It shells out to a model command; there is no embedded API client.

## Running

Build `shards2` first (`cargo build --release -p shards-cli`), then:

```sh
cargo run -p authoring-eval -- verify    # reference solutions through the real CLI

cargo run -p authoring-eval -- run --model sonnet --format claude-json \
  --cmd 'claude -p --model sonnet --output-format json --tools "" --setting-sources "" --strict-mcp-config' \
  --out results.csv --transcripts transcripts/sonnet

cargo run -p authoring-eval -- run --model codex --format text \
  --cmd 'codex exec --skip-git-repo-check --sandbox read-only -' --out results.csv
```

The prompt goes to the command's stdin, from an empty temporary directory, so the model sees only the prompt; disable tools where the CLI allows it. Per task: the model's last fenced code block is checked with `shards2 check --json`; diagnostics go back for at most `--rounds` repairs (default 5); a program that checks is run and its log compared with the expected lines (the result lines `shards2 run` prints for `root` and scheduled wires are ignored). `--only TEXT` selects tasks whose id contains the text, `--jobs N` runs tasks in parallel (default 4).

## Results

One CSV row per (task, model, variant): `SemanticSuccess` (the log matched), `FirstPassSuccess` (the first reply checked clean), `RepairRounds`, `Tokens` (summed over rounds; `claude-json` only, and it includes the CLI's own system prompt, about 20k tokens a call), `LatencyMs` (model time only) and `Failure`: `syntax` (parse errors), `missing-shard` (an unknown shard), `compose` (other check errors), `runtime`, `wrong-output`, `timeout` or `model-error`.

`baseline.csv` holds the pre-M2 baseline. Commit rows from real runs only, never mock or placeholder data. Small samples are evidence, not proof.
