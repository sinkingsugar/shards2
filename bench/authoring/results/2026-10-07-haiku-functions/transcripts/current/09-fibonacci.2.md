# 09-fibonacci

## Task

Log the first 10 Fibonacci numbers, starting 0, 1, 1, 2, ..., one per line.

## Reply 0

```shards
0 >= prev
1 >= curr
prev | Log
Repeat({
  prev | Add(curr) = sum
  curr > prev
  sum > curr
  curr | Log
} Times: 9)
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0","1","2","3","5","8","13","21","34","55"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
