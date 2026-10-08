# 35-in-range

## Task

For each of 3, 8 and 12, log whether it lies between 5 and 10 inclusive, as a Bool (`true` or `false`), one per line.

## Reply 0

```shards
3 | All({IsMoreEqual(5)} {IsLessEqual(10)}) | Log
8 | All({IsMoreEqual(5)} {IsLessEqual(10)}) | Log
12 | All({IsMoreEqual(5)} {IsLessEqual(10)}) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["false","true","false"],"outcomes":[{"wire":"root","outcome":"completed","value":"false"}],"spawned_failures":[]}
stderr:
```
