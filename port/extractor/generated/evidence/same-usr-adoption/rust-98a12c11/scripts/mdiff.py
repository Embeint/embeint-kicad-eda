import json, sys
a = json.load(open(sys.argv[1])); b = json.load(open(sys.argv[2]))
def walk(x, y, p=''):
    if isinstance(x, dict) and isinstance(y, dict):
        for k in sorted(set(x) | set(y)):
            walk(x.get(k), y.get(k), f'{p}.{k}')
    elif x != y:
        print(p, '|', json.dumps(x)[:300], '->', json.dumps(y)[:300])
walk(a, b)
