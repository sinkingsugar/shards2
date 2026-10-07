# 41-inventory-orders

## Task

Prices are {apple: 3 pear: 5 plum: 2}. Process these orders in order: {item: "apple" quantity: "2"}, {item: "pear" quantity: "bad"}, {item: "missing" quantity: "4"}, {item: "plum" quantity: "-1"}, {item: "pear" quantity: "3"}, {item: "apple" quantity: "0"}. Accept an order only if its item exists and its quantity parses as a strictly positive integer. For each accepted order log "ITEM: SUBTOTAL" and add its subtotal to the total; for every other order log "rejected: ITEM" and increment a rejection count. Finally log "total=T rejected=R". Unknown items and malformed quantities must not terminate the program. Build the result from the input records, not literal output lines.

## Reply 0

```shards
{apple: 3 pear: 5 plum: 2} = prices
0 | Var(total)
0 | Var(rejected-count)
[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
]

Repeat(
  {
    .item = item
    .quantity = quantity-str
    
    prices | Take(item) = price-or-none
    
    If(
      { price-or-none | IsNone }
      {
        f"rejected: {item}" | Log
        Inc(rejected-count)
      }
      {
        Maybe(
          {
            quantity-str | ParseInt | Var(quantity)
            
            If(
              { quantity | IsMore(0) }
              {
                quantity | Math.Multiply(price-or-none) | Var(subtotal)
                f"{item}: {subtotal}" | Log
                subtotal | Math.Add(total) | Update(total)
              }
              {
                f"rejected: {item}" | Log
                Inc(rejected-count)
              }
            )
          }
          {
            f"rejected: {item}" | Log
            Inc(rejected-count)
          }
          silent: true
        )
      }
    )
  }
  times: 6
)

f"total={total} rejected={rejected-count}" | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"parse","severity":"error","kind":"syntax","code":"expected-value","message":"expected a shard or a value, found `.`","file":"solution.shs","line":15,"column":5},{"phase":"parse","severity":"error","kind":"syntax","code":"expected-value","message":"expected a shard or a value, found `.`","file":"solution.shs","line":16,"column":5}],"wires":[],"functions":[]}
```

## Reply 1

```shards
{apple: 3 pear: 5 plum: 2} = prices
0 | Var(total)
0 | Var(rejected-count)
0 | Var(idx)
[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

Repeat(
  {
    orders | Take(idx) = order
    order | Take("item") = item
    order | Take("quantity") = quantity-str
    Inc(idx)
    
    prices | Take(item) = price-or-none
    
    If(
      { price-or-none | IsNone }
      {
        f"rejected: {item}" | Log
        Inc(rejected-count)
      }
      {
        Maybe(
          {
            quantity-str | ParseInt | Var(quantity)
            
            If(
              { quantity | IsMore(0) }
              {
                quantity | Math.Multiply(price-or-none) | Var(subtotal)
                f"{item}: {subtotal}" | Log
                subtotal | Math.Add(total) | Update(total)
              }
              {
                f"rejected: {item}" | Log
                Inc(rejected-count)
              }
            )
          }
          {
            f"rejected: {item}" | Log
            Inc(rejected-count)
          }
          silent: true
        )
      }
    )
  }
  times: 6
)

f"total={total} rejected={rejected-count}" | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"wrong-variable-type","message":"operand must be Int or Float or Float2 or Float3 or Float4, but price-or-none is Int | None","file":"solution.shs","line":37,"column":28,"shard":"Math.Multiply","actual":{"name":"Int | None","basic_type":-1},"expected":[{"name":"Int","basic_type":4},{"name":"Float","basic_type":10},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"param_index":0,"param":"operand","path":[{"wire":"root"},{"shard":10,"name":"Repeat"},{"param":"action"},{"shard":13,"name":"If"},{"param":"else"},{"shard":0,"name":"Maybe"},{"param":"action"},{"shard":3,"name":"If"},{"param":"then"},{"shard":1,"name":"Math.Multiply"}]}],"wires":[],"functions":[]}
```

## Reply 2

```shards
{apple: 3 pear: 5 plum: 2} = prices
0 | Var(total)
0 | Var(rejected-count)
0 | Var(idx)
[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

Repeat(
  {
    orders | Take(idx) = order
    order | Take("item") = item
    order | Take("quantity") = quantity-str
    Inc(idx)
    
    prices | Take(item) = price-or-none
    
    If(
      { price-or-none | IsNone }
      {
        f"rejected: {item}" | Log
        Inc(rejected-count)
      }
      {
        price-or-none | ExpectInt = price
        Maybe(
          {
            quantity-str | ParseInt | Var(quantity)
            
            If(
              { quantity | IsMore(0) }
              {
                quantity | Math.Multiply(price) | Var(subtotal)
                f"{item}: {subtotal}" | Log
                subtotal | Math.Add(total) | Update(total)
              }
              {
                f"rejected: {item}" | Log
                Inc(rejected-count)
              }
            )
          }
          {
            f"rejected: {item}" | Log
            Inc(rejected-count)
          }
          silent: true
        )
      }
    )
  }
  times: 6
)

f"total={total} rejected={rejected-count}" | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["apple: 6","rejected: pear","rejected: missing","rejected: plum","pear: 15","rejected: apple","total=21 rejected=4"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"total=21 rejected=4\""}],"spawned_failures":[]}
stderr:
```
