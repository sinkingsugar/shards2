# 40-delimited-batches

## Task

Process ["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] in order. The string "end" closes a batch: log its valid-integer sum and invalid-string count as "sum=S invalid=E", then reset both counters. Other strings are parsed as integers; a parse failure increments the invalid count without changing the sum. Empty batches must be logged too. After the sequence ends, log the unfinished final batch only if it contains any entries. Use the same algorithm on a second sequence ["end" "2" "end"] to demonstrate that a trailing delimiter does not add an extra empty batch. Do not hardcode the output lines.

## Reply 0

```shards
@wire(process-batch {
  f"sum={sum} invalid={invalid}" | Log
})

0 >= sum
0 >= invalid
false >= has-entries
0 >= idx

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq
Repeat({
  seq | Take(idx) = item
  item "end" Is If(
    Predicate: { true }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      item ParseInt Maybe(
        Action: { 
          = parsed_int
          Get(sum) | Add(parsed_int) | Update(sum)
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 10)

has-entries If(
  Predicate: { true }
  Then: { Do(process-batch) }
)

0 > sum
0 > invalid
false > has-entries
0 > idx

["end" "2" "end"] = seq2
Repeat({
  seq2 | Take(idx) = item
  item "end" Is If(
    Predicate: { true }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      item ParseInt Maybe(
        Action: { 
          = parsed_int
          Get(sum) | Add(parsed_int) | Update(sum)
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 3)

has-entries If(
  Predicate: { true }
  Then: { Do(process-batch) }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"Is: missing required parameter Operand","file":"solution.shs","line":13,"column":14,"shard":"Is","param_index":0,"param":"Operand","path":[{"wire":"root"},{"shard":10,"name":"Repeat"},{"param":"Action"},{"shard":5,"name":"Is"}]}]}
```

## Reply 1

```shards
@wire(process-batch {
  f"sum={sum} invalid={invalid}" | Log
})

0 >= sum
0 >= invalid
false >= has-entries
0 >= idx

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq
Repeat({
  seq | Take(idx) = item
  If(
    Predicate: { item | Is("end") }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      Maybe(
        Action: {
          item | ParseInt = parsed_int
          sum | Add(parsed_int) > sum
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 10)

If(
  Predicate: { has-entries }
  Then: { Do(process-batch) }
)

0 > sum
0 > invalid
false > has-entries
0 > idx

["end" "2" "end"] = seq2
Repeat({
  seq2 | Take(idx) = item
  If(
    Predicate: { item | Is("end") }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      Maybe(
        Action: {
          item | ParseInt = parsed_int
          sum | Add(parsed_int) > sum
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 3)

If(
  Predicate: { has-entries }
  Then: { Do(process-batch) }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"item already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":48,"column":20,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":22,"name":"Repeat"},{"param":"Action"},{"shard":2,"name":"Ref"}]}]}
```

## Reply 2

```shards
@wire(process-batch {
  f"sum={sum} invalid={invalid}" | Log
})

0 >= sum
0 >= invalid
false >= has-entries
0 >= idx

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq
Repeat({
  seq | Take(idx) = item
  If(
    Predicate: { item | Is("end") }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      Maybe(
        Action: {
          item | ParseInt = parsed_int
          sum | Add(parsed_int) > sum
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 10)

If(
  Predicate: { has-entries }
  Then: { Do(process-batch) }
)

0 > sum
0 > invalid
false > has-entries
0 > idx

["end" "2" "end"] = seq2
Repeat({
  seq2 | Take(idx) = item2
  If(
    Predicate: { item2 | Is("end") }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      Maybe(
        Action: {
          item2 | ParseInt = parsed_int
          sum | Add(parsed_int) > sum
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 3)

If(
  Predicate: { has-entries }
  Then: { Do(process-batch) }
)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"variable-exists","message":"parsed_int already exists; Ref declares a new variable (use Set or Update to assign a mutable one)","file":"solution.shs","line":61,"column":28,"shard":"Ref","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":22,"name":"Repeat"},{"param":"Action"},{"shard":3,"name":"If"},{"param":"Else"},{"shard":2,"name":"Maybe"},{"param":"Action"},{"shard":2,"name":"Ref"}]}]}
```

## Reply 3

```shards
@wire(process-batch {
  f"sum={sum} invalid={invalid}" | Log
})

0 >= sum
0 >= invalid
false >= has-entries
0 >= idx

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq
Repeat({
  seq | Take(idx) = item
  If(
    Predicate: { item | Is("end") }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      Maybe(
        Action: {
          item | ParseInt >= parsed_int
          sum | Add(parsed_int) > sum
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 10)

If(
  Predicate: { has-entries }
  Then: { Do(process-batch) }
)

0 > sum
0 > invalid
false > has-entries
0 > idx

["end" "2" "end"] = seq2
Repeat({
  seq2 | Take(idx) = item2
  If(
    Predicate: { item2 | Is("end") }
    Then: {
      Do(process-batch)
      0 > sum
      0 > invalid
      false > has-entries
    }
    Else: {
      true > has-entries
      Maybe(
        Action: {
          item2 | ParseInt > parsed_int
          sum | Add(parsed_int) > sum
        }
        Else: { Inc(invalid) }
        Silent: true
      )
    }
  )
  Inc(idx)
} Times: 3)

If(
  Predicate: { has-entries }
  Then: { Do(process-batch) }
)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["sum=1 invalid=1","sum=0 invalid=0","sum=12 invalid=1","sum=4 invalid=0","sum=0 invalid=0","sum=2 invalid=0"],"outcomes":[{"wire":"root","outcome":"completed","value":"[\"end\" \"2\" \"end\"]"}],"spawned_failures":[]}
stderr:
```
