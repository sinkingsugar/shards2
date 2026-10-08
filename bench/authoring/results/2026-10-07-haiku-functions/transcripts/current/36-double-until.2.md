# 36-double-until

## Task

Start from 1 and keep doubling while the value is not greater than 100. Log the first value greater than 100.

## Reply 0

```shards
1 >= value
While({value | IsLessEqual(100)} {value | Math.Multiply(2) > value})
value | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["128"],"outcomes":[{"wire":"root","outcome":"completed","value":"128"}],"spawned_failures":[]}
stderr:
```
