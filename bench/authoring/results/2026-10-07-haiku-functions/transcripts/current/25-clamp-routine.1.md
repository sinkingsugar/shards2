# 25-clamp-routine

## Task

Define a reusable named routine `clamp` that clamps a value to a lower and an upper bound, and call it (do not copy its body) three times: 5 with bounds 0..3, -2 with bounds 0..3, and 2 with bounds 0..3. Log each result.

## Reply 0

```shards
@wire(clamp {
  SubFlow({Take("value") = v})
  SubFlow({Take("lower") = l})
  SubFlow({Take("upper") = u})
  v | If({IsLess(l)} {l} {If({v | IsMore(u)} {u} {v})})
})

{value: 5 lower: 0 upper: 3} | Do(clamp) | Log
{value: -2 lower: 0 upper: 3} | Do(clamp) | Log
{value: 2 lower: 0 upper: 3} | Do(clamp) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"v already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":2,"column":26,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":4,"name":"Do"},{"param":"Wire"},{"wire":"clamp"},{"shard":0,"name":"SubFlow"},{"param":"Action"},{"shard":1,"name":"Ref"}]}]}
```

## Reply 1

```shards
0 >= v
0 >= l
0 >= u

@wire(clamp {
  SubFlow({Take("value") > v})
  SubFlow({Take("lower") > l})
  SubFlow({Take("upper") > u})
  v | If({IsLess(l)} {l} {If({IsMore(u)} {u} {v})})
})

{value: 5 lower: 0 upper: 3} | Do(clamp) | Log
{value: -2 lower: 0 upper: 3} | Do(clamp) | Log
{value: 2 lower: 0 upper: 3} | Do(clamp) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["3","0","2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
