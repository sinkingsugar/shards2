# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
0 >= i
Repeat({
  Inc(i)
  i | If(
    Predicate: {Math.Divide(15) | Math.Multiply(15) | Is(i)}
    Then: {Log("FizzBuzz")}
    Else: {If(
      Predicate: {Math.Divide(3) | Math.Multiply(3) | Is(i)}
      Then: {Log("Fizz")}
      Else: {If(
        Predicate: {Math.Divide(5) | Math.Multiply(5) | Is(i)}
        Then: {Log("Buzz")}
        Else: {Log}
      )}
    )}
  )
} Times: 15)
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","Fizz: 3","4","Buzz: 5","Fizz: 6","7","8","Fizz: 9","Buzz: 10","11","Fizz: 12","13","14","FizzBuzz: 15"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
