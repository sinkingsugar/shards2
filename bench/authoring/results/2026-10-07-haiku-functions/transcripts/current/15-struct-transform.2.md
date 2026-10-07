# 15-struct-transform

## Task

Start from the table {name: "ann" age: 41}. Build a new table with the same name, the age increased by one, and a Bool field `adult` that is true when the new age is at least 18. Log the new table.

## Reply 0

```shards
{name: "ann" age: 41} = original-table
original-table.name = name
original-table.age | Math.Add(1) = new-age
new-age | IsMoreEqual(18) = is-adult
Table.Make(Keys: ["name" "age" "adult"] Values: name new-age is-adult) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"variadic-by-name","message":"Table.Make: Values takes the remaining positional arguments; it cannot be given by name","file":"solution.shs","line":5,"column":1,"shard":"Table.Make","param_index":1,"param":"Values","path":[{"wire":"root"},{"shard":12,"name":"Table.Make"}]}]}
```

## Reply 1

```shards
{name: "ann" age: 41} = original-table
original-table.name = name
original-table.age | Math.Add(1) = new-age
new-age | IsMoreEqual(18) = is-adult
Table.Make(Keys: ["name" "age" "adult"] name new-age is-adult) | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"positional-after-named","message":"Table.Make: positional argument 1 after a named one","file":"solution.shs","line":5,"column":1,"shard":"Table.Make","path":[{"wire":"root"},{"shard":12,"name":"Table.Make"}]}]}
```

## Reply 2

```shards
{name: "ann" age: 41} = original-table
original-table.name = name
original-table.age | Math.Add(1) = new-age
new-age | IsMoreEqual(18) = is-adult
Table.Make(["name" "age" "adult"] name new-age is-adult) | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["{adult: true age: 42 name: ann}"],"outcomes":[{"wire":"root","outcome":"completed","value":"{adult: true age: 42 name: \"ann\"}"}],"spawned_failures":[]}
stderr:
```
