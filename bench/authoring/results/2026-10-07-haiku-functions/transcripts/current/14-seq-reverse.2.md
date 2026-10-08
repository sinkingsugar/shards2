# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = original
[] >= reversed
0 >= i
original | Count = length
Repeat({
  length | Math.Sub(1) | Math.Sub(i) = idx
  original | Take(idx) | Push(reversed)
  Inc(i)
} Times: length)
reversed | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"unknown-shard","code":"unknown-shard","message":"unknown shard `Math.Sub`","file":"solution.shs","line":6,"column":12,"shard":"Math.Sub","did_you_mean":["Math.Abs","Math.Add","Math.Dec"]},{"phase":"construct","severity":"error","kind":"unknown-shard","code":"unknown-shard","message":"unknown shard `Math.Sub`","file":"solution.shs","line":6,"column":26,"shard":"Math.Sub","did_you_mean":["Math.Abs","Math.Add","Math.Dec"]}]}
```

## Reply 1

```shards
[1 2 3 4 5] = original
[] >= reversed
0 >= i
original | Count = length
Repeat({
  length | Sub(1) | Sub(i) = idx
  original | Take(idx) | Push(reversed)
  Inc(i)
} Times: length)
reversed | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[5 4 3 2 1]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[5 4 3 2 1]"}],"spawned_failures":[]}
stderr:
```
