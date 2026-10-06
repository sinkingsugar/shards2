# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
[10 20 30 40] = xs

0 >= idx
-1 >= result
Repeat({
  xs | Take(idx) = found-elem
  found-elem | Is(30) | If(
    {idx | Update(result) Stop}
    {Inc(idx)}
  )
} Times: 4)
result | Log

0 > idx
-1 > result
Repeat({
  xs | Take(idx) = found-elem
  found-elem | Is(99) | If(
    {idx | Update(result) Stop}
    {Inc(idx)}
  )
} Times: 4)
result | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"found-elem already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":17,"column":18,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":13,"name":"Repeat"},{"param":"Action"},{"shard":2,"name":"Ref"}]}]}
```

## Reply 1

```shards
[10 20 30 40] = xs

0 >= idx
-1 >= result
Repeat({
  xs | Take(idx) | Is(30) | If(
    {idx | Update(result) Stop}
    {Inc(idx)}
  )
} Times: 4)
result | Log

0 > idx
-1 > result
Repeat({
  xs | Take(idx) | Is(99) | If(
    {idx | Update(result) Stop}
    {Inc(idx)}
  )
} Times: 4)
result | Log
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":[],"outcomes":[{"wire":"root","outcome":"stopped"}],"spawned_failures":[]}
stderr:
```
