// TastyInputHelper: 개발용 macOS 키 입력 도우미.
// 키코드 CGEvent를 게시하고 TIS 입력 소스를 바꾼다. 문자열 주입은 하지 않는다(OS IME를 거치게 하기 위해서다).
// `open -g -W -n -a TastyInputHelper.app --args --result <경로> [--command-file <경로>]`로 띄워
// 이 앱이 손쉬운 사용 권한의 책임 프로세스가 되게 한다. 사용법은 docs/dev-guide/build.md의
// "macOS 키 입력 도우미" 절에 있다.
// `--args request-access [--result <경로>]`는 손쉬운 사용·이벤트 게시 권한을 시스템에 요청한다.
//
// 명령 파일은 한 줄에 한 단계다. 빈 줄과 #로 시작하는 줄은 건너뛴다.
//   switch_source <TIS 입력 소스 ID>
//   key <가상 키코드> <press|release|click>
//   sleep <ms>
//   delay <ms>              이후 키 이벤트 사이 지연(기본 30ms)
//   activate <pid>
//   require_front <pid>     앞 앱 PID가 다르면 키를 보내지 않고 종료 코드 5로 끝낸다
// 키코드는 0...65535, pid는 1...2147483647이다. 형식 오류는 권한 판정보다 먼저 종료 코드 2로 끝낸다.
// 수식키는 수식키 키코드(Command 55, Shift 56, Option 58, Control 59 등)를 press/release로 보낸다.
// 도우미는 눌린 키코드를 기억해 그 뒤 모든 키 이벤트에 눌린 수식키의 플래그를 싣고, 실패하면 눌린 키를 뗀다.

import AppKit
import ApplicationServices
import Carbon
import CoreGraphics

enum Exit: Int32 {
    case ok = 0
    case failure = 1
    case usage = 2
    case noPermission = 3
    case inputSource = 4
    case frontMismatch = 5
}

struct StepError: Error {
    let code: Exit
    let message: String
}

enum Direction: String {
    case press, release, click
}

// 사전 검사를 통과한 단계. 실행 중에는 형식 오류가 생기지 않는다.
enum Step {
    case switchSource(String)
    case key(CGKeyCode, Direction)
    case sleep(Int)
    case delay(Int)
    case activate(pid_t)
    case requireFront(pid_t)

    var postsKeys: Bool {
        if case .key = self { return true }
        return false
    }
}

// 수식키 키코드와 그 키가 켜는 플래그. 좌우 키를 구분하지 않고 같은 플래그를 쓴다.
let modifierFlags: [CGKeyCode: CGEventFlags] = [
    55: .maskCommand, 54: .maskCommand,
    56: .maskShift, 60: .maskShift,
    58: .maskAlternate, 61: .maskAlternate,
    59: .maskControl, 62: .maskControl,
    63: .maskSecondaryFn,
]

let isoFormatter = ISO8601DateFormatter()
isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]

var result: [String: Any] = [
    "helper_pid": Int(getpid()),
    "started_at": isoFormatter.string(from: Date()),
    "bundle_id": Bundle.main.bundleIdentifier ?? NSNull(),
    "bundle_version": Bundle.main.infoDictionary?["CFBundleShortVersionString"] ?? NSNull(),
    "executable": CommandLine.arguments[0],
    "modifier_method": "modifier keycode press/release; flags of held modifier keycodes are set on every posted event",
    "event_source": "CGEventSourceStateID.combinedSessionState",
    "event_tap": "CGEventTapLocation.cgAnnotatedSessionEventTap",
]
var steps: [[String: Any]] = []
var resultPath: String?

func currentSourceID() -> String? {
    let source = TISCopyCurrentKeyboardInputSource().takeRetainedValue()
    guard let raw = TISGetInputSourceProperty(source, kTISPropertyInputSourceID) else { return nil }
    return Unmanaged<CFString>.fromOpaque(raw).takeUnretainedValue() as String
}

// 결과 파일은 임시 이름으로 쓴 뒤 rename해 읽는 쪽이 반쯤 쓴 파일을 보지 않게 한다.
// 결과 파일을 쓰지 못하면 호출자가 결과를 판정할 수 없으므로 종료 코드 1로 끝낸다.
func finish(_ code: Exit, _ message: String? = nil) -> Never {
    result["steps"] = steps
    result["source_after"] = currentSourceID() ?? NSNull()
    result["finished_at"] = isoFormatter.string(from: Date())
    result["exit_code"] = Int(code.rawValue)
    if let message { result["message"] = message }
    if let message { printError(message) }
    if let path = resultPath {
        let tmp = path + ".tmp.\(getpid())"
        do {
            let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: URL(fileURLWithPath: tmp))
        } catch {
            printError("result write failed: \(error)")
            exit(Exit.failure.rawValue)
        }
        if rename(tmp, path) != 0 {
            printError("result rename failed: \(String(cString: strerror(errno)))")
            unlink(tmp)
            exit(Exit.failure.rawValue)
        }
    }
    exit(code.rawValue)
}

func printError(_ message: String) {
    FileHandle.standardError.write((message + "\n").data(using: .utf8)!)
}

// 앞 앱 정보는 실행 루프가 돌아야 갱신되므로 대기는 실행 루프를 돌리며 한다.
func wait(ms: Int) {
    RunLoop.current.run(until: Date(timeIntervalSinceNow: Double(ms) / 1000))
}

// LaunchServices 등록을 위해 NSApplication을 만든다. LSUIElement 앱이라 Dock·메뉴 막대에 나타나지 않는다.
_ = NSApplication.shared

// 인자 오류는 끝까지 읽어 --result 경로를 안 뒤에 보고한다. 그래야 옛 결과 파일을 먼저 지울 수 있다.
var commandFile: String?
var requestAccessMode = false
var argError: String?
var argIndex = 1
let args = CommandLine.arguments
while argIndex < args.count {
    let arg = args[argIndex]
    switch arg {
    case "--result", "--command-file":
        guard argIndex + 1 < args.count else {
            argError = argError ?? "\(arg) needs a path"
            argIndex += 1
            continue
        }
        if arg == "--result" { resultPath = args[argIndex + 1] } else { commandFile = args[argIndex + 1] }
        argIndex += 2
    case "request-access":
        requestAccessMode = true
        argIndex += 1
    default:
        // 옛 LaunchServices가 붙이는 -psn_ 인자는 무시한다.
        if !arg.hasPrefix("-psn_") { argError = argError ?? "unknown argument: \(arg)" }
        argIndex += 1
    }
}
// 옛 결과 파일이 이번 실행 결과로 읽히지 않게 먼저 지운다.
if let path = resultPath, unlink(path) != 0, errno != ENOENT {
    let reason = String(cString: strerror(errno))
    resultPath = nil
    printError("cannot remove old result file \(path): \(reason)")
    exit(Exit.failure.rawValue)
}
if let argError { finish(.usage, argError) }
if requestAccessMode, commandFile != nil { finish(.usage, "request-access does not take --command-file") }
guard resultPath != nil || requestAccessMode else { finish(.usage, "--result <path> is required") }

let axTrusted = AXIsProcessTrusted()
let postAccess = CGPreflightPostEventAccess()
result["AXIsProcessTrusted"] = axTrusted
result["CGPreflightPostEventAccess"] = postAccess
result["source_before"] = currentSourceID() ?? NSNull()
result["command_file"] = commandFile ?? NSNull()
result["requested_access"] = false

// 시스템 요청 창을 띄워 손쉬운 사용 목록에 이 앱을 올린다. 창은 앱마다 한 번만 뜬다.
func requestAccess() {
    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
    result["AXIsProcessTrustedWithOptions_prompt"] = AXIsProcessTrustedWithOptions(options)
    result["CGRequestPostEventAccess"] = CGRequestPostEventAccess()
    result["requested_access"] = true
    // 요청 창은 비동기로 뜨므로 잠시 실행 루프를 돌린 뒤 다시 조회한다.
    wait(ms: 500)
    result["AXIsProcessTrusted_after_request"] = AXIsProcessTrusted()
    result["CGPreflightPostEventAccess_after_request"] = CGPreflightPostEventAccess()
}

if requestAccessMode {
    requestAccess()
    if result["CGPreflightPostEventAccess_after_request"] as? Bool == true { finish(.ok) }
    finish(.noPermission, "access is not granted yet; allow the app in System Settings")
}

// 0 이상 max 이하의 10진 정수만 받는다.
func parseInt(_ s: String, max: Int) -> Int? {
    guard let v = Int(s), (0...max).contains(v) else { return nil }
    return v
}

func parseStep(_ words: [String]) -> Step? {
    switch (words[0], words.count) {
    case ("switch_source", 2):
        return .switchSource(words[1])
    case ("key", 3):
        guard let v = parseInt(words[1], max: Int(UInt16.max)), let code = CGKeyCode(exactly: v),
              let direction = Direction(rawValue: words[2]) else { return nil }
        return .key(code, direction)
    case ("sleep", 2):
        return parseInt(words[1], max: Int.max).map(Step.sleep)
    case ("delay", 2):
        return parseInt(words[1], max: Int.max).map(Step.delay)
    case ("activate", 2), ("require_front", 2):
        guard let v = parseInt(words[1], max: Int(Int32.max)), v >= 1, let pid = pid_t(exactly: v) else { return nil }
        return words[0] == "activate" ? .activate(pid) : .requireFront(pid)
    default:
        return nil
    }
}

// 명령 파일 형식 오류는 권한 판정·요청과 키 게시보다 먼저 거른다.
var lines: [(line: Int, command: String, step: Step)] = []
if let commandFile {
    guard let text = try? String(contentsOfFile: commandFile, encoding: .utf8) else {
        finish(.usage, "cannot read command file: \(commandFile)")
    }
    for (offset, raw) in text.components(separatedBy: "\n").enumerated() {
        let line = raw.trimmingCharacters(in: .whitespaces)
        if line.isEmpty || line.hasPrefix("#") { continue }
        let words = line.split(separator: " ", omittingEmptySubsequences: true).map(String.init)
        guard let step = parseStep(words) else {
            finish(.usage, "line \(offset + 1): unknown or malformed command: \(words.joined(separator: " "))")
        }
        lines.append((offset + 1, words.joined(separator: " "), step))
    }
}

// 게시 권한이 없으면 이벤트가 오류 없이 버려지므로 어떤 단계도 실행하지 않고 끝낸다.
// key 단계가 있을 때만 권한을 요청한다. 키를 보내지 않는 실행은 허용을 기다리며 반복 조회에 쓰므로 요청 창을 띄우지 않는다.
guard postAccess else {
    if lines.contains(where: { $0.step.postsKeys }) { requestAccess() }
    finish(.noPermission, "CGPreflightPostEventAccess is false; no step was run")
}

let eventSource = CGEventSource(stateID: .combinedSessionState)
// 누른 채로 있는 키코드. 플래그는 이 집합의 수식키에서 계산하므로, 같은 플래그의 한쪽 키를 떼도
// 다른 쪽이 눌려 있으면 플래그가 남는다.
var heldKeys = Set<CGKeyCode>()
var delayMs = 30

func heldFlags() -> CGEventFlags {
    heldKeys.reduce(into: CGEventFlags()) { flags, key in
        if let flag = modifierFlags[key] { flags.insert(flag) }
    }
}

func postKey(_ keycode: CGKeyCode, down: Bool) throws {
    guard let event = CGEvent(keyboardEventSource: eventSource, virtualKey: keycode, keyDown: down) else {
        throw StepError(code: .failure, message: "CGEvent creation failed for keycode \(keycode)")
    }
    if down { heldKeys.insert(keycode) } else { heldKeys.remove(keycode) }
    if modifierFlags[keycode] != nil { event.type = .flagsChanged }
    event.flags = heldFlags()
    event.post(tap: .cgAnnotatedSessionEventTap)
    wait(ms: delayMs)
}

func run(_ step: Step) throws -> [String: Any] {
    var info: [String: Any] = [:]
    switch step {
    case .switchSource(let id):
        let filter = [kTISPropertyInputSourceID as String: id] as CFDictionary
        let list = TISCreateInputSourceList(filter, false)?.takeRetainedValue() as? [TISInputSource] ?? []
        guard let source = list.first else { throw StepError(code: .inputSource, message: "input source not found: \(id)") }
        let status = TISSelectInputSource(source)
        wait(ms: 200)
        let now = currentSourceID()
        info["status"] = Int(status)
        info["source_now"] = now ?? NSNull()
        guard status == noErr, now == id else {
            throw StepError(code: .inputSource, message: "switch failed: status \(status), current \(now ?? "nil")")
        }
    case .key(let code, let direction):
        switch direction {
        case .press: try postKey(code, down: true)
        case .release: try postKey(code, down: false)
        case .click: try postKey(code, down: true); try postKey(code, down: false)
        }
        info["flags_after"] = heldFlags().rawValue
    case .sleep(let ms):
        wait(ms: ms)
    case .delay(let ms):
        delayMs = ms
    case .activate(let pid):
        guard let app = NSRunningApplication(processIdentifier: pid) else {
            throw StepError(code: .failure, message: "no running application with pid \(pid)")
        }
        info["activated"] = app.activate(options: [.activateAllWindows])
        wait(ms: 300)
    case .requireFront(let pid):
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
        info["front_pid"] = front.map { Int($0) } ?? NSNull()
        guard front == pid else {
            throw StepError(code: .frontMismatch, message: "frontmost pid \(front.map { String($0) } ?? "nil") is not \(pid)")
        }
    }
    return info
}

for entry in lines {
    var step: [String: Any] = ["line": entry.line, "command": entry.command]
    do {
        step.merge(try run(entry.step)) { _, new in new }
        step["ok"] = true
        steps.append(step)
    } catch {
        let failure = error as? StepError ?? StepError(code: .failure, message: "\(error)")
        step["ok"] = false
        step["error"] = failure.message
        steps.append(step)
        // 누른 채로 남은 키를 떼어 다음 입력이 그 키를 물고 들어가지 않게 한다. 수식키는 마지막에 뗀다.
        let held = heldKeys.sorted { (modifierFlags[$0] == nil ? 0 : 1, $0) < (modifierFlags[$1] == nil ? 0 : 1, $1) }
        step["released_on_failure"] = held.map { Int($0) }
        steps[steps.count - 1] = step
        for keycode in held { try? postKey(keycode, down: false) }
        finish(failure.code, "line \(entry.line): \(failure.message)")
    }
}
finish(.ok)
