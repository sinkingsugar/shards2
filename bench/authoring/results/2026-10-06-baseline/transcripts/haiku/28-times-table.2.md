# 28-times-table

## Task

Log the multiplication table for 1 to 3 with nested loops: one line per pair, formatted like `2x3=6`, with the left number in the outer loop.

## Reply 0

```shards
0 >= i
0 >= j
Repeat({
  Inc(i)
  0 > j
  Repeat({
    Inc(j)
    f"{i}x{j}={i | Mul(j)}" | Log
  } Times: 3)
} Times: 3)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1x1=1","1x2=2","1x3=3","2x1=2","2x2=4","2x3=6","3x1=3","3x2=6","3x3=9"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
