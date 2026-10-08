# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@wire(find-index {
  = target
  [10 20 30 40] = seq
  0 >= i
  -1 >= result
  While(
    Predicate: {
      All(
        { result | Is(-1) }
        { i | IsLess(seq | Count) }
      )
    }
    Action: {
      If(
        Predicate: { seq | Take(i) | Is(target) }
        Then: { i > result }
        Else: { Inc(i) }
      )
    }
  )
  result
})

@run {
  30 | Do(find-index) | Log
  99 | Do(find-index) | Log
}
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"declaration","message":"`@run` takes a mesh: `@run(main FPS: 30)`","file":"solution.shs","line":24,"column":1},{"phase":"construct","severity":"error","kind":"generic","code":"unsupported","message":"a flow in braces is a parameter value (`When({...} {...})`); on its own it does nothing","file":"solution.shs","line":24,"column":6}]}
```

## Reply 1

```shards
@wire(find-index {
  = target
  [10 20 30 40] = seq
  0 >= i
  -1 >= result
  While(
    Predicate: {
      All(
        { result | Is(-1) }
        { i | IsLess(seq | Count) }
      )
    }
    Action: {
      If(
        Predicate: { seq | Take(i) | Is(target) }
        Then: { i > result }
        Else: { Inc(i) }
      )
    }
  )
  result
})

30 | Do(find-index) | Log
99 | Do(find-index) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"target already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":2,"column":3,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":4,"name":"Do"},{"param":"Wire"},{"wire":"find-index"},{"shard":0,"name":"Ref"}]}]}
```

## Reply 2

```shards
@wire(find-index {
  Set(target)
  [10 20 30 40] | Set(seq)
  0 | Set(i)
  -1 | Set(result)
  While(
    Predicate: {
      All(
        { result | Is(-1) }
        { i | IsLess(seq | Count) }
      )
    }
    Action: {
      If(
        Predicate: { seq | Take(i) | Is(target) }
        Then: { i | Update(result) }
        Else: { Inc(i) }
      )
    }
  )
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
