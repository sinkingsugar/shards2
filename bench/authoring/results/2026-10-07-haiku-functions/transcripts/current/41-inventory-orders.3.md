# 41-inventory-orders

## Task

Prices are {apple: 3 pear: 5 plum: 2}. Process these orders in order: {item: "apple" quantity: "2"}, {item: "pear" quantity: "bad"}, {item: "missing" quantity: "4"}, {item: "plum" quantity: "-1"}, {item: "pear" quantity: "3"}, {item: "apple" quantity: "0"}. Accept an order only if its item exists and its quantity parses as a strictly positive integer. For each accepted order log "ITEM: SUBTOTAL" and add its subtotal to the total; for every other order log "rejected: ITEM" and increment a rejection count. Finally log "total=T rejected=R". Unknown items and malformed quantities must not terminate the program. Build the result from the input records, not literal output lines.

## Reply 0

```shards
value = prices {apple: 3 pear: 5 plum: 2}

[
  {item: "apple" quantity: "2"}
  {item: "pear" quantity: "bad"}
  {item: "missing" quantity: "4"}
  {item: "plum" quantity: "-1"}
  {item: "pear" quantity: "3"}
  {item: "apple" quantity: "0"}
] = orders

value >= total 0
value >= rejection-count 0
value >= i 0

Repeat({
  orders | Take(i) = current-order
  current-order.item = item
  current-order.quantity = quantity-str
  
  Maybe(
    Action: { prices | Take(item) }
    Else: { none }
    Silent: true
  ) = price-opt
  
  If(
    Predicate: { price-opt | IsNotNone }
    Then: {
      price-opt | ExpectInt = price
      
      Maybe(
        Action: { quantity-str | ParseInt }
        Else: { none }
        Silent: true
      ) = quantity-opt
      
      If(
        Predicate: { quantity-opt | IsNotNone }
        Then: {
          quantity-opt | ExpectInt = quantity
          
          If(
            Predicate: { quantity | IsMore(0) }
            Then: {
              quantity | Mul(price) = subtotal
              f"{item}: {subtotal}" | Log
              subtotal | Add(total) > total
            }
            Else: {
              f"rejected: {item}" | Log
              Inc(rejection-count)
            }
          )
        }
        Else: {
          f"rejected: {item}" | Log
          Inc(rejection-count)
        }
      )
    }
    Else: {
      f"rejected: {item}" | Log
      Inc(rejection-count)
    }
  )
  
  Inc(i)
} Times: 6)

f"total={total} rejected={rejection-count}" | Log
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"unknown-variable","message":"unknown variable value","file":"solution.shs","line":1,"column":1,"shard":"Get","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":0,"name":"Get"}]}]}
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
0 >= rejection-count
0 >= i

Repeat({
  orders | Take(i) = current-order
  current-order.item = item
  current-order.quantity = quantity-str
  
  Maybe(
    Action: { prices | Take(item) }
    Else: { none }
    Silent: true
  ) = price-opt
  
  If(
    Predicate: { price-opt | IsNotNone }
    Then: {
      price-opt | ExpectInt = price
      
      Maybe(
        Action: { quantity-str | ParseInt }
        Else: { none }
        Silent: true
      ) = quantity-opt
      
      If(
        Predicate: { quantity-opt | IsNotNone }
        Then: {
          quantity-opt | ExpectInt = quantity
          
          If(
            Predicate: { quantity | IsMore(0) }
            Then: {
              quantity | Mul(price) = subtotal
              f"{item}: {subtotal}" | Log
              subtotal | Add(total) > total
            }
            Else: {
              f"rejected: {item}" | Log
              Inc(rejection-count)
            }
          )
        }
        Else: {
          f"rejected: {item}" | Log
          Inc(rejection-count)
        }
      )
    }
    Else: {
      f"rejected: {item}" | Log
      Inc(rejection-count)
    }
  )
  
  Inc(i)
} Times: 6)

f"total={total} rejected={rejection-count}" | Log
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["apple: 6","rejected: pear","rejected: missing","rejected: plum","pear: 15","rejected: apple","total=21 rejected=4"],"outcomes":[{"wire":"root","outcome":"completed","value":"\"total=21 rejected=4\""}],"spawned_failures":[]}
stderr:
```
