# 30-price-lookup

## Task

Prices are stored in a table {apple: 3 pear: 5}. An order is the sequence ["apple" "pear" "apple"]. Look up each item's price by its name and log the order total.

## Reply 0

```shards
{apple: 3 pear: 5} = prices
["apple" "pear" "apple"] = order
0 >= total
0 >= i

Repeat(
  {
    order | Take(i) = item
    prices | Take(item) | Add(total) > total
    Inc(i)
  }
  Times: 3
)

total | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Math.Add needs Int, Float, Float2, Float3 or Float4 input, got Int | None (the input comes from 4:Take)","file":"solution.shs","line":9,"column":27,"shard":"Math.Add","actual":{"name":"Int | None","basic_type":-1},"expected":[{"name":"Int","basic_type":4},{"name":"Float","basic_type":10},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"input_from":{"origin":{"shard":4,"name":"Take"},"via":[]},"path":[{"wire":"root"},{"shard":8,"name":"Repeat"},{"param":"Action"},{"shard":5,"name":"Math.Add"}]}]}
```

## Reply 1

```shards
{apple: 3 pear: 5} = prices
["apple" "pear" "apple"] = order
0 >= total
0 >= i

Repeat(
  {
    order | Take(i) = item
    prices | Take(item) | ExpectInt | Add(total) > total
    Inc(i)
  }
  Times: 3
)

total | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["11"],"outcomes":[{"wire":"root","outcome":"completed","value":"11"}],"spawned_failures":[]}
stderr:
```
