# 15-struct-transform

## Task

Start from the table {name: "ann" age: 41}. Build a new table with the same name, the age increased by one, and a Bool field `adult` that is true when the new age is at least 18. Log the new table.

## Reply 0

```shards
{name: "ann" age: 41} = person
person.name = name
person.age | Math.Add(1) = new-age
new-age | IsMoreEqual(18) = adult
{name: name age: new-age adult: adult} | Log
```

## Run (pass)

```
exit Some(0)
stdout:
{adult: true age: 42 name: ann}
root: {adult: true age: 42 name: "ann"}
stderr:
```
