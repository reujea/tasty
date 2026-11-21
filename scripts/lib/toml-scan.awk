# TOML 문서의 표 머리와 키를 줄 단위가 아니라 문자 단위로 읽어 낸다.
# 출력(탭 구분):
#   H<TAB><머리>        표 머리. [[ plugin ]] 은 "[[plugin]]", [ release ] 는 "[release]"
#   K<TAB><키 형식><TAB><키><TAB><값 형식><TAB><값>
#       키 형식: bare | quoted. 이스케이프나 탭이 든 따옴표 키는 키 자리에 ? 를 낸다.
#       값 형식: basic("[A-Za-z0-9._-]+") | literal('[A-Za-z0-9._-]+') | other. other 의 값 자리는 빈칸이다(값에 탭이 있어도 레코드가 섞이지 않게).
# 주석, 기본·리터럴 문자열, 여러 줄 문자열, 배열·인라인 표의 괄호 깊이를 따라가므로
# 여러 줄 문자열 안의 "id = ..." 나 "[entry]" 줄, 중첩 배열 원소의 "[" 를 키·표 머리로 읽지 않는다.
# 닫히지 않은 문자열·괄호가 남으면 "E<TAB><이유>"를 내고 종료 코드 3으로 끝난다.
# 사용: awk -f scripts/lib/toml-scan.awk <파일>
# 검사기가 판정에 쓰는 최소한의 판독기다. 문법 전체를 검증하지 않는다.

BEGIN { ml = ""; depth = 0; bad = "" }

# s 의 i 번째부터 끝까지 문자열·괄호 상태를 따라간다. 주석을 만나면 그 줄의 나머지는 건너뛴다.
function scan(s, i,    c, n, j) {
    n = length(s)
    while (i <= n) {
        c = substr(s, i, 1)
        if (ml == "b") {
            if (c == "\\") { i += 2; continue }
            if (substr(s, i, 3) == "\"\"\"") { ml = ""; i += 3; while (substr(s, i, 1) == "\"") i++; continue }
            i++; continue
        }
        if (ml == "l") {
            if (substr(s, i, 3) == "'''") { ml = ""; i += 3; while (substr(s, i, 1) == "'") i++; continue }
            i++; continue
        }
        if (c == "#") return
        if (substr(s, i, 3) == "\"\"\"") { ml = "b"; i += 3; continue }
        if (substr(s, i, 3) == "'''") { ml = "l"; i += 3; continue }
        if (c == "\"") {
            for (j = i + 1; j <= n; j++) {
                if (substr(s, j, 1) == "\\") { j++; continue }
                if (substr(s, j, 1) == "\"") break
            }
            if (j > n) { bad = NR ": 기본 문자열이 줄 안에서 닫히지 않았다"; return }
            i = j + 1; continue
        }
        if (c == "'") {
            j = index(substr(s, i + 1), "'")
            if (j == 0) { bad = NR ": 리터럴 문자열이 줄 안에서 닫히지 않았다"; return }
            i = i + j + 1; continue
        }
        if (c == "[" || c == "{") depth++
        else if (c == "]" || c == "}") depth--
        i++
    }
}

function trim(s) { sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s); return s }

{
    sub(/\r$/, "")
    if (bad != "") next
    line = $0
    if (ml != "" || depth > 0) { scan(line, 1); next }
    t = line; sub(/^[ \t]+/, "", t)
    if (t == "" || substr(t, 1, 1) == "#") next
    if (substr(t, 1, 1) == "[") {
        if (substr(t, 1, 2) == "[[") {
            rest = substr(t, 3); k = index(rest, "]]")
            if (k == 0) { bad = NR ": 배열 표 머리를 읽지 못했다"; next }
            name = trim(substr(rest, 1, k - 1)); tail = trim(substr(rest, k + 2))
            head = "[[" name "]]"
        } else {
            rest = substr(t, 2); k = index(rest, "]")
            if (k == 0) { bad = NR ": 표 머리를 읽지 못했다"; next }
            name = trim(substr(rest, 1, k - 1)); tail = trim(substr(rest, k + 1))
            head = "[" name "]"
        }
        if (tail != "" && substr(tail, 1, 1) != "#") { bad = NR ": 표 머리 뒤에 읽을 수 없는 내용이 있다"; next }
        printf "H\t%s\n", head
        next
    }
    # 키
    c = substr(t, 1, 1)
    if (c == "\"" || c == "'") {
        n = length(t)
        for (j = 2; j <= n; j++) {
            if (c == "\"" && substr(t, j, 1) == "\\") { j++; continue }
            if (substr(t, j, 1) == c) break
        }
        if (j > n) { bad = NR ": 따옴표 키가 닫히지 않았다"; next }
        key = substr(t, 2, j - 2); kform = "quoted"
        if ((c == "\"" && index(key, "\\") > 0) || index(key, "\t") > 0) key = "?"
        rest = substr(t, j + 1)
    } else {
        k = index(t, "=")
        if (k == 0) { bad = NR ": key = value 형식이 아니다"; next }
        key = trim(substr(t, 1, k - 1)); kform = "bare"
        if (index(key, "\t") > 0) key = "?"
        rest = substr(t, k)
    }
    rest = trim(rest)
    if (substr(rest, 1, 1) != "=") { bad = NR ": 키 뒤에 = 가 없다"; next }
    v = trim(substr(rest, 2))
    val = ""; vform = "other"
    if (match(v, /^"[A-Za-z0-9._-]+"[ \t]*(#.*)?$/)) vform = "basic"
    else if (match(v, /^'[A-Za-z0-9._-]+'[ \t]*(#.*)?$/)) vform = "literal"
    if (vform != "other") { q = substr(v, 1, 1); val = substr(v, 2); val = substr(val, 1, index(val, q) - 1) }
    printf "K\t%s\t%s\t%s\t%s\n", kform, key, vform, val
    scan(v, 1)
}

END {
    if (bad == "" && ml != "") bad = "여러 줄 문자열이 닫히지 않았다"
    if (bad == "" && depth != 0) bad = "배열·인라인 표 괄호가 닫히지 않았다"
    if (bad != "") { printf "E\t%s\n", bad; exit 3 }
}
