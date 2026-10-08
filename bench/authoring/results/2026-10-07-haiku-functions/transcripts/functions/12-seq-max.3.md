# 12-seq-max

## Task

Log the largest element of the sequence [3 8 1 9 4]. Do not hard-code the answer; scan the sequence.

## Reply 0

```shards
[3 8 1 9 4] = seq
seq.0 | Var(max-val)
0 | Var(i)
Repeat({
  seq | Take(i) | When(IsMore(max-val) {
    Update(max-val)
  })
  Inc(i)
} times: 5)
max-val | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["9"],"outcomes":[{"wire":"root","outcome":"completed","value":"9"}],"spawned_failures":[]}
stderr:
```
