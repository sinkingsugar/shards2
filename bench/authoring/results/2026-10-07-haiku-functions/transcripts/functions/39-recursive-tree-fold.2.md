# 39-recursive-tree-fold

## Task

Represent a tree by parallel sequences of node values [5 -2 7 4 3 -1] and child-index lists [[1 2] [3 4] [5] [] [] []]. Define a recursive named function that takes a node index and these sequences as parameters and returns the sum of that node and all its descendants. Do not hardcode subtree totals or replace recursion with an explicit traversal stack. Call the same function for nodes 0, 1, 2 and 5 and log each result. Each invocation must keep its own partial sum while it visits its children.

## Reply 0

```shards
@fn(TreeSum input: Int output: Int params: {values: [Int] children: [[Int]]} {
  input | Take(values) | Var(sum)
  input | Take(children) = child-indices
  0 | Var(i)
  
  Repeat({
    child-indices | Take(i) | TreeSum(values: values children: children) |
    Math.Add(sum) |
    Update(sum) |
    Inc(i)
  } times: (child-indices | Count))
  
  sum
})

[5 -2 7 4 3 -1] = values
[[1 2] [3 4] [5] [] [] []] = children

0 | TreeSum(values: values children: children) | Log("0")
1 | TreeSum(values: values children: children) | Log("1")
2 | TreeSum(values: values children: children) | Log("2")
5 | TreeSum(values: values children: children) | Log("5")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"wrong-variable-type","message":"TreeSum: children must be [[Int]], got [([Any] | [Int])]","file":"solution.shs","line":19,"column":5,"shard":"TreeSum","actual":{"name":"[([Any] | [Int])]","basic_type":56},"expected":[{"name":"[[Int]]","basic_type":56}],"param_index":1,"param":"children","path":[{"wire":"root"},{"shard":5,"name":"TreeSum"}]},{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Take needs Seq, Table, Float2, Float3 or Float4 input, got Int (the input comes from 0:Get)","file":"solution.shs","line":2,"column":11,"shard":"Take","actual":{"name":"Int","basic_type":4},"expected":[{"name":"Seq","basic_type":56},{"name":"Table","basic_type":57},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"input_from":{"origin":{"shard":0,"name":"Get"},"via":[]},"path":[{"function":"TreeSum"},{"shard":1,"name":"Take"}]}],"wires":[],"functions":[]}
```

## Reply 1

```shards
@fn(TreeSum input: Int output: Int params: {values: [Int] children: [Any]} {
  values | Take(input) | Var(sum)
  children | Take(input) | ExpectSeq | Var(child-indices)
  0 | Var(i)
  
  Repeat({
    child-indices | Take(i) | ExpectInt | TreeSum(values: values children: children) |
    Math.Add(sum) |
    Update(sum) |
    Inc(i)
  } times: (child-indices | Count))
  
  sum
})

[5 -2 7 4 3 -1] = values
[[1 2] [3 4] [5] [] [] []] = children

0 | TreeSum(values: values children: children) | Log("0")
1 | TreeSum(values: values children: children) | Log("1")
2 | TreeSum(values: values children: children) | Log("2")
5 | TreeSum(values: values children: children) | Log("5")
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["0: 16","1: 5","2: 6","5: -1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
