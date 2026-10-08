# 06-countdown

## Task

Count down from 5 to 1, logging each number on its own line, then log `liftoff`.

## Reply 0

```shards
5 >= n
Repeat(
  {
    n | Log
    Dec(n)
  }
  Until: {n | IsLessEqual(0)}
)
"liftoff" | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["5","4","3","2","1","liftoff"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"liftoff\""}],"spawned_failures":[]}
stderr:
```
