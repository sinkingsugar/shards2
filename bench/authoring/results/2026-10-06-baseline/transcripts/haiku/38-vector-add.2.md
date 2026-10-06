# 38-vector-add

## Task

Add the 3D float vectors (1, 2, 3) and (4, 5, 6) and log the resulting vector.

## Reply 0

```shards
[4.0 5.0 6.0] | ToFloat3 = v2
[1.0 2.0 3.0] | ToFloat3 | Math.Add(v2) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["@f3(5 7 9)"],"outcomes":[{"wire":"root","outcome":"completed","value":"@f3(5.0 7.0 9.0)"}],"spawned_failures":[]}
stderr:
```
