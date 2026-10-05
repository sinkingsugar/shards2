import sys,os,json,hashlib,subprocess,statistics,datetime
from pathlib import Path
sys.path.insert(0,str(Path('bench/vm-execution').resolve()))
import run as vm
os.sched_setaffinity(0,{2})
binaries={'old':'/tmp/shards-dispatch-before','explicit-tag':'/tmp/shards-explicit-tag','offsets-only':'/tmp/shards-niche-offsets','tag-and-offsets':'/tmp/shards-explicit-offsets','1x':'../shards/build/Release/shards'}
engines=[(variant,backend) for variant in binaries for backend in (['stackless','stackful'] if variant!='1x' else ['native'])]
records=[];scripts={}
for case in ['const-int','const-seq','get-int','get-seq','assign-int','add-int','add-float','add-float4','take-seq']:
 for width in [8,256]:
  iterations=500000 if width==8 else 50000
  source=vm.script(case,width,iterations,5);scripts[f'{case}-{width}']=source
  path=Path('/tmp/shards-layout-matrix.shs');path.write_text(source)
  for round_id in range(3):
   order=engines[round_id:]+engines[:round_id]
   if round_id%2:order=list(reversed(order))
   for variant,backend in order:
    command=[binaries[variant]]+([] if variant=='1x' else ['run'])+(['--stackful'] if backend=='stackful' else [])
    samples=vm.run(command,path,iterations,5,120)
    records.append(dict(case=case,width=width,iterations=iterations,round=round_id,variant=variant,backend=backend,seconds=samples,warmup=2))
  print(case,width,flush=True)
Path('/tmp/shards-layout-matrix.json').write_text(json.dumps({'method':'CPU 2, sequential rotating/reversing process order; 3 rounds, 2 warmups + 3 retained batches; identical scripts/iterations in each cell; all outputs/counts validated. Frequency unlocked.','started_from':'4b524723e659f3e45075c1c363bc94bb4cd793ca','binaries':{k:{'path':v,'sha256':hashlib.sha256(Path(v).read_bytes()).hexdigest()} for k,v in binaries.items()},'scripts':scripts,'records':records},indent=2)+'\n')
