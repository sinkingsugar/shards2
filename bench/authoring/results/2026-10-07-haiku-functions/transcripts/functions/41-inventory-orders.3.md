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
0 | Var(rejection-count)
0 | Var(i)

Repeat(
  {
    orders | Take(i) = order
    order.item = item
    order.quantity = quantity-str
    
    Maybe(
      {
        quantity-str | ParseInt = quantity-int
        
        If(
          quantity-int | IsMore(0)
          {
            item | IsAny(["apple" "pear" "plum"]) | If(
              { true }
              {
                item | Match([
                  "apple" { prices.apple }
                  "pear" { prices.pear }
                  "plum" { prices.plum }
                ] default: { 0 }) = price
                
                quantity-int | Math.Multiply(price) = subtotal
                f"{item}: {subtotal}" | Log
                subtotal | Math.Add(total) | Update(total)
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
      }
      silent: true
    )
    
    Inc(i)
  }
  times: 6
)

f"total={total} rejected={rejection-count}" | Log
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["apple: 6","rejected: pear","missing: 0","rejected: plum","pear: 15","rejected: apple","total=21 rejected=3"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"total=21 rejected=3\""}],"spawned_failures":[]}
stderr:
```
