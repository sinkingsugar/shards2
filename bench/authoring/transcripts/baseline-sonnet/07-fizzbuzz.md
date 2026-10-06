# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
1 >= i
Repeat({
  i | Math.Divide(15) | Math.Multiply(15) | Is(i) = d15
  i | Math.Divide(3) | Math.Multiply(3) | Is(i) = d3
  i | Math.Divide(5) | Math.Multiply(5) | Is(i) = d5
  If({d15} {"FizzBuzz" | Log} {
    If({d3} {"Fizz" | Log} {
      If({d5} {"Buzz" | Log} {i | Log})
    })
  })
  Inc(i)
} Times: 15)
```

## Run (pass)

```
exit Some(0)
stdout:
1
2
Fizz
4
Buzz
Fizz
7
8
Fizz
Buzz
11
Fizz
13
14
FizzBuzz
root: 1
stderr:
```
