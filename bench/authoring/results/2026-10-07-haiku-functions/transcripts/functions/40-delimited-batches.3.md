# 40-delimited-batches

## Task

Process ["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] in order. The string "end" closes a batch: log its valid-integer sum and invalid-string count as "sum=S invalid=E", then reset both counters. Other strings are parsed as integers; a parse failure increments the invalid count without changing the sum. Empty batches must be logged too. After the sequence ends, log the unfinished final batch only if it contains any entries. Use the same algorithm on a second sequence ["end" "2" "end"] to demonstrate that a trailing delimiter does not add an extra empty batch. Do not hardcode the output lines.

## Reply 0

```shards
0 | Var(sum)
0 | Var(invalid)
0 | Var(i)
false | Var(has-entries)

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1

Repeat({
  seq1 | Take(i)
  If(Is("end")
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      true | Update(has-entries)
      Maybe(
        { ParseInt | Math.Add(sum) | Update(sum) }
        { Inc(invalid) }
        silent: true
      )
    }
  )
  Inc(i)
} times: (seq1 | Count))

When(has-entries { f"sum={sum} invalid={invalid}" | Log })

0 | Update(sum)
0 | Update(invalid)
0 | Update(i)
false | Update(has-entries)

["end" "2" "end"] = seq2

Repeat({
  seq2 | Take(i)
  If(Is("end")
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      true | Update(has-entries)
      Maybe(
        { ParseInt | Math.Add(sum) | Update(sum) }
        { Inc(invalid) }
        silent: true
      )
    }
  )
  Inc(i)
} times: (seq2 | Count))

When(has-entries { f"sum={sum} invalid={invalid}" | Log })
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"ParseInt needs String input, got Bool (the input is the flow's own input)","file":"solution.shs","line":20,"column":11,"shard":"ParseInt","actual":{"name":"Bool","basic_type":3},"expected":[{"name":"String","basic_type":52}],"input_from":{"origin":"flow-input","via":[]},"path":[{"wire":"root"},{"shard":11,"name":"Repeat"},{"param":"action"},{"shard":2,"name":"If"},{"param":"else"},{"shard":2,"name":"Maybe"},{"param":"action"},{"shard":0,"name":"ParseInt"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
0 | Var(sum)
0 | Var(invalid)
0 | Var(i)
false | Var(has-entries)

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1

Repeat({
  seq1 | Take(i)
  If(Is("end")
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      SubFlow({ true | Update(has-entries) })
      Maybe(
        { ParseInt | Math.Add(sum) | Update(sum) }
        { Inc(invalid) }
        silent: true
      )
    }
  )
  Inc(i)
} times: (seq1 | Count))

When(has-entries { f"sum={sum} invalid={invalid}" | Log })

0 | Update(sum)
0 | Update(invalid)
0 | Update(i)
false | Update(has-entries)

["end" "2" "end"] = seq2

Repeat({
  seq2 | Take(i)
  If(Is("end")
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      SubFlow({ true | Update(has-entries) })
      Maybe(
        { ParseInt | Math.Add(sum) | Update(sum) }
        { Inc(invalid) }
        silent: true
      )
    }
  )
  Inc(i)
} times: (seq2 | Count))

When(has-entries { f"sum={sum} invalid={invalid}" | Log })
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["sum=1 invalid=1","sum=0 invalid=0","sum=12 invalid=1","sum=4 invalid=0","sum=0 invalid=0","sum=2 invalid=0"],"outcomes":[{"wire":"root","outcome":"completed","value":"[\"end\" \"2\" \"end\"]"}],"spawned_failures":[]}
stderr:
```
