# 40-delimited-batches

## Task

Process ["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] in order. The string "end" closes a batch: log its valid-integer sum and invalid-string count as "sum=S invalid=E", then reset both counters. Other strings are parsed as integers; a parse failure increments the invalid count without changing the sum. Empty batches must be logged too. After the sequence ends, log the unfinished final batch only if it contains any entries. Use the same algorithm on a second sequence ["end" "2" "end"] to demonstrate that a trailing delimiter does not add an extra empty batch. Do not hardcode the output lines.

## Reply 0

```shards
0 | Var(sum)
0 | Var(invalid)
false | Var(has-entries)

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1
0 | Var(idx)

Repeat({
  seq1 | Take(idx) | If(
    {Is("end")}
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      Maybe(
        {ParseInt | Math.Add(sum) | Update(sum)}
        {invalid | Math.Add(1) | Update(invalid)}
        silent: true
      )
      true | Update(has-entries)
    }
  )
  idx | Inc
} times: (seq1 | Count))

If({has-entries} {f"sum={sum} invalid={invalid}" | Log})

0 | Update(sum)
0 | Update(invalid)
false | Update(has-entries)

["end" "2" "end"] = seq2
0 | Update(idx)

Repeat({
  seq2 | Take(idx) | If(
    {Is("end")}
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      Maybe(
        {ParseInt | Math.Add(sum) | Update(sum)}
        {invalid | Math.Add(1) | Update(invalid)}
        silent: true
      )
      true | Update(has-entries)
    }
  )
  idx | Inc
} times: (seq2 | Count))

If({has-entries} {f"sum={sum} invalid={invalid}" | Log})
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"Math.Inc: missing required parameter variable","file":"solution.shs","line":26,"column":9,"shard":"Math.Inc","param_index":0,"param":"variable","path":[{"wire":"root"},{"shard":11,"name":"Repeat"},{"param":"action"},{"shard":4,"name":"Math.Inc"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
0 | Var(sum)
0 | Var(invalid)
false | Var(has-entries)

["3" "bad" "-2" "end" "end" "5" "oops" "7" "end" "4"] = seq1
0 | Var(idx)

Repeat({
  seq1 | Take(idx) | If(
    {Is("end")}
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      Maybe(
        {ParseInt | Math.Add(sum) | Update(sum)}
        {invalid | Math.Add(1) | Update(invalid)}
        silent: true
      )
      true | Update(has-entries)
    }
  )
  Inc(idx)
} times: (seq1 | Count))

If({has-entries} {f"sum={sum} invalid={invalid}" | Log})

0 | Update(sum)
0 | Update(invalid)
false | Update(has-entries)

["end" "2" "end"] = seq2
0 | Update(idx)

Repeat({
  seq2 | Take(idx) | If(
    {Is("end")}
    {
      f"sum={sum} invalid={invalid}" | Log
      0 | Update(sum)
      0 | Update(invalid)
      false | Update(has-entries)
    }
    {
      Maybe(
        {ParseInt | Math.Add(sum) | Update(sum)}
        {invalid | Math.Add(1) | Update(invalid)}
        silent: true
      )
      true | Update(has-entries)
    }
  )
  Inc(idx)
} times: (seq2 | Count))

If({has-entries} {f"sum={sum} invalid={invalid}" | Log})
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["sum=1 invalid=1","sum=0 invalid=0","sum=12 invalid=1","sum=4 invalid=0","sum=0 invalid=0","sum=2 invalid=0"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
