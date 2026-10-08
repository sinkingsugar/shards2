# 10-seq-double

## Task

Given the sequence [1 2 3 4], build a new sequence with every element doubled and log the new sequence (as one value).

## Reply 0

```shards
[] | Var(result)
0 | Var(i)
Repeat({ [1 2 3 4] | Take(i) | Math.Multiply(2) | Push(result) Inc(i) } times: 4)
result | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[2 4 6 8]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[2 4 6 8]"}],"spawned_failures":[]}
stderr:
```
