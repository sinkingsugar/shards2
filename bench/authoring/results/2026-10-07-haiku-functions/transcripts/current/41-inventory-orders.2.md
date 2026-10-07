# 41-inventory-orders

## Task

Prices are {apple: 3 pear: 5 plum: 2}. Process these orders in order: {item: "apple" quantity: "2"}, {item: "pear" quantity: "bad"}, {item: "missing" quantity: "4"}, {item: "plum" quantity: "-1"}, {item: "pear" quantity: "3"}, {item: "apple" quantity: "0"}. Accept an order only if its item exists and its quantity parses as a strictly positive integer. For each accepted order log "ITEM: SUBTOTAL" and add its subtotal to the total; for every other order log "rejected: ITEM" and increment a rejection count. Finally log "total=T rejected=R". Unknown items and malformed quantities must not terminate the program. Build the result from the input records, not literal output lines.

## Reply 0

```shards
{apple: 3 pear: 5 plum: 2} = prices
0 >= total
0 >= rejection-count
0 >= i

[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

Repeat({
  orders | Take(i) = order
  order | Take("item") = item
  order | Take("quantity") = quantity-str
  
  Maybe({
    quantity-str | ParseInt = parsed-quantity
    prices | Take(item) = price
    
    If(
      All(
        price | IsNotNone
        parsed-quantity | IsMore(0)
      )
      {
        price | Math.Multiply(parsed-quantity) = subtotal
        f"{item}: {subtotal}" | Log
        subtotal | Math.Add(total) > total
      }
      {
        f"rejected: {item}" | Log
        Math.Inc(rejection-count)
      }
    )
  }
  {
    f"rejected: {item}" | Log
    Math.Inc(rejection-count)
  })
  
  Math.Inc(i)
} Times: 6)

f"total={total} rejected={rejection-count}" | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Math.Multiply needs Int, Float, Float2, Float3 or Float4 input, got Int | None (the input comes from 0:Get)","file":"solution.shs","line":30,"column":17,"shard":"Math.Multiply","actual":{"name":"Int | None","basic_type":-1},"expected":[{"name":"Int","basic_type":4},{"name":"Float","basic_type":10},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"input_from":{"origin":{"shard":0,"name":"Get"},"via":[]},"path":[{"wire":"root"},{"shard":10,"name":"Repeat"},{"param":"Action"},{"shard":9,"name":"Maybe"},{"param":"Action"},{"shard":6,"name":"If"},{"param":"Then"},{"shard":1,"name":"Math.Multiply"}]}]}
```

## Reply 1

```shards
{apple: 3 pear: 5 plum: 2} = prices
0 >= total
0 >= rejection-count
0 >= i

[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

Repeat({
  orders | Take(i) = order
  order | Take("item") = item
  order | Take("quantity") = quantity-str
  
  Maybe({
    quantity-str | ParseInt = parsed-quantity
    If(
      parsed-quantity | IsMore(0)
      {
        prices | Take(item) = price
        If(
          price | IsNotNone
          {
            price | ExpectInt = price-int
            price-int | Math.Multiply(parsed-quantity) = subtotal
            f"{item}: {subtotal}" | Log
            subtotal | Math.Add(total) > total
          }
          {
            f"rejected: {item}" | Log
            Math.Inc(rejection-count)
          }
        )
      }
      {
        f"rejected: {item}" | Log
        Math.Inc(rejection-count)
      }
    )
  }
  {
    f"rejected: {item}" | Log
    Math.Inc(rejection-count)
  })
  
  Math.Inc(i)
} Times: 6)

f"total={total} rejected={rejection-count}" | Log
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["apple: 6","Maybe: activation error: ParseInt: \"bad\" is not an Int in base 10","rejected: pear","rejected: missing","rejected: plum","pear: 15","rejected: apple","total=21 rejected=4"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"total=21 rejected=4\""}],"spawned_failures":[]}
stderr:
```
