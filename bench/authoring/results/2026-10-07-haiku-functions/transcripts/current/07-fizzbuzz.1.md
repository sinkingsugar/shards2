# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
0 >= i

Repeat({
  Inc(i)
  
  If(
    Predicate: {
      div-15 = i | Math.Divide(15) | Math.Multiply(15)
      i | Math.Subtract(div-15) | Is(0)
    }
    Then: { "FizzBuzz" | Log }
    Else: {
      If(
        Predicate: {
          div-3 = i | Math.Divide(3) | Math.Multiply(3)
          i | Math.Subtract(div-3) | Is(0)
        }
        Then: { "Fizz" | Log }
        Else: {
          If(
            Predicate: {
              div-5 = i | Math.Divide(5) | Math.Multiply(5)
              i | Math.Subtract(div-5) | Is(0)
            }
            Then: { "Buzz" | Log }
            Else: { i | Log }
          )
        }
      )
    }
  )
} Times: 15)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"unknown-variable","message":"unknown variable div-15; `-` is part of names, so `div-15` is one name: subtract with `Math.Subtract` (`Sub`)","file":"solution.shs","line":8,"column":7,"shard":"Get","param_index":0,"param":"Variable","path":[{"wire":"root"},{"shard":2,"name":"Repeat"},{"param":"Action"},{"shard":1,"name":"If"},{"param":"Predicate"},{"shard":0,"name":"Get"}]}]}
```

## Reply 1

```shards
0 >= i

Repeat({
  Inc(i)
  i | Math.Divide(15) | Math.Multiply(15) | Is(i) = div-by-15
  i | Math.Divide(3) | Math.Multiply(3) | Is(i) = div-by-3
  i | Math.Divide(5) | Math.Multiply(5) | Is(i) = div-by-5
  
  If(
    Predicate: { div-by-15 }
    Then: { "FizzBuzz" | Log }
    Else: {
      If(
        Predicate: { div-by-3 }
        Then: { "Fizz" | Log }
        Else: {
          If(
            Predicate: { div-by-5 }
            Then: { "Buzz" | Log }
            Else: { i | Log }
          )
        }
      )
    }
  )
} Times: 15)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","Fizz","4","Buzz","Fizz","7","8","Fizz","Buzz","11","Fizz","13","14","FizzBuzz"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
