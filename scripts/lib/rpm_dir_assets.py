"""rpm 폴더 항목을 덧붙인 cargo-generate-rpm assets 표를 만든다.

cargo-generate-rpm은 glob 결과의 폴더를 건너뛰고 rpm 크레이트는 부모 폴더를 파일 항목으로 만들지 않는다.
그래서 패키지가 자기 폴더를 소유하지 않아 제거 뒤 빈 폴더가 남는다. 이 스크립트는 Cargo.toml의
[package.metadata.generate-rpm].assets를 그대로 옮기고, 소유 뿌리 아래에 놓일 파일의 조상 폴더(뿌리 포함)마다
폴더 항목(mode "40755")을 덧붙인 TOML을 쓴다. --metadata-overwrite는 assets 배열을 통째로 바꾸므로
기존 항목도 모두 옮긴다. 결과 rpm의 폴더 집합은 scripts/lib/rpm-dirs.sh의 verify_rpm_dir_ownership이 따로 대조한다.

사용: python3 -I rpm_dir_assets.py <Cargo.toml> <폴더 항목 source 파일> <출력 TOML> <소유 뿌리>...
상대 source는 cwd 기준이다(build-linux.sh는 저장소 루트에서 부른다). Python 3.11 이상(tomllib)이 필요하다.
"""

import os
import sys
import tomllib

DIR_MODE = "40755"


def fail(msg):
    print(f"Error: rpm dir assets: {msg}", file=sys.stderr)
    sys.exit(1)


def toml_string(value):
    out = ['"']
    for ch in value:
        code = ord(ch)
        if ch == '"' or ch == "\\":
            out.append("\\" + ch)
        elif code < 0x20 or code == 0x7F:
            out.append(f"\\u{code:04X}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def toml_value(key, value, idx):
    # bool은 int의 하위형이므로 먼저 본다.
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return toml_string(value)
    fail(f"asset {idx} key {key!r} has a {type(value).__name__} value; only strings and booleans are supported")


def asset_line(asset, idx):
    if not isinstance(asset, dict):
        fail(f"asset {idx} is not a table")
    parts = []
    for key, value in asset.items():
        if not key.replace("_", "").replace("-", "").isalnum() or not key.isascii():
            fail(f"asset {idx} has an unsupported key {key!r}")
        parts.append(f"{key} = {toml_value(key, value, idx)}")
    return "    { " + ", ".join(parts) + " },"


def walk_files(base, recursive):
    """base 아래 일반 파일의 상대 경로. 점으로 시작하는 이름도 넣는다(cargo-generate-rpm의 glob과 같다)."""
    if not os.path.isdir(base):
        return []
    found = []
    if recursive:
        for root, dirs, files in os.walk(base):
            dirs.sort()
            for name in files:
                path = os.path.join(root, name)
                if os.path.isfile(path):
                    found.append(os.path.relpath(path, base))
    else:
        for name in os.listdir(base):
            if os.path.isfile(os.path.join(base, name)):
                found.append(name)
    return sorted(found)


def asset_dests(asset, idx):
    """한 asset이 rpm에 놓을 경로들. 지원하는 glob은 "<폴더>/*"와 "<폴더>/**/*" 두 형태다."""
    source = asset.get("source")
    dest = asset.get("dest")
    if not isinstance(source, str) or not isinstance(dest, str):
        fail(f"asset {idx} needs string source and dest")
    if "*" not in source:
        if dest.endswith("/"):
            return [dest + os.path.basename(source)]
        return [dest]
    if not dest.endswith("/"):
        fail(f"asset {idx} has a glob source but dest {dest!r} does not end with /")
    if source.endswith("/**/*") and "*" not in source[: -len("/**/*")]:
        rels = walk_files(source[: -len("/**/*")], True)
    elif source.endswith("/*") and "*" not in source[: -len("/*")]:
        rels = walk_files(source[: -len("/*")], False)
    else:
        fail(f"asset {idx} uses an unsupported glob {source!r}")
    return [dest + rel for rel in rels]


def owned_dirs(dests, roots):
    dirs = set()
    for dest in dests:
        for root in roots:
            if dest.startswith(root + "/"):
                parent = os.path.dirname(dest)
                while True:
                    dirs.add(parent)
                    if parent == root:
                        break
                    parent = os.path.dirname(parent)
    return sorted(dirs)


def main(argv):
    if len(argv) < 5:
        fail("usage: rpm_dir_assets.py <Cargo.toml> <dir source> <out toml> <owned root>...")
    manifest, dir_source, out, roots = argv[1], argv[2], argv[3], argv[4:]
    for root in roots:
        if not root.startswith("/") or root.endswith("/") or root == "/":
            fail(f"owned root {root!r} must be absolute without a trailing slash")
    try:
        with open(manifest, "rb") as f:
            meta = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        fail(f"cannot read {manifest}: {e}")
    assets = meta.get("package", {}).get("metadata", {}).get("generate-rpm", {}).get("assets")
    if not isinstance(assets, list) or not assets:
        fail(f"{manifest} has no [package.metadata.generate-rpm] assets")
    lines = ["assets = ["]
    dests = []
    for idx, asset in enumerate(assets):
        lines.append(asset_line(asset, idx))
        dests.extend(asset_dests(asset, idx))
    for directory in owned_dirs(dests, roots):
        # dest 끝에 /를 붙이면 cargo-generate-rpm이 source 파일 이름을 덧붙인다.
        lines.append(
            "    { "
            + f"source = {toml_string(dir_source)}, dest = {toml_string(directory)}, mode = {toml_string(DIR_MODE)}"
            + " },"
        )
    lines.append("]")
    try:
        with open(out, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    except OSError as e:
        fail(f"cannot write {out}: {e}")


if __name__ == "__main__":
    main(sys.argv)
