# 40-delimited-batches

## Task

Process ["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] in order. The string "end" closes a batch: log its valid-integer sum and invalid-string count as "sum=S invalid=E", then reset both counters. Other strings are parsed as integers; a parse failure increments the invalid count without changing the sum. Empty batches must be logged too. After the sequence ends, log the unfinished final batch only if it contains any entries. Use the same algorithm on a second sequence ["end" "2" "end"] to demonstrate that a trailing delimiter does not add an extra empty batch. Do not hardcode the output lines.

## Reply 0

```shards
["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1
seq1 | Count = len1

0 >= sum
0 >= invalid
0 >= i

Repeat({
  seq1 | Take(i) = item
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len1)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)

0 > sum
0 > invalid
0 > i

["end" "2" "end"] = seq2
seq2 | Count = len2

Repeat({
  seq2 | Take(i) = item
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len2)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"item already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":39,"column":18,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":24,"name":"Repeat"},{"param":"Action"},{"shard":2,"name":"Ref"}]}]}
```

## Reply 1

```shards
["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1
seq1 | Count = len1

0 >= sum
0 >= invalid
0 >= i
none >= item

Repeat({
  seq1 | Take(i) | Set(item)
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len1)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)

0 > sum
0 > invalid
0 > i

["end" "2" "end"] = seq2
seq2 | Count = len2

Repeat({
  seq2 | Take(i) | Set(item)
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len2)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-type-mismatch","message":"item is None, cannot assign String (the input comes from 1:Take)","file":"solution.shs","line":10,"column":20,"shard":"Set","param_index":0,"param":"Variable","input_from":{"origin":{"shard":1,"name":"Take"},"via":[]},"path":[{"wire":"root"},{"shard":13,"name":"Repeat"},{"param":"Action"},{"shard":2,"name":"Set"}]}]}
```

## Reply 2

```shards
["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1
seq1 | Count = len1

0 >= sum
0 >= invalid
0 >= i

Repeat({
  seq1 | Take(i) = item
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len1)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)

0 > sum
0 > invalid
0 > i

["end" "2" "end"] = seq2
seq2 | Count = len2

Repeat({
  seq2 | Take(i) = item
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len2)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"item already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":39,"column":18,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":24,"name":"Repeat"},{"param":"Action"},{"shard":2,"name":"Ref"}]}]}
```

## Reply 3

```shards
["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1
seq1 | Count = len1

0 >= sum
0 >= invalid
0 >= i

Repeat({
  seq1 | Take(i) >= item
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len1)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)

0 > sum
0 > invalid
0 > i

["end" "2" "end"] = seq2
seq2 | Count = len2

Repeat({
  seq2 | Take(i) > item
  If(item | Is("end")
    {
      ["sum=" sum " invalid=" invalid] | String.Format | Log
      0 > sum
      0 > invalid
    }
    {
      Maybe(
        { item | ParseInt | Add(sum) > sum }
        { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(i)
} Times: len2)

If(Any({sum | IsMore(0)} {invalid | IsMore(0)})
  { ["sum=" sum " invalid=" invalid] | String.Format | Log }
)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["sum=1 invalid=1","sum=0 invalid=0","sum=12 invalid=1","sum=4 invalid=0","sum=0 invalid=0","sum=2 invalid=0"],"outcomes":[{"wire":"root","outcome":"completed","value":"3"}],"spawned_failures":[]}
stderr:
```
