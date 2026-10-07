# 29-index-of

## Task

Find the index (0-based) of a value in the sequence [10 20 30 40], or -1 when it is absent. Log the result for 30 and then for 99.

## Reply 0

```shards
@fn(FindIndex input: None output: Int params: {value: Int sequence: [Int]} {
  0 | Var(index)
  -1 | Var(result)
  Repeat({
    When(
      predicate: { sequence | Take(index) | Is(value) }
      action: { index | Update(result) Return }
    )
    Inc(index)
  } times: { sequence | Count })
  result
})

[10 20 30 40] = seq
30 | FindIndex(value: 30 sequence: seq) | Log
99 | FindIndex(value: 99 sequence: seq) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"wrong-argument-form","message":"Repeat: times takes a literal or variable, got a flow","file":"solution.shs","line":4,"column":3,"shard":"Repeat","param_index":1,"param":"times","path":[{"wire":"root"},{"shard":3,"name":"FindIndex"},{"function":"FindIndex"},{"shard":4,"name":"Repeat"}]},{"phase":"construct","severity":"error","kind":"generic","code":"wrong-argument-form","message":"Repeat: times takes a literal or variable, got a flow","file":"solution.shs","line":4,"column":3,"shard":"Repeat","param_index":1,"param":"times","path":[{"function":"FindIndex"},{"shard":4,"name":"Repeat"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(FindIndex input: Int output: Int params: {sequence: [Int]} {
  0 | Var(index)
  -1 | Var(result)
  sequence | Count | Var(length)
  Repeat({
    When(
      predicate: { sequence | Take(index) | Is(input) }
      action: { 
        index | Update(result)
        result | Return
      }
    )
    Inc(index)
  } times: length)
  result
})

[10 20 30 40] = seq
30 | FindIndex(sequence: seq) | Log
99 | FindIndex(sequence: seq) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["2","-1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
