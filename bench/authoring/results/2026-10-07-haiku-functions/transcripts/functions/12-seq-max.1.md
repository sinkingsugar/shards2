# 12-seq-max

## Task

Log the largest element of the sequence [3 8 1 9 4]. Do not hard-code the answer; scan the sequence.

## Reply 0

```shards
[3 8 1 9 4] = seq
seq | Count = count
0 | Var(i)
-9999999 | Var(max)
none | Repeat({
  seq | Take(i) | If({IsMore(max)} {Update(max)} {})
  Inc(i)
} times: count)
max | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["9"],"outcomes":[{"wire":"root","outcome":"completed","value":"9"}],"spawned_failures":[]}
stderr:
```
