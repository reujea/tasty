# TOML 문서의 표 머리와 키를 줄 단위가 아니라 문자 단위로 읽어 낸다.
# 출력(탭 구분):
#   H<TAB><머리><TAB><이름 형식><TAB><첫 조각>   표 머리. [[ plugin ]] 은 "[[plugin]]", [ release ] 는 "[release]"
#       이름은 키와 같은 조각(맨 키, 따옴표 키)을 점으로 이은 것이다. ["entry"] 는 "[entry]" 로 낸다.
#       이름 형식: bare(맨 키 조각만, 점 앞뒤 공백 없음) | spaced(맨 키 조각만, 점 앞뒤에 공백) |
#                  quoted(따옴표 조각이 있고 모두 맨 키 글자) |
#                  unknown(맨 키 글자 밖의 글자·이스케이프·탭이 든 따옴표 조각. 머리 자리 이름은 빈칸).
#       공백·탭 밖의 공백 문자처럼 조각으로 읽지 못하는 머리는 오류로 끝난다.
#   K<TAB><키 형식><TAB><키><TAB><값 형식><TAB><값><TAB><첫 조각>
#       키 형식: bare | quoted | dotted(점으로 이은 조각 둘 이상, 키 자리는 조각을 점으로 이은 값)
#                | unknown(이스케이프나 탭이 든 따옴표 조각이 있다. 키 자리는 빈칸).
#       값 형식: basic("[A-Za-z0-9._-]+") | literal('[A-Za-z0-9._-]+') | bool(true·false) | other.
#                other 의 값 자리는 빈칸이다(값에 탭이 있어도 레코드가 섞이지 않게).
#   첫 조각: 머리·키의 첫 조각을 따옴표를 벗긴 값으로 낸다. 점이 든 따옴표 조각("a.b")도 한 조각이다.
#       이스케이프나 탭이 든 첫 조각은 디코딩하지 않고 빈칸으로 낸다(읽는 쪽은 빈칸을 확신할 수 없는 값으로 본다).
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

# t 의 pos 부터 점으로 이은 키 조각(맨 키, 기본·리터럴 따옴표 키)을 읽는다. 조각 사이 공백을 허용한다.
# 결과: SKEY(조각을 점으로 이은 값), SPOS(조각 뒤 공백을 건넌 위치), SN(조각 수), SQ(따옴표 조각 있음),
# SUNK(이스케이프나 탭이 든 따옴표 조각 있음), SNB(맨 키 글자 밖의 글자가 든 따옴표 조각 있음),
# SSP(점 앞뒤에 공백이 있음), SROOT(첫 조각, 이스케이프나 탭이 있으면 빈칸).
# 읽지 못하면 bad 를 채우고 0 을 돌려준다.
function segments(t, pos,    n, c, j, seg, p0) {
    n = length(t); SN = 0; SKEY = ""; SQ = 0; SUNK = 0; SNB = 0; SSP = 0; SROOT = ""
    while (1) {
        p0 = pos
        while (pos <= n && (substr(t, pos, 1) == " " || substr(t, pos, 1) == "\t")) pos++
        if (SN > 0 && pos > p0) SSP = 1
        c = substr(t, pos, 1)
        if (c == "\"" || c == "'") {
            for (j = pos + 1; j <= n; j++) {
                if (c == "\"" && substr(t, j, 1) == "\\") { j++; continue }
                if (substr(t, j, 1) == c) break
            }
            if (j > n) { bad = NR ": 따옴표 키가 닫히지 않았다"; return 0 }
            seg = substr(t, pos + 1, j - pos - 1); SQ = 1
            if ((c == "\"" && index(seg, "\\") > 0) || index(seg, "\t") > 0) { SUNK = 1; if (SN == 0) seg = "" }
            if (seg !~ /^[A-Za-z0-9_-]+$/) SNB = 1
            pos = j + 1
        } else if (match(substr(t, pos), /^[A-Za-z0-9_-]+/)) {
            seg = substr(t, pos, RLENGTH); pos += RLENGTH
        } else { bad = NR ": 키를 읽지 못했다"; return 0 }
        SN++
        if (SN == 1) SROOT = seg
        SKEY = (SN == 1) ? seg : SKEY "." seg
        p0 = pos
        while (pos <= n && (substr(t, pos, 1) == " " || substr(t, pos, 1) == "\t")) pos++
        if (substr(t, pos, 1) == ".") { if (pos > p0) SSP = 1; pos++; continue }
        break
    }
    SPOS = pos
    return 1
}

{
    sub(/\r$/, "")
    if (bad != "") next
    line = $0
    if (ml != "" || depth > 0) { scan(line, 1); next }
    t = line; sub(/^[ \t]+/, "", t)
    if (t == "" || substr(t, 1, 1) == "#") next
    if (substr(t, 1, 1) == "[") {
        arr = (substr(t, 1, 2) == "[[")
        shut = arr ? "]]" : "]"
        if (!segments(t, arr ? 3 : 2)) next
        if (substr(t, SPOS, length(shut)) != shut) { bad = NR ": 표 머리를 읽지 못했다"; next }
        tail = trim(substr(t, SPOS + length(shut)))
        if (tail != "" && substr(tail, 1, 1) != "#") { bad = NR ": 표 머리 뒤에 읽을 수 없는 내용이 있다"; next }
        hform = (SUNK || SNB) ? "unknown" : (SQ ? "quoted" : (SSP ? "spaced" : "bare"))
        name = (hform == "unknown") ? "" : SKEY
        printf "H\t%s\t%s\t%s\n", (arr ? "[[" name "]]" : "[" name "]"), hform, SROOT
        next
    }
    if (!segments(t, 1)) next
    pos = SPOS
    kform = (SN > 1) ? "dotted" : (SQ ? "quoted" : "bare")
    key = SKEY
    if (SUNK) { kform = "unknown"; key = "" }
    rest = substr(t, pos)
    rest = trim(rest)
    if (substr(rest, 1, 1) != "=") { bad = NR ": 키 뒤에 = 가 없다"; next }
    v = trim(substr(rest, 2))
    val = ""; vform = "other"
    if (match(v, /^"[A-Za-z0-9._-]+"[ \t]*(#.*)?$/)) vform = "basic"
    else if (match(v, /^'[A-Za-z0-9._-]+'[ \t]*(#.*)?$/)) vform = "literal"
    else if (match(v, /^(true|false)[ \t]*(#.*)?$/)) { vform = "bool"; val = substr(v, 1, (substr(v, 1, 1) == "t") ? 4 : 5) }
    if (vform == "basic" || vform == "literal") { q = substr(v, 1, 1); val = substr(v, 2); val = substr(val, 1, index(val, q) - 1) }
    printf "K\t%s\t%s\t%s\t%s\t%s\n", kform, key, vform, val, SROOT
    scan(v, 1)
}

END {
    if (bad == "" && ml != "") bad = "여러 줄 문자열이 닫히지 않았다"
    if (bad == "" && depth != 0) bad = "배열·인라인 표 괄호가 닫히지 않았다"
    if (bad != "") { printf "E\t%s\n", bad; exit 3 }
}
