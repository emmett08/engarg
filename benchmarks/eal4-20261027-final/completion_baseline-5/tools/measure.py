import json,sys
operation=sys.argv[1]
request=json.load(sys.stdin)
with open('requests.jsonl','a') as stream:
    stream.write(json.dumps({'operation':operation,'input':request['input']},sort_keys=True)+'\n')
if operation=='snapshot':
    assert request['input']=={'scenario':'positive'}
    value={'snapshot':'fixture-v1','items':['Alpha','Beta','Gamma'],'complete':True,'manifest_count':3}
else:
    assert operation=='audit'
    items=request['input']['items']
    assert isinstance(items,list) and all(isinstance(item,str) for item in items)
    value={'count':len(items),'distinct':len(items)==len(set(items))}
print(json.dumps({'value':value,'observed_at':'2026-10-25T12:00:00Z'}))
