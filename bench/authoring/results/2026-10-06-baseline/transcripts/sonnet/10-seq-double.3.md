# 10-seq-double

## Task

Given the sequence [1 2 3 4], build a new sequence with every element doubled and log the new sequence (as one value).

## Reply 0

```shards
[1 2 3 4] = xs
0 >= i
Repeat({
  xs | Take(i) | Math.Multiply(2) >> ys
  Inc(i)
} Times: 4)
ys | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"possibly-uninitialized","message":"ys may be uninitialized here (it is only assigned in a branch or loop body that might not run)","file":"solution.shs","line":7,"column":1,"shard":"Get","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":5,"name":"Get"}]}]}
```

## Reply 1

```shards
[1 2 3 4] = xs
xs | Take(0) | Math.Multiply(2) >> ys
1 >= i
Repeat({
  xs | Take(i) | Math.Multiply(2) >> ys
  Inc(i)
} Times: 3)
ys | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[2 4 6 8]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[2 4 6 8]"}],"spawned_failures":[]}
stderr:
```
