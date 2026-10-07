# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
Keep(counter 0)
Repeat({
  Inc(counter) | Var(current)
  current | If(
    predicate: { current | Math.Divide(15) | Math.Multiply(15) | Is(current) }
    then: { "FizzBuzz" | Log }
    else: { current | If(
      predicate: { current | Math.Divide(3) | Math.Multiply(3) | Is(current) }
      then: { "Fizz" | Log }
      else: { current | If(
        predicate: { current | Math.Divide(5) | Math.Multiply(5) | Is(current) }
        then: { "Buzz" | Log }
        else: { current | Log }
      )}
    )}
  )
} times: 15)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","Fizz","4","Buzz","Fizz","7","8","Fizz","Buzz","11","Fizz","13","14","FizzBuzz"],"outcomes":[{"wire":"root","outcome":"completed","value":"none"}],"spawned_failures":[]}
stderr:
```
