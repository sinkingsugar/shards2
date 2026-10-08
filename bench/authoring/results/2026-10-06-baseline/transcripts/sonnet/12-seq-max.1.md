# 12-seq-max

## Task

Log the largest element of the sequence [3 8 1 9 4]. Do not hard-code the answer; scan the sequence.

## Reply 0

```shards
[3 8 1 9 4] = xs
xs | Take(0) >= best
1 >= i
Repeat({
  xs | Take(i) | ExpectInt = x
  When({x | IsMore(best)} {x > best})
  Inc(i)
} Times: 4)
best | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["9"],"outcomes":[{"wire":"root","outcome":"completed","value":"9"}],"spawned_failures":[]}
stderr:
```
