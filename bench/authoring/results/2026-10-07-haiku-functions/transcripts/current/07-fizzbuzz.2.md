# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
0 >= i
Repeat({
  Inc(i) = n
  n |
  If(
    { Div(15) | Mul(15) | Is(n) }
    { "FizzBuzz" | Log }
    {
      If(
        { Div(3) | Mul(3) | Is(n) }
        { "Fizz" | Log }
        {
          If(
            { Div(5) | Mul(5) | Is(n) }
            { "Buzz" | Log }
            { Log }
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
