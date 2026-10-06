# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@wire(find-in-seq {
  Ref(item)
  -1 >= result
  0 >= i
  [10 20 30 40] = seq
  Repeat({
    If({ seq | Take(i) | Is(item) } {
      i > result
    } {
      Inc(i)
    })
  } Times: 4 Until: { Get(result) | IsMoreEqual(0) })
  result
})

30 | Do(find-in-seq) | Log
99 | Do(find-in-seq) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"item already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":2,"column":3,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":4,"name":"Do"},{"param":"Wire"},{"wire":"find-in-seq"},{"shard":0,"name":"Ref"}]}]}
```

## Reply 1

```shards
[10 20 30 40] = seq

@wire(find-in-seq {
  Set(item)
  -1 | Set(result)
  0 | Set(i)
  Repeat({
    If({ seq | Take(i) | Is(item) } {
      i > result
    } {
      Inc(i)
    })
  } Times: 4 Until: { Get(result) | IsMoreEqual(0) })
  result
})

30 | Do(find-in-seq) | Log
99 | Do(find-in-seq) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
