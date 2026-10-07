# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
0 | Var(n)
Repeat({
  Inc(n) | If(
    predicate: { Div(15) | ToInt | Mul(15) | Is(n) }
    then: { "FizzBuzz" | Log }
    else: {
      If(
        predicate: { Div(3) | ToInt | Mul(3) | Is(n) }
        then: { "Fizz" | Log }
        else: {
          If(
            predicate: { Div(5) | ToInt | Mul(5) | Is(n) }
            then: { "Buzz" | Log }
            else: { n | Log }
          )
        }
      )
    }
  )
} times: 15)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","Fizz","4","Buzz","Fizz","7","8","Fizz","Buzz","11","Fizz","13","14","FizzBuzz"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
