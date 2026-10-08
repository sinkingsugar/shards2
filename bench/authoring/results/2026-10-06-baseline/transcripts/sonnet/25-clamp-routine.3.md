# 25-clamp-routine

## Task

Define a reusable named routine `clamp` that clamps a value to a lower and an upper bound, and call it (do not copy its body) three times: 5 with bounds 0..3, -2 with bounds 0..3, and 2 with bounds 0..3. Log each result.

## Reply 0

```shards
@wire(clamp {
  = x
  If(IsLess(lo) {lo} {If(IsMore(hi) {hi} {x})})
})

0 >= lo
3 >= hi

5 | Do(clamp) | Log
-2 | Do(clamp) | Log
2 | Do(clamp) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"x already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":2,"column":3,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":8,"name":"Do"},{"param":"Wire"},{"wire":"clamp"},{"shard":0,"name":"Ref"}]}]}
```

## Reply 1

```shards
@wire(clamp {
  If(IsLess(lo) {lo})
  If(IsMore(hi) {hi})
})

0 >= lo
3 >= hi

5 | Do(clamp) | Log
-2 | Do(clamp) | Log
2 | Do(clamp) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["3","0","2"],"outcomes":[{"wire":"root","outcome":"completed","value":"2"}],"spawned_failures":[]}
stderr:
```
