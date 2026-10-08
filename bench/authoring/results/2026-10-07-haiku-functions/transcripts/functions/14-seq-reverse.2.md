# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = original
[] | Var(reversed)
original | Count = count
0 | Var(i)
Repeat({
  count | Math.Subtract(1) | Math.Subtract(i) = idx
  original | Take(idx) | Push(reversed)
  Inc(i)
} times: count)
reversed | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[5 4 3 2 1]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[5 4 3 2 1]"}],"spawned_failures":[]}
stderr:
```
