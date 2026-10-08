# 09-fibonacci

## Task

Log the first 10 Fibonacci numbers, starting 0, 1, 1, 2, ..., one per line.

## Reply 0

```shards
Keep(a 0)
Keep(b 1)
Repeat({
  a | Log
  a | Add(b) | Var(temp)
  b | Update(a)
  temp | Update(b)
} times: 10)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0","1","1","2","3","5","8","13","21","34"],"outcomes":[{"wire":"root","outcome":"completed","value":"none"}],"spawned_failures":[]}
stderr:
```
