# 39-recursive-tree-fold

## Task

Represent a tree by parallel sequences of node values [5 -2 7 4 3 -1] and child-index lists [[1 2] [3 4] [5] [] [] []]. Define a recursive named function that takes a node index and these sequences as parameters and returns the sum of that node and all its descendants. Do not hardcode subtree totals or replace recursion with an explicit traversal stack. Call the same function for nodes 0, 1, 2 and 5 and log each result. Each invocation must keep its own partial sum while it visits its children.

## Reply 0

```shards
@fn(TreeSum input: None output: Int params: {node-index: Int values: [Int] children: [[Int]]} {
  values | Take(node-index) = node-value
  node-value | Var(sum)
  children | Take(node-index) = child-indices
  child-indices | Count = num-children
  
  0 | Var(child-i)
  Repeat({
    child-indices | Take(child-i) = child-idx
    TreeSum(node-index: child-idx values: values children: children) | Math.Add(sum) | Update(sum)
    Math.Inc(child-i)
  } times: num-children)
  
  sum
})

TreeSum(node-index: 0 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 0")
TreeSum(node-index: 1 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 1")
TreeSum(node-index: 2 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 2")
TreeSum(node-index: 5 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 5")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"compose-error","code":"wrong-argument-type","message":"TreeSum: children must be [[Int]], got [([Any] | [Int])]","file":"solution.shs","line":17,"column":1,"shard":"TreeSum","actual":{"name":"[([Any] | [Int])]","basic_type":56},"expected":[{"name":"[[Int]]","basic_type":56}],"param_index":2,"param":"children","path":[{"wire":"root"},{"shard":0,"name":"TreeSum"}]}],"wires":[],"functions":[{"signature":{"name":"TreeSum","revision":1,"input":{"kind":"type","type":"None"},"output":{"kind":"type","type":"Int"},"params":[{"name":"node-index","type":"Int","forms":["literal","variable"],"requirement":"required","help":""},{"name":"values","type":"[Int]","forms":["literal","variable"],"requirement":"required","help":""},{"name":"children","type":"[[Int]]","forms":["literal","variable"],"requirement":"required","help":""}],"lifetime":"stateless","effects":{"suspends":false,"io":false,"time":false,"random":false,"unknown":false},"uses":[],"mutates":[],"source":null,"summary":"","help":""},"line":1,"column":1}]}
```

## Reply 1

```shards
@fn(TreeSum input: None output: Int params: {node-index: Int values: Any children: Any} {
  values | Take(node-index) | ExpectInt = node-value
  node-value | Var(sum)
  children | Take(node-index) | ExpectSeq = child-indices
  child-indices | Count = num-children
  
  0 | Var(child-i)
  Repeat({
    child-indices | Take(child-i) | ExpectInt = child-idx
    TreeSum(node-index: child-idx values: values children: children) | Math.Add(sum) | Update(sum)
    Math.Inc(child-i)
  } times: num-children)
  
  sum
})

TreeSum(node-index: 0 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 0")
TreeSum(node-index: 1 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 1")
TreeSum(node-index: 2 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 2")
TreeSum(node-index: 5 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 5")
```

## Check

```json
{"ok":false,"file":"solution.shs","diagnostics":[{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Take needs Seq, Table, Float2, Float3 or Float4 input, got Any (the input comes from 0:Get)","file":"solution.shs","line":2,"column":12,"shard":"Take","actual":{"name":"Any","basic_type":1},"expected":[{"name":"Seq","basic_type":56},{"name":"Table","basic_type":57},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"input_from":{"origin":{"shard":0,"name":"Get"},"via":[]},"path":[{"wire":"root"},{"shard":0,"name":"TreeSum"},{"function":"TreeSum"},{"shard":1,"name":"Take"}]},{"phase":"compose","severity":"error","kind":"input-type-mismatch","code":"input-type-mismatch","message":"Take needs Seq, Table, Float2, Float3 or Float4 input, got Any (the input comes from 0:Get)","file":"solution.shs","line":2,"column":12,"shard":"Take","actual":{"name":"Any","basic_type":1},"expected":[{"name":"Seq","basic_type":56},{"name":"Table","basic_type":57},{"name":"Float2","basic_type":11},{"name":"Float3","basic_type":12},{"name":"Float4","basic_type":13}],"input_from":{"origin":{"shard":0,"name":"Get"},"via":[]},"path":[{"function":"TreeSum"},{"shard":1,"name":"Take"}]}],"wires":[],"functions":[]}
```

## Reply 2

```shards
@fn(TreeSum input: None output: Int params: {node-index: Int values: Any children: Any} {
  values | ExpectSeq | Take(node-index) | ExpectInt = node-value
  node-value | Var(sum)
  children | ExpectSeq | Take(node-index) | ExpectSeq = child-indices
  child-indices | Count = num-children
  
  0 | Var(child-i)
  Repeat({
    child-indices | Take(child-i) | ExpectInt = child-idx
    TreeSum(node-index: child-idx values: values children: children) | Math.Add(sum) | Update(sum)
    Math.Inc(child-i)
  } times: num-children)
  
  sum
})

TreeSum(node-index: 0 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 0")
TreeSum(node-index: 1 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 1")
TreeSum(node-index: 2 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 2")
TreeSum(node-index: 5 values: [5 -2 7 4 3 -1] children: [[1 2] [3 4] [5] [] [] []]) | Log("Node 5")
```

## Run (wrong-output)

```
exit Some(0)
{"ok":true,"file":"solution.shs","diagnostics":[],"log":["Node 0: 16","Node 1: 5","Node 2: 6","Node 5: -1"],"outcomes":[{"wire":"root","outcome":"completed","value":"-1"}],"spawned_failures":[]}
stderr:
```
