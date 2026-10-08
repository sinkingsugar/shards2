# 09-fibonacci

## Task

Log the first 10 Fibonacci numbers, starting 0, 1, 1, 2, ..., one per line.

## Reply 0

```shards
0 >= fib
1 >= next

Log(0)

Repeat({
  next | Log
  fib | Math.Add(next) >= temp
  next > fib
  temp > next
} Times: 9)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"wrong-argument-type","message":"Log: Prefix must be String, got Int","file":"solution.shs","line":4,"column":1,"shard":"Log","actual":{"name":"Int","basic_type":4},"expected":[{"name":"String","basic_type":52}],"param_index":0,"param":"Prefix","path":[{"wire":"root"},{"shard":4,"name":"Log"}]}]}
```

## Reply 1

```shards
0 >= fib
1 >= next

0 | Log

Repeat({
  next | Log
  fib | Math.Add(next) >= temp
  next > fib
  temp > next
} Times: 9)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0","1","1","2","3","5","8","13","21","34"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
