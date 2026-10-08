# 12-seq-max

## Task

Log the largest element of the sequence [3 8 1 9 4]. Do not hard-code the answer; scan the sequence.

## Reply 0

```shards
[3 8 1 9 4] = xs
xs | Count = n
xs | Take(0) | ExpectInt >= best
1 >= i
Repeat({
  xs | Take(i) | ExpectInt | When({IsMore(best)} {Update(best)})
  Inc(i)
} Times: n)
best | Log
```

## Run (runtime)

```
exit Some(1)
{"ok":false,"file":"solution.shs","diagnostics":[],"log":[],"outcomes":[{"wire":"root","outcome":"failed","error":"activation error: Take: index 5 is out of range (length 5)"}],"spawned_failures":[]}
stderr:
```
