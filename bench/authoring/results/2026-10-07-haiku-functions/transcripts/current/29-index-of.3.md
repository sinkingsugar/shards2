# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@wire(find-index {
  = search-value
  [10 20 30 40] = sequence
  -1 >= result
  0 >= index
  Repeat({
    If(
      {sequence.index | Is(search-value)}
      {index > result}
      {}
    )
    Inc(index)
  } Until: {result | IsNot(-1)} Times: 4)
  result
})

30 | Do(find-index) | Log
99 | Do(find-index) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"wrong-key-type","message":"Take on [Int] needs a Int key, got String","file":"solution.shs","line":8,"column":17,"shard":"Take","param_index":0,"param":"Key","path":[{"wire":"root"},{"shard":1,"name":"Do"},{"param":"Wire"},{"wire":"find-index"},{"shard":7,"name":"Repeat"},{"param":"Action"},{"shard":0,"name":"If"},{"param":"Predicate"},{"shard":1,"name":"Take"}]}]}
```

## Reply 1

```shards
@wire(find-index {
  = search-value
  [10 20 30 40] = sequence
  -1 >= result
  0 >= index
  Repeat({
    If(
      {sequence | Take(index) | Is(search-value)}
      {index > result}
      {}
    )
    Inc(index)
  } Until: {result | IsNot(-1)} Times: 4)
  result
})

30 | Do(find-index) | Log
99 | Do(find-index) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"search-value already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":2,"column":3,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":4,"name":"Do"},{"param":"Wire"},{"wire":"find-index"},{"shard":0,"name":"Ref"}]}]}
```

## Reply 2

```shards
@wire(find-index {
  Set(search-value)
  [10 20 30 40] | Set(sequence)
  -1 >= result
  0 >= index
  Repeat({
    If(
      {sequence | Take(index) | Is(search-value)}
      {Get(index) > result}
      {}
    )
    Inc(index)
  } Until: {result | IsNot(-1)} Times: 4)
  result
})

30 | Do(find-index) | Log
99 | Do(find-index) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
