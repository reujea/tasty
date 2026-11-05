#!/usr/bin/env python3
"""Regenerate tokens/tasty.tokens.json from the token CSS.

usage: gen_tokens_json.py <project-dir> <prev-json> <out-json> [--pure]

Names, values, aliases and Latte values come only from the CSS:
  tokens.css + tokens/primitives.css -> primitive
  tokens/semantic.css                -> semantic
  tokens/components.css              -> component
The CSS does not hold everything the JSON holds, so <prev-json> supplies, for tokens it
already has: key order, $description, $type and the int-vs-string spelling of a number.
A new token is placed right after its CSS predecessor and gets an inferred $type.
--pure ignores <prev-json> for those fields (used to measure what the CSS alone can give).
A token in <prev-json> that is not in the CSS is an error: tokens are never removed here.
"""
import json, re, sys

TIERS = ("primitive", "semantic", "component")
DECL = re.compile(
    r"/\*.*?\*/|([^{}/;]*?)\{|\}|(--[\w-]+)\s*:\s*((?:[^;/]|/(?!\*))*?);", re.S)

def parse(path):
    """-> [(selector, name, value)] with whitespace in values collapsed."""
    scope, out = None, []
    for m in DECL.finditer(open(path, encoding="utf-8").read()):
        if m.group(2):
            out.append((scope, m.group(2), re.sub(r"\s+", " ", m.group(3)).strip()))
        elif m.group(1) is not None:
            scope = m.group(1).strip()
        elif m.group(0) == "}":
            scope = None
    return out

def is_latte(scope):
    return scope is not None and 'data-theme="latte"' in scope

def collect(project):
    files = {
        "primitive": [f"{project}/tokens.css", f"{project}/tokens/primitives.css"],
        "semantic": [f"{project}/tokens/semantic.css"],
        "component": [f"{project}/tokens/components.css"],
    }
    tiers = {}
    for tier, paths in files.items():
        base, latte = {}, {}
        for p in paths:
            for scope, name, value in parse(p):
                if not name.startswith("--tasty-"):
                    continue  # tokens.css also defines the type-class helpers --font-sans/--font-mono
                key = name[len("--tasty-"):]
                if is_latte(scope):
                    if key in latte:
                        sys.exit(f"duplicate Latte value: {name}")
                    latte[key] = value
                else:
                    if key in base:
                        sys.exit(f"duplicate value: {name}")
                    base[key] = value
        missing = set(latte) - set(base)
        if missing:
            sys.exit(f"Latte value without a baseline: {sorted(missing)}")
        tiers[tier] = (base, latte)
    return tiers

def tier_of(tiers):
    owner = {}
    for tier in TIERS:
        for key in tiers[tier][0]:
            if key in owner:
                sys.exit(f"token in two tiers: {key}")
            owner[key] = tier
    return owner

def alias(value, owner):
    m = re.fullmatch(r"var\(--tasty-([\w-]+)\)", value)
    if not m:
        return value  # literals, color-mix(), calc() and lists stay verbatim
    if m.group(1) not in owner:
        sys.exit(f"alias of a missing token: {value}")
    return "{%s.%s}" % (owner[m.group(1)], m.group(1))

def infer_type(key, tier, value, tiers, owner):
    m = re.fullmatch(r"var\(--tasty-([\w-]+)\)", value)
    if m:
        t = m.group(1)
        return infer_type(t, owner[t], tiers[owner[t]][0][t], tiers, owner)
    v = value
    if "font-family" in key: return "fontFamily"
    if key.startswith("font-weight"): return "fontWeight"
    if key.startswith("shadow") or "shadow" in key: return "shadow"
    if re.match(r"(#|rgba?\(|hsla?\(|color-mix\()", v): return "color"
    if v.startswith("cubic-bezier("): return "cubicBezier"
    if re.fullmatch(r"-?[\d.]+ms", v): return "duration"
    if re.fullmatch(r"\d+( \d+)+", v): return "strokeStyle"
    if re.search(r"(px|em)\b", v) or v.startswith("calc("): return "dimension"
    if key.startswith(("size-", "radius-", "letter-spacing-")): return "dimension"
    return "number"

def build(project, prev, pure):
    tiers = collect(project)
    owner = tier_of(tiers)
    out = {k: prev[k] for k in prev if k not in TIERS}
    for tier in TIERS:
        base, latte = tiers[tier]
        old = {k: v for k, v in prev[tier].items() if k != "$description"}
        gone = set(old) - set(base)
        if gone and not pure:
            sys.exit(f"{tier}: in the old JSON, not in the CSS: {sorted(gone)}")
        css_order = list(base)
        if pure:
            order = css_order
        else:
            order = [k for k in old]
            for i, k in enumerate(css_order):
                if k in old:
                    continue
                at = order.index(css_order[i - 1]) + 1 if i else 0
                order.insert(at, k)
        group = {}
        if "$description" in prev[tier]:
            group["$description"] = prev[tier]["$description"]
        for k in order:
            value = alias(base[k], owner)
            o = old.get(k) if not pure else None
            typ = o["$type"] if o else infer_type(k, tier, base[k], tiers, owner)
            if o and str(o["$value"]) == value:
                value = o["$value"]  # keeps the old int-vs-string spelling
            entry = {"$type": typ, "$value": value}
            if o and "$description" in o and list(o).index("$description") < list(o).index("$value"):
                sys.exit(f"unexpected key order: {k}")
            ext = {"$extensions": {"com.tasty.mode": {"latte": alias(latte[k], owner)}}} if k in latte else {}
            desc = {"$description": o["$description"]} if o and "$description" in o else {}
            # key order as the old file has it, when it has the token
            keys = list(o) if o else ["$type", "$value", "$extensions", "$description"]
            merged = {**entry, **ext, **desc}
            group[k] = {x: merged[x] for x in keys if x in merged}
            for x in merged:
                group[k].setdefault(x, merged[x])
        out[tier] = group
    return out

if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--pure"]
    pure = "--pure" in sys.argv
    project, prev_path, out_path = args
    prev = json.load(open(prev_path, encoding="utf-8"))
    res = build(project, prev, pure)
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(res, f, ensure_ascii=False, indent=2)
        f.write("\n")
