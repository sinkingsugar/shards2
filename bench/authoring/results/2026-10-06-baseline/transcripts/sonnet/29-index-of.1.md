# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
[10 20 30 40] = xs
[30 99] = targets
0 >= t
0 >= target
0 >= i
-1 >= result
Repeat({
  targets | Take(t) | ExpectInt > target
  -1 > result
  0 > i
  Repeat({
    When({ xs | Take(i) | Is(target) } { i > result })
    Inc(i)
  } Times: 4)
  result | Log
  Inc(t)
} Times: 2)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
