# 15-struct-transform

## Task

Start from the table {name: "ann" age: 41}. Build a new table with the same name, the age increased by one, and a Bool field `adult` that is true when the new age is at least 18. Log the new table.

## Reply 0

```shards
{name: "ann" age: 41} = original
original | Take("name") = name
original | Take("age") = age
age | Add(1) = new-age
new-age | IsMoreEqual(18) = adult
Table.Make(["name" "age" "adult"] name new-age adult) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["{adult: true age: 42 name: ann}"],"outcomes":[{"wire":"root","outcome":"completed","value":"{adult: true age: 42 name: \"ann\"}"}],"spawned_failures":[]}
stderr:
```
