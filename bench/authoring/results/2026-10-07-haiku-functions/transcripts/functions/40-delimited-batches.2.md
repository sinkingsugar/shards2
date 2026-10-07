# 40-delimited-batches

## Task

Process ["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] in order. The string "end" closes a batch: log its valid-integer sum and invalid-string count as "sum=S invalid=E", then reset both counters. Other strings are parsed as integers; a parse failure increments the invalid count without changing the sum. Empty batches must be logged too. After the sequence ends, log the unfinished final batch only if it contains any entries. Use the same algorithm on a second sequence ["end" "2" "end"] to demonstrate that a trailing delimiter does not add an extra empty batch. Do not hardcode the output lines.

## Reply 0

```shards
@fn(ProcessSequence input: [String] output: None params: {} {
  0 | Var(sum)
  0 | Var(invalid)
  0 | Var(entries)
  0 | Var(i)
  false | Var(parsed-ok)
  0 | Var(parsed-value)
  
  While({ i | IsLess(input | Count) }
    {
      input | Take(i) = item
      If(item Is "end"
        then: {
          f"sum={sum} invalid={invalid}" | Log
          0 | Update(sum)
          0 | Update(invalid)
          0 | Update(entries)
        }
        else: {
          item | Maybe(
            { ParseInt | Update(parsed-value) true | Update(parsed-ok) }
            { false | Update(parsed-ok) }
            silent: true
          )
          If(parsed-ok
            then: { parsed-value | Math.Add(sum) | Update(sum) }
            else: { invalid | Math.Add(1) | Update(invalid) }
          )
          entries | Math.Add(1) | Update(entries)
        }
      )
      Inc(i)
    }
  )
  
  If(entries IsMore(0)
    then: { f"sum={sum} invalid={invalid}" | Log }
  )
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | ProcessSequence

["end" "2" "end"] | ProcessSequence
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"duplicate-argument","message":"If: then given more than once","file":"solution.shs","line":12,"column":7,"shard":"If","param_index":1,"param":"then","path":[{"wire":"root"},{"shard":1,"name":"ProcessSequence"},{"function":"ProcessSequence"},{"shard":12,"name":"While"},{"param":"action"},{"shard":3,"name":"If"}]},{"phase":"construct","severity":"error","kind":"generic","code":"duplicate-argument","message":"If: then given more than once","file":"solution.shs","line":12,"column":7,"shard":"If","param_index":1,"param":"then","path":[{"function":"ProcessSequence"},{"shard":12,"name":"While"},{"param":"action"},{"shard":3,"name":"If"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(ProcessSequence input: [String] output: None params: {} {
  0 | Var(sum)
  0 | Var(invalid)
  0 | Var(entries)
  0 | Var(i)
  false | Var(parsed-ok)
  0 | Var(parsed-value)
  
  While({ i | IsLess(input | Count) }
    {
      input | Take(i) = item
      If({ item Is "end" }
        then: {
          f"sum={sum} invalid={invalid}" | Log
          0 | Update(sum)
          0 | Update(invalid)
          0 | Update(entries)
        }
        else: {
          item | Maybe(
            { ParseInt | Update(parsed-value) true | Update(parsed-ok) }
            { false | Update(parsed-ok) }
            silent: true
          )
          If({ parsed-ok }
            then: { parsed-value | Math.Add(sum) | Update(sum) }
            else: { invalid | Math.Add(1) | Update(invalid) }
          )
          entries | Math.Add(1) | Update(entries)
        }
      )
      Inc(i)
    }
  )
  
  If({ entries | IsMore(0) }
    then: { f"sum={sum} invalid={invalid}" | Log }
  )
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | ProcessSequence

["end" "2" "end"] | ProcessSequence
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"Is: missing required parameter operand","file":"solution.shs","line":12,"column":17,"shard":"Is","param_index":0,"param":"operand","path":[{"wire":"root"},{"shard":1,"name":"ProcessSequence"},{"function":"ProcessSequence"},{"shard":12,"name":"While"},{"param":"action"},{"shard":3,"name":"If"},{"param":"predicate"},{"shard":1,"name":"Is"}]},{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"Is: missing required parameter operand","file":"solution.shs","line":12,"column":17,"shard":"Is","param_index":0,"param":"operand","path":[{"function":"ProcessSequence"},{"shard":12,"name":"While"},{"param":"action"},{"shard":3,"name":"If"},{"param":"predicate"},{"shard":1,"name":"Is"}]}],"wires":[],"functions":[]}
```

## Reply 2

```shards
@fn(ProcessSequence input: [String] output: None params: {} {
  0 | Var(sum)
  0 | Var(invalid)
  0 | Var(entries)
  0 | Var(i)
  0 | Var(parsed-value)
  
  While({ i | IsLess(input | Count) }
    {
      input | Take(i) = item
      If({ item | Is("end") }
        then: {
          f"sum={sum} invalid={invalid}" | Log
          0 | Update(sum)
          0 | Update(invalid)
          0 | Update(entries)
        }
        else: {
          If({
            item | Maybe(
              { ParseInt | Update(parsed-value) true }
              { false }
              silent: true
            )
          }
          then: { parsed-value | Math.Add(sum) | Update(sum) }
          else: { invalid | Math.Add(1) | Update(invalid) }
          )
          Inc(entries)
        }
      )
      Inc(i)
    }
  )
  
  If({ entries | IsMore(0) }
    then: { f"sum={sum} invalid={invalid}" | Log }
  )
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | ProcessSequence

["end" "2" "end"] | ProcessSequence
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"ProcessSequence declares output None but its body outputs Int | String","file":"solution.shs","line":1,"column":1,"shard":"ProcessSequence","actual":{"name":"Int | String","basic_type":-1},"expected":[{"name":"None","basic_type":0}],"path":[{"wire":"root"},{"shard":1,"name":"ProcessSequence"},{"function":"ProcessSequence"}]},{"phase":"compose","severity":"error","kind":"compose-error","code":"output-type-mismatch","message":"ProcessSequence declares output None but its body outputs Int | String","file":"solution.shs","line":1,"column":1,"shard":"ProcessSequence","actual":{"name":"Int | String","basic_type":-1},"expected":[{"name":"None","basic_type":0}],"path":[{"function":"ProcessSequence"}]}],"wires":[],"functions":[]}
```

## Reply 3

```shards
@fn(ProcessSequence input: [String] output: None params: {} {
  0 | Var(sum)
  0 | Var(invalid)
  0 | Var(entries)
  0 | Var(i)
  0 | Var(parsed-value)
  
  While({ i | IsLess(input | Count) }
    {
      input | Take(i) = item
      If({ item | Is("end") }
        then: {
          f"sum={sum} invalid={invalid}" | Log
          0 | Update(sum)
          0 | Update(invalid)
          0 | Update(entries)
        }
        else: {
          If({
            item | Maybe(
              { ParseInt | Update(parsed-value) true }
              { false }
              silent: true
            )
          }
          then: { parsed-value | Math.Add(sum) | Update(sum) }
          else: { invalid | Math.Add(1) | Update(invalid) }
          )
          Inc(entries)
        }
      )
      Inc(i)
    }
  )
  
  If({ entries | IsMore(0) }
    then: { f"sum={sum} invalid={invalid}" | Log }
  )
  
  none
})

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] | ProcessSequence

["end" "2" "end"] | ProcessSequence
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["sum=1 invalid=1","sum=0 invalid=0","sum=12 invalid=1","sum=4 invalid=0","sum=0 invalid=0","sum=2 invalid=0"],"outcomes":[{"wire":"root","outcome":"completed","value":"none"}],"spawned_failures":[]}
stderr:
```
