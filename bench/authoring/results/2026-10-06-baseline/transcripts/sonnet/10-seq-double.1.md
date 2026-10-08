# 10-seq-double

## Task

Given the sequence [1 2 3 4], build a new sequence with every element doubled and log the new sequence (as one value).

## Reply 0

```shards
[1 2 3 4] = xs
xs.0 | Math.Multiply(2) >> out
1 >= i
Repeat({ xs | Take(i) | ExpectInt | Math.Multiply(2) >> out  Inc(i) } Times: 3)
out | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[2 4 6 8]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[2 4 6 8]"}],"spawned_failures":[]}
stderr:
```
