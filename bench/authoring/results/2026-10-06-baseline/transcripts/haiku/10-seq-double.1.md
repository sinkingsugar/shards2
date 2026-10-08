# 10-seq-double

## Task

Given the sequence [1 2 3 4], build a new sequence with every element doubled and log the new sequence (as one value).

## Reply 0

```shards
[1 2 3 4] = input-seq
[] >= output-seq
0 >= i
Repeat({
  input-seq | Take(i) | Math.Multiply(2) >> output-seq
  Inc(i)
} Times: 4)
output-seq | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[2 4 6 8]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[2 4 6 8]"}],"spawned_failures":[]}
stderr:
```
