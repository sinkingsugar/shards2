# 40-delimited-batches

## Task

Process ["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] in order. The string "end" closes a batch: log its valid-integer sum and invalid-string count as "sum=S invalid=E", then reset both counters. Other strings are parsed as integers; a parse failure increments the invalid count without changing the sum. Empty batches must be logged too. After the sequence ends, log the unfinished final batch only if it contains any entries. Use the same algorithm on a second sequence ["end" "2" "end"] to demonstrate that a trailing delimiter does not add an extra empty batch. Do not hardcode the output lines.

## Reply 0

```shards
@wire(process-sequence {
  0 >= sum
  0 >= invalid
  false >= has-entries
  =seq
  0 >= i
  
  seq | Count >= count
  
  Repeat({
    seq | Take(i) | ExpectString = item
    If({ item | Is("end") }
      {
        f"sum={sum} invalid={invalid}" | Log
        0 > sum
        0 > invalid
        false > has-entries
      }
      {
        Maybe(
          {
            item | ParseInt | Add(sum) > sum
            true > has-entries
          }
          {
            invalid | Add(1) > invalid
            true > has-entries
          }
          Silent: true
        )
      }
    )
    Inc(i)
  } Times: count)
  
  When(has-entries { f"sum={sum} invalid={invalid}" | Log })
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | Do(process-sequence)
["end" "2" "end"] | Do(process-sequence)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Count needs Seq, Table or String input, got Bool (the input comes from 9:Get)","file":"solution.shs","line":8,"column":9,"shard":"Count","actual":{"name":"Bool","basic_type":3},"expected":[{"name":"Seq","basic_type":56},{"name":"Table","basic_type":57},{"name":"String","basic_type":52}],"input_from":{"origin":{"shard":9,"name":"Get"},"via":[]},"path":[{"wire":"root"},{"shard":1,"name":"Do"},{"param":"Wire"},{"wire":"process-sequence"},{"shard":10,"name":"Count"}]}]}
```

## Reply 1

```shards
@wire(process-sequence {
  = seq
  0 >= sum
  0 >= invalid
  false >= has-entries
  0 >= i
  
  seq | Count >= count
  
  Repeat({
    seq | Take(i) | ExpectString = item
    If({ item | Is("end") }
      {
        f"sum={sum} invalid={invalid}" | Log
        0 > sum
        0 > invalid
        false > has-entries
      }
      {
        Maybe(
          {
            item | ParseInt | Add(sum) > sum
            true > has-entries
          }
          {
            invalid | Add(1) > invalid
            true > has-entries
          }
          Silent: true
        )
      }
    )
    Inc(i)
  } Times: count)
  
  When(has-entries { f"sum={sum} invalid={invalid}" | Log })
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | Do(process-sequence)
["end" "2" "end"] | Do(process-sequence)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"seq already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":2,"column":3,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":3,"name":"Do"},{"param":"Wire"},{"wire":"process-sequence"},{"shard":0,"name":"Ref"}]}]}
```

## Reply 2

```shards
@wire(process-sequence {
  >= seq
  0 >= sum
  0 >= invalid
  false >= has-entries
  0 >= i
  
  seq | Count >= count
  
  Repeat({
    seq | Take(i) | ExpectString = item
    If({ item | Is("end") }
      {
        f"sum={sum} invalid={invalid}" | Log
        0 > sum
        0 > invalid
        false > has-entries
      }
      {
        Maybe(
          {
            item | ParseInt | Add(sum) > sum
            true > has-entries
          }
          {
            invalid | Add(1) > invalid
            true > has-entries
          }
          Silent: true
        )
      }
    )
    Inc(i)
  } Times: count)
  
  When(has-entries { f"sum={sum} invalid={invalid}" | Log })
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | Do(process-sequence)
["end" "2" "end"] | Do(process-sequence)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"item already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":11,"column":34,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":3,"name":"Do"},{"param":"Wire"},{"wire":"process-sequence"},{"shard":12,"name":"Repeat"},{"param":"Action"},{"shard":3,"name":"Ref"}]}]}
```

## Reply 3

```shards
@wire(process-sequence {
  >= seq
  0 >= sum
  0 >= invalid
  false >= has-entries
  0 >= i
  
  seq | Count >= count
  
  Repeat({
    seq | Take(i) | ExpectString >= item
    If({ item | Is("end") }
      {
        f"sum={sum} invalid={invalid}" | Log
        0 > sum
        0 > invalid
        false > has-entries
      }
      {
        Maybe(
          {
            item | ParseInt | Add(sum) > sum
            true > has-entries
          }
          {
            invalid | Add(1) > invalid
            true > has-entries
          }
          Silent: true
        )
      }
    )
    Inc(i)
  } Times: count)
  
  When(has-entries { f"sum={sum} invalid={invalid}" | Log })
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | Do(process-sequence)
["end" "2" "end"] | Do(process-sequence)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["sum=1 invalid=1","sum=0 invalid=0","sum=12 invalid=1","sum=4 invalid=0","sum=0 invalid=0","sum=2 invalid=0"],"outcomes":[{"wire":"root","outcome":"completed","value":"3"}],"spawned_failures":[]}
stderr:
```
