# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@wire(find-index {
  value = target
  -1 >= result
  true >= keep-going
  0 >= i
  [10 20 30 40] = seq
  While(
    {keep-going}
    {
      If(
        {i | IsLess(Count(seq))}
        {
          If(
            {seq | Take(i) | Is(target)}
            {
              i | Set(result)
              false | Set(keep-going)
            }
          )
          Inc(i)
        }
        {
          false | Set(keep-going)
        }
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
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"unknown-variable","message":"unknown variable value","file":"solution.shs","line":2,"column":3,"shard":"Get","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":1,"name":"Do"},{"param":"Wire"},{"wire":"find-index"},{"shard":0,"name":"Get"}]}]}
```

## Reply 1

```shards
@wire(find-index {
  = target
  -1 >= result
  true >= keep-going
  0 >= i
  [10 20 30 40] = seq
  While(
    {keep-going}
    {
      If(
        {i | IsLess(Count(seq))}
        {
          If(
            {seq | Take(i) | Is(target)}
            {
              i | Set(result)
              false | Set(keep-going)
            }
          )
          Inc(i)
        }
        {
          false | Set(keep-going)
        }
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
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"too-many-arguments","message":"Count takes at most 0 arguments, got 1","file":"solution.shs","line":11,"column":21,"shard":"Count","path":[{"wire":"root"},{"shard":1,"name":"Do"},{"param":"Wire"},{"wire":"find-index"},{"shard":9,"name":"While"},{"param":"Action"},{"shard":0,"name":"If"},{"param":"Predicate"},{"shard":1,"name":"SubFlow"},{"param":"Action"},{"shard":0,"name":"Count"}]}]}
```

## Reply 2

```shards
@wire(find-index {
  = target
  -1 >= result
  true >= keep-going
  0 >= i
  [10 20 30 40] = seq
  While(
    {keep-going}
    {
      If(
        {i | IsLess(seq | Count)}
        {
          If(
            {seq | Take(i) | Is(target)}
            {
              i | Set(result)
              false | Set(keep-going)
            }
          )
          Inc(i)
        }
        {
          false | Set(keep-going)
        }
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

## Reply 3

```shards
@wire(find-index {
  Set(target)
  -1 >= result
  true >= keep-going
  0 >= i
  [10 20 30 40] = seq
  While(
    {keep-going}
    {
      If(
        {i | IsLess(seq | Count)}
        {
          If(
            {seq | Take(i) | Is(target)}
            {
              i | Set(result)
              false | Set(keep-going)
            }
          )
          Inc(i)
        }
        {
          false | Set(keep-going)
        }
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
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"seq already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":6,"column":17,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":4,"name":"Do"},{"param":"Wire"},{"wire":"find-index"},{"shard":8,"name":"Ref"}]}]}
```

## Reply 4

```shards
@wire(find-index {
  Set(target)
  -1 >= result
  true >= keep-going
  0 >= i
  [10 20 30 40] | Set(seq)
  While(
    {keep-going}
    {
      If(
        {i | IsLess(seq | Count)}
        {
          If(
            {seq | Take(i) | Is(target)}
            {
              i | Set(result)
              false | Set(keep-going)
            }
          )
          Inc(i)
        }
        {
          false | Set(keep-going)
        }
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
