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

0 >= total
0 >= rejected

orders | Repeat({
  = order
  
  order.item = item
  
  Maybe({
    order.quantity | ParseInt | ExpectInt = qty
    qty | If(Predicate: {IsMore(0)} Then: {qty} Else: {none}) | ExpectInt
    prices | Take(item) | ExpectInt = price
    qty | Math.Multiply(price) = subtotal
    f"{item}: {subtotal}" | Log
    total | Math.Add(subtotal) > total
  }
  Else: {
    f"rejected: {item}" | Log
    Inc(rejected)
  }
  Silent: true)
})

f"total={total} rejected={rejected}" | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"wrong-key-type","message":"Take on [{item: String quantity: String}] needs a Int key, got String","file":"solution.shs","line":18,"column":9,"shard":"Take","param_index":0,"param":"Key","path":[{"wire":"root"},{"shard":9,"name":"Repeat"},{"param":"Action"},{"shard":2,"name":"Take"}]}]}
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

0 >= total
0 >= rejected
0 >= idx

orders | Count = num-orders

Repeat({
  orders | Take(idx) = order
  
  order.item = item
  
  Maybe({
    order.quantity | ParseInt | ExpectInt = qty
    qty | If(Predicate: {IsMore(0)} Then: {qty} Else: {none}) | ExpectInt
    prices | Take(item) | ExpectInt = price
    qty | Math.Multiply(price) = subtotal
    f"{item}: {subtotal}" | Log
    total | Math.Add(subtotal) > total
  }
  Else: {
    f"rejected: {item}" | Log
    Inc(rejected)
  }
  Silent: true)
  
  Inc(idx)
} Times: num-orders)

f"total={total} rejected={rejected}" | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["apple: 6","rejected: pear","rejected: missing","rejected: plum","pear: 15","rejected: apple","total=21 rejected=4"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"total=21 rejected=4\""}],"spawned_failures":[]}
stderr:
```
