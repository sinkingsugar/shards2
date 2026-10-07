# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = original
[original | Count] = length
[] >= result
0 >= i

Repeat({
  original | Take(length | Sub(i) | Sub(1)) | Push(result)
  Inc(i)
} Times: 5)

result | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Math.Subtract needs Int, Float, Float2, Float3 or Float4 input, got [Int] (the input comes from 0:Get)","file":"solution.shs","line":7,"column":28,"shard":"Math.Subtract","actual":{"name":"[Int]","basic_type":56},"expected":[{"name":"Int","basic_type":4},{"name":"Float","basic_type":10},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"input_from":{"origin":{"shard":0,"name":"Get"},"via":[]},"path":[{"wire":"root"},{"shard":9,"name":"Repeat"},{"param":"Action"},{"shard":1,"name":"SubFlow"},{"param":"Action"},{"shard":1,"name":"Math.Subtract"}]}]}
```

## Reply 1

```shards
[1 2 3 4 5] = original
original | Count = length
[] >= result
0 >= i

Repeat({
  length | Sub(i) | Sub(1) = idx
  original | Take(idx) | Push(result)
  Inc(i)
} Times: 5)

result | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[5 4 3 2 1]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[5 4 3 2 1]"}],"spawned_failures":[]}
stderr:
```
