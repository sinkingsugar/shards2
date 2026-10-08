# 41-inventory-orders

## Task

Prices are {apple: 3 pear: 5 plum: 2}. Process these orders in order: {item: "apple" quantity: "2"}, {item: "pear" quantity: "bad"}, {item: "missing" quantity: "4"}, {item: "plum" quantity: "-1"}, {item: "pear" quantity: "3"}, {item: "apple" quantity: "0"}. Accept an order only if its item exists and its quantity parses as a strictly positive integer. For each accepted order log "ITEM: SUBTOTAL" and add its subtotal to the total; for every other order log "rejected: ITEM" and increment a rejection count. Finally log "total=T rejected=R". Unknown items and malformed quantities must not terminate the program. Build the result from the input records, not literal output lines.

## Reply 0

```shards
{apple: 3 pear: 5 plum: 2} = prices
[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

0 | Var(total)
0 | Var(rejected)

orders | Repeat({
  . = order
  order.item = item
  order.quantity = quantity-str
  
  quantity-str | Maybe({ ParseInt } { none } silent: true) = qty
  prices | Take(item) = price
  
  qty | Maybe({ ExpectInt | IsMore(0) } { false } silent: true) = qty-valid
  price | IsNotNone = price-valid
  
  If(
    predicate: { All(qty-valid price-valid) }
    then: {
      price | ExpectInt = price-int
      qty | ExpectInt = qty-int
      price-int | Math.Multiply(qty-int) = subtotal
      f"{item}: {subtotal}" | Log
      subtotal | Math.Add(total) | Update(total)
    }
    else: {
      f"rejected: {item}" | Log
      rejected | Math.Add(1) | Update(rejected)
    }
  )
} times: (orders | Count))

f"total={total} rejected={rejected}" | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"parse","severity":"error","kind":"syntax","code":"expected-value","message":"expected a shard or a value, found `.`","file":"solution.shs","line":15,"column":3}],"wires":[],"functions":[]}
```

## Reply 1

```shards
{apple: 3 pear: 5 plum: 2} = prices
[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

0 | Var(total)
0 | Var(rejected)
0 | Var(i)

Repeat({
  orders | Take(i) = order
  order | Take("item") = item
  order | Take("quantity") = qty-str
  
  qty-str | Maybe({ ParseInt } { none } silent: true) = qty
  prices | Take(item) = price
  
  qty | Maybe({ ExpectInt | IsMore(0) } { false } silent: true) = qty-valid
  price | IsNotNone = price-valid
  
  If(
    predicate: { All(qty-valid price-valid) }
    then: {
      price | ExpectInt = price-int
      qty | ExpectInt = qty-int
      price-int | Math.Multiply(qty-int) = subtotal
      f"{item}: {subtotal}" | Log
      subtotal | Math.Add(total) | Update(total)
    }
    else: {
      f"rejected: {item}" | Log
      rejected | Math.Add(1) | Update(rejected)
    }
  )
  
  Inc(i)
} times: (orders | Count))

f"total={total} rejected={rejected}" | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["apple: 6","rejected: pear","rejected: missing","rejected: plum","pear: 15","rejected: apple","total=21 rejected=4"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"total=21 rejected=4\""}],"spawned_failures":[]}
stderr:
```
