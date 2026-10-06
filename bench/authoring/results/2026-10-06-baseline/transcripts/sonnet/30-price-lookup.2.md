# 30-price-lookup

## Task

Prices are stored in a table {apple: 3 pear: 5}. An order is the sequence ["apple" "pear" "apple"]. Look up each item's price by its name and log the order total.

## Reply 0

```shards
{apple: 3 pear: 5} = prices
["apple" "pear" "apple"] = order
order | Count = n
0 >= total
0 >= i
Repeat({
  order | Take(i) | ExpectString = name
  prices | Take(name) | ExpectInt | Math.Add(total) > total
  Inc(i)
} Times: n)
total | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["11"],"outcomes":[{"wire":"root","outcome":"completed","value":"11"}],"spawned_failures":[]}
stderr:
```
