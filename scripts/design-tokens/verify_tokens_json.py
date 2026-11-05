#!/usr/bin/env python3
"""Compare tokens/tasty.tokens.json with the token CSS in both directions.

usage: verify_tokens_json.py <project-dir> <json>

Names, values, aliases and Latte values must match. Exits 1 on any difference.
"""
import json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_tokens_json as g

after,json_path=sys.argv[1],sys.argv[2]
d=json.load(open(json_path,encoding='utf-8'))
T=g.collect(after);own=g.tier_of(T)
tot=0;errs=[]
for t in g.TIERS:
    base,lat=T[t]; j={k:v for k,v in d[t].items() if k!='$description'}
    tot+=len(j)
    a=set(base)-set(j); b=set(j)-set(base)
    if a or b: errs.append((t,'names',a,b))
    nv=al=la=0
    for k,v in j.items():
        if k not in base: continue
        e=g.alias(base[k],own)
        if str(v['$value'])!=e: errs.append((t,k,'value',v['$value'],e))
        nv+=1; al+=str(v['$value']).startswith('{')
        jl=v.get('$extensions',{}).get('com.tasty.mode',{}).get('latte')
        cl=g.alias(lat[k],own) if k in lat else None
        if jl!=cl: errs.append((t,k,'latte',jl,cl))
        la+=cl is not None
    print(f'{t}: css {len(base)} json {len(j)} css-only {len(a)} json-only {len(b)} | aliases {al} | latte css {len(lat)} json {sum("$extensions" in v for v in j.values())} matched {la}')
print('total',tot,'errors',errs)
sys.exit(1 if errs else 0)
