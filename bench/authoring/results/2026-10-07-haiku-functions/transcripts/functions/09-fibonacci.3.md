# 09-fibonacci

## Task

Log the first 10 Fibonacci numbers, starting 0, 1, 1, 2, ..., one per line.

## Reply 0

```shards
0 | Log
0 | Var(prev)
1 | Var(curr)
0 | Var(next)
Repeat({
  curr | Log
  curr | Math.Add(prev) | Update(next)
  curr | Update(prev)
  next | Update(curr)
} times: 9)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0","1","1","2","3","5","8","13","21","34"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
