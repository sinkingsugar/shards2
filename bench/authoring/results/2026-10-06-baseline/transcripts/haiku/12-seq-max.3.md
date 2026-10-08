# 12-seq-max

## Task

Log the largest element of the sequence [3 8 1 9 4]. Do not hard-code the answer; scan the sequence.

## Reply 0

```shards
[3 8 1 9 4] = xs
xs | Take(0) >= max
1 >= i
xs | Count >= length
length | Math.Subtract(1) >= iterations
Repeat({
  xs | Take(i) | If({IsMore(max)} {> max})
  Inc(i)
} Times: iterations)
max | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["9"],"outcomes":[{"wire":"root","outcome":"completed","value":"9"}],"spawned_failures":[]}
stderr:
```
