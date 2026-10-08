# 32-rounding

## Task

Log, one per line: 2.5 rounded to the nearest integer value, 2.7 rounded down, and 2.1 rounded up. Log them as integers.

## Reply 0

```shards
2.5 | Math.Round | ToInt | Log
2.7 | Math.Floor | ToInt | Log
2.1 | Math.Ceil | ToInt | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["3","2","3"],"outcomes":[{"wire":"root","outcome":"completed","value":"3"}],"spawned_failures":[]}
stderr:
```
