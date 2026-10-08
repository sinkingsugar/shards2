# 09-fibonacci

## Task

Log the first 10 Fibonacci numbers, starting 0, 1, 1, 2, ..., one per line.

## Reply 0

```shards
0 >= a
1 >= b
a | Log
Repeat({
  b | Log
  a | Add(b) = next
  b > a
  next > b
} Times: 9)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0","1","1","2","3","5","8","13","21","34"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
