# 14-seq-reverse

## Task

Reverse the sequence [1 2 3 4 5] into a new sequence and log it (as one value).

## Reply 0

```shards
[1 2 3 4 5] = original
[] >= reversed-seq
original | Count = count
count | Math.Sub(1) >= i
Repeat({
  original | Take(i) | Push(reversed-seq)
  Dec(i)
} Until: { i | IsLess(0) })
reversed-seq | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"unknown-shard","code":"unknown-shard","message":"unknown shard `Math.Sub`","file":"solution.shs","line":4,"column":9,"shard":"Math.Sub","did_you_mean":["Math.Abs","Math.Add","Math.Dec"]}]}
```

## Reply 1

```shards
[1 2 3 4 5] = original
[] >= reversed-seq
original | Count = count
count | Sub(1) >= i
Repeat({
  original | Take(i) | Push(reversed-seq)
  Dec(i)
} Until: { i | IsLess(0) })
reversed-seq | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["[5 4 3 2 1]"],"outcomes":[{"wire":"root","outcome":"completed","value":"[5 4 3 2 1]"}],"spawned_failures":[]}
stderr:
```
