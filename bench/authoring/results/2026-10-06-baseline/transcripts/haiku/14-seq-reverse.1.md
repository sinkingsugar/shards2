# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = original
[] >= result
original | Count = count
count >= i
Repeat({
  i | Dec > i
  original | Take(i) >> result
} Times: count)
result | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"Math.Dec: missing required parameter Variable","file":"solution.shs","line":6,"column":7,"shard":"Math.Dec","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":9,"name":"Repeat"},{"param":"Action"},{"shard":1,"name":"Math.Dec"}]}]}
```

## Reply 1

```shards
[1 2 3 4 5] = original
[] >= result
original | Count = count
count >= i
Repeat({
  Dec(i) > i
  original | Take(i) >> result
} Times: count)
result | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[5 4 3 2 1]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[5 4 3 2 1]"}],"spawned_failures":[]}
stderr:
```
