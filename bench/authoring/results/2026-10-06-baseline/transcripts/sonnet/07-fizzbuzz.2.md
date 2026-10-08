# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
1 >= i
Repeat({
  If({i | Div(15) | Mul(15) | Is(i)}
     {"FizzBuzz" | Log}
     {If({i | Div(3) | Mul(3) | Is(i)}
         {"Fizz" | Log}
         {If({i | Div(5) | Mul(5) | Is(i)}
             {"Buzz" | Log}
             {i | Log})})})
  Inc(i)
} Times: 15)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","Fizz","4","Buzz","Fizz","7","8","Fizz","Buzz","11","Fizz","13","14","FizzBuzz"],"outcomes":[{"wire":"root","outcome":"completed","value":"1"}],"spawned_failures":[]}
stderr:
```
