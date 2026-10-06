# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
0 >= i
Repeat({
  Inc(i) = num
  If(
    Predicate: { num | Div(15) | Mul(15) | Is(num) }
    Then: { "FizzBuzz" | Log }
    Else: {
      If(
        Predicate: { num | Div(3) | Mul(3) | Is(num) }
        Then: { "Fizz" | Log }
        Else: {
          If(
            Predicate: { num | Div(5) | Mul(5) | Is(num) }
            Then: { "Buzz" | Log }
            Else: { num | Log }
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
