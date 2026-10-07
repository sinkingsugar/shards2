# 09-fibonacci

## Task

Log the first 10 Fibonacci numbers, starting 0, 1, 1, 2, ..., one per line.

## Reply 0

```shards
0 >= fib-prev
1 >= fib-curr
fib-prev | Log
fib-curr | Log
Repeat({
  fib-prev | Add(fib-curr) = fib-next
  fib-curr > fib-prev
  fib-next > fib-curr
  fib-curr | Log
} Times: 8)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0","1","1","2","3","5","8","13","21","34"],"outcomes":[{"wire":"root","outcome":"completed","value":"1"}],"spawned_failures":[]}
stderr:
```
