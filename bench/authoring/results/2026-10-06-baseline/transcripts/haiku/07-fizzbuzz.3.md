# 07-fizzbuzz

## Task

FizzBuzz for 1 to 15: for each number log `FizzBuzz` if it is divisible by 15, `Fizz` if divisible by 3, `Buzz` if divisible by 5, and the number itself otherwise. One line per number.

## Reply 0

```shards
0 >= i
Repeat({
  Inc(i)
  i | Math.Divide(15) | Math.Multiply(15) = div15-times
  i | Math.Subtract(div15-times) = rem15
  If({rem15 Is 0} {"FizzBuzz" | Log} {
    i | Math.Divide(3) | Math.Multiply(3) = div3-times
    i | Math.Subtract(div3-times) = rem3
    If({rem3 Is 0} {"Fizz" | Log} {
      i | Math.Divide(5) | Math.Multiply(5) = div5-times
      i | Math.Subtract(div5-times) = rem5
      If({rem5 Is 0} {"Buzz" | Log} {i | Log})
    })
  })
} Times: 15)
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"construct","severity":"error","kind":"generic","code":"missing-argument","message":"Is: missing required parameter Operand","file":"solution.shs","line":6,"column":13,"shard":"Is","param_index":0,"param":"Operand","path":[{"wire":"root"},{"shard":2,"name":"Repeat"},{"param":"Action"},{"shard":8,"name":"If"},{"param":"Predicate"},{"shard":1,"name":"Is"}]}]}
```

## Reply 1

```shards
0 >= i
Repeat({
  Inc(i)
  i | Math.Divide(15) | Math.Multiply(15) = div15-times
  i | Math.Subtract(div15-times) = rem15
  If({rem15 | Is(0)} {"FizzBuzz" | Log} {
    i | Math.Divide(3) | Math.Multiply(3) = div3-times
    i | Math.Subtract(div3-times) = rem3
    If({rem3 | Is(0)} {"Fizz" | Log} {
      i | Math.Divide(5) | Math.Multiply(5) = div5-times
      i | Math.Subtract(div5-times) = rem5
      If({rem5 | Is(0)} {"Buzz" | Log} {i | Log})
    })
  })
} Times: 15)
```

## Run (pass)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["1","2","Fizz","4","Buzz","Fizz","7","8","Fizz","Buzz","11","Fizz","13","14","FizzBuzz"],"outcomes":[{"wire":"root","outcome":"completed","value":"0"}],"spawned_failures":[]}
stderr:
```
