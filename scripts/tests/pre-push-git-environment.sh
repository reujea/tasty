#!/usr/bin/env bash
# Exercise the real hook against disposable Git repositories; Cargo is a fixture command.
set -eu -o pipefail
hook="$1"
fixture="$(cd "$2" && pwd -P)"
local_vars="$(git rev-parse --local-env-vars)"
while IFS= read -r name; do
    [ -z "$name" ] || unset "$name"
done <<< "$local_vars"
repo="$fixture/pushing repo"
mkdir -p "$repo/scripts" "$fixture/bin"
git -C "$repo" init -q
printf 'original\n' > "$repo/original"
git -C "$repo" add original
git -C "$repo" -c user.name=fixture -c user.email=fixture@example.invalid -c commit.gpgsign=false commit -qm original
before_head="$(git -C "$repo" rev-parse HEAD)"
before_index="$(git -C "$repo" ls-files --stage)"
cp "$repo/.git/config" "$fixture/config.before"
cp "$repo/.git/index" "$fixture/index.before"
cat > "$fixture/check" <<'CHECK'
#!/usr/bin/env bash
set -eu -o pipefail
[ "$(git rev-parse --show-toplevel)" = "$EXPECTED_PARENT" ]
while IFS= read -r name; do
    if printenv "$name" >/dev/null; then
        echo "leaked repository variable: $name" >&2
        exit 81
    fi
done < <(git rev-parse --local-env-vars)
child="$(mktemp -d "$EXPECTED_FIXTURE/child.XXXXXX")"
git -C "$child" init -q
printf 'synthetic\n' > "$child/synthetic"
git -C "$child" add synthetic
git -C "$child" -c user.name=fixture -c user.email=fixture@example.invalid -c commit.gpgsign=false commit -qm synthetic
git -C "$child" config core.bare true
[ "$(git -C "$child" rev-parse --is-bare-repository)" = true ]
printf 'checked\n' >> "$EXPECTED_FIXTURE/checks"
# B.10 finds a passed test of the module in this output.
printf 'test source_guards::fixture ... ok\n'
CHECK
chmod +x "$fixture/check"
cp "$fixture/check" "$fixture/bin/cargo"
cp "$fixture/check" "$repo/scripts/check-plugin-version-bump.sh"
EXPECTED_PARENT="$(git -C "$repo" rev-parse --show-toplevel)"
EXPECTED_FIXTURE="$fixture"
export EXPECTED_PARENT EXPECTED_FIXTURE
export PATH="$fixture/bin:$PATH"
# Only this disposable repository is ever exported as the hook's Git context.
export GIT_DIR="$repo/.git" GIT_COMMON_DIR="$repo/.git" GIT_WORK_TREE="$repo"
export GIT_INDEX_FILE="$repo/.git/index" GIT_OBJECT_DIRECTORY="$repo/.git/objects" GIT_PREFIX=''
printf 'refs/heads/main %s refs/heads/main %s\n' "$before_head" "$before_head" | bash "$hook" origin unused
[ "$GIT_DIR" = "$repo/.git" ]
[ "$GIT_COMMON_DIR" = "$repo/.git" ]
[ "$(git rev-parse HEAD)" = "$before_head" ]
[ "$(git ls-files --stage)" = "$before_index" ]
cmp "$fixture/config.before" "$repo/.git/config"
cmp "$fixture/index.before" "$repo/.git/index"
[ "$(git config --get core.bare)" = false ]
[ "$(wc -l < "$fixture/checks" | tr -d ' ')" = 6 ]
