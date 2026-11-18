// TastyInputHelper: 개발용 macOS 키 입력 도우미.
// 키코드 CGEvent를 게시하고 TIS 입력 소스를 바꾼다. 문자열 주입은 하지 않는다(OS IME를 거치게 하기 위해서다).
// `open -g -W -a TastyInputHelper.app --args --result <경로> [--command-file <경로>]`로 띄워
// 이 앱이 손쉬운 사용 권한의 책임 프로세스가 되게 한다. 사용법은 docs/dev-guide/build.md의
// "macOS 키 입력 도우미" 절에 있다.
//
// 명령 파일은 한 줄에 한 단계다. 빈 줄과 #로 시작하는 줄은 건너뛴다.
//   switch_source <TIS 입력 소스 ID>
//   key <가상 키코드> <press|release|click>
//   sleep <ms>
//   delay <ms>              이후 키 이벤트 사이 지연(기본 30ms)
//   activate <pid>
//   require_front <pid>     앞 앱 PID가 다르면 키를 보내지 않고 종료 코드 5로 끝낸다
// 수식키는 수식키 키코드(Command 55, Shift 56, Option 58, Control 59 등)를 press/release로 보낸다.
// 도우미는 누른 수식키를 기억해 그 뒤 모든 키 이벤트에 같은 플래그를 싣는다.

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
    "modifier_method": "modifier keycode press/release; held modifiers are also set as flags on every posted event",
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
func finish(_ code: Exit, _ message: String? = nil) -> Never {
    result["steps"] = steps
    result["source_after"] = currentSourceID() ?? NSNull()
    result["finished_at"] = isoFormatter.string(from: Date())
    result["exit_code"] = Int(code.rawValue)
    if let message { result["message"] = message }
    if let path = resultPath {
        let tmp = path + ".tmp.\(getpid())"
        do {
            let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: URL(fileURLWithPath: tmp))
            if rename(tmp, path) != 0 {
                FileHandle.standardError.write("rename failed: \(String(cString: strerror(errno)))\n".data(using: .utf8)!)
            }
        } catch {
            FileHandle.standardError.write("result write failed: \(error)\n".data(using: .utf8)!)
        }
    }
    if let message { FileHandle.standardError.write((message + "\n").data(using: .utf8)!) }
    exit(code.rawValue)
}

// 앞 앱 정보는 실행 루프가 돌아야 갱신되므로 대기는 실행 루프를 돌리며 한다.
func wait(ms: Int) {
    RunLoop.current.run(until: Date(timeIntervalSinceNow: Double(ms) / 1000))
}

// LaunchServices 등록을 위해 NSApplication을 만든다. LSUIElement 앱이라 Dock·메뉴 막대에 나타나지 않는다.
_ = NSApplication.shared

var commandFile: String?
var argIndex = 1
let args = CommandLine.arguments
while argIndex < args.count {
    let arg = args[argIndex]
    switch arg {
    case "--result", "--command-file":
        guard argIndex + 1 < args.count else { finish(.usage, "\(arg) needs a path") }
        if arg == "--result" { resultPath = args[argIndex + 1] } else { commandFile = args[argIndex + 1] }
        argIndex += 2
    default:
        // 옛 LaunchServices가 붙이는 -psn_ 인자는 무시한다.
        if arg.hasPrefix("-psn_") { argIndex += 1; continue }
        finish(.usage, "unknown argument: \(arg)")
    }
}
guard resultPath != nil else { finish(.usage, "--result <path> is required") }

let axTrusted = AXIsProcessTrusted()
let postAccess = CGPreflightPostEventAccess()
result["AXIsProcessTrusted"] = axTrusted
result["CGPreflightPostEventAccess"] = postAccess
result["source_before"] = currentSourceID() ?? NSNull()
result["command_file"] = commandFile ?? NSNull()

var lines: [(Int, [String])] = []
if let commandFile {
    guard let text = try? String(contentsOfFile: commandFile, encoding: .utf8) else {
        finish(.usage, "cannot read command file: \(commandFile)")
    }
    for (offset, raw) in text.components(separatedBy: "\n").enumerated() {
        let line = raw.trimmingCharacters(in: .whitespaces)
        if line.isEmpty || line.hasPrefix("#") { continue }
        lines.append((offset + 1, line.split(separator: " ", omittingEmptySubsequences: true).map(String.init)))
    }
}

// 명령 파일 형식 오류는 권한 판정과 키 게시보다 먼저 거른다.
for (lineNo, words) in lines {
    let known: [String: Int] = ["switch_source": 2, "key": 3, "sleep": 2, "delay": 2, "activate": 2, "require_front": 2]
    // switch_source 밖의 명령은 둘째 낱말이 0 이상의 정수다. 단어 수를 먼저 확인하므로 색인은 안전하다.
    let numeric = words[0] == "switch_source" ? [] : [1]
    let directionOK = words[0] != "key" || ["press", "release", "click"].contains(words.last ?? "")
    guard known[words[0]] == words.count, directionOK,
          numeric.allSatisfy({ Int(words[$0]).map { $0 >= 0 } ?? false })
    else {
        finish(.usage, "line \(lineNo): unknown or malformed command: \(words.joined(separator: " "))")
    }
}

// 권한 요청 함수는 대화상자를 띄우므로 부르지 않는다. 게시 권한이 없으면 이벤트가 오류 없이 버려지므로
// 어떤 단계도 실행하지 않고 끝낸다.
guard postAccess else { finish(.noPermission, "CGPreflightPostEventAccess is false; no step was run") }

let eventSource = CGEventSource(stateID: .combinedSessionState)
var heldFlags = CGEventFlags()
var delayMs = 30

func postKey(_ keycode: CGKeyCode, down: Bool) throws {
    guard let event = CGEvent(keyboardEventSource: eventSource, virtualKey: keycode, keyDown: down) else {
        throw StepError(code: .failure, message: "CGEvent creation failed for keycode \(keycode)")
    }
    if let flag = modifierFlags[keycode] {
        if down { heldFlags.insert(flag) } else { heldFlags.remove(flag) }
        event.type = .flagsChanged
    }
    event.flags = heldFlags
    event.post(tap: .cgAnnotatedSessionEventTap)
    wait(ms: delayMs)
}

func parseInt(_ s: String, _ what: String) throws -> Int {
    guard let v = Int(s), v >= 0 else { throw StepError(code: .usage, message: "bad \(what): \(s)") }
    return v
}

func run(_ words: [String]) throws -> [String: Any] {
    var info: [String: Any] = [:]
    switch (words[0], words.count) {
    case ("switch_source", 2):
        let filter = [kTISPropertyInputSourceID as String: words[1]] as CFDictionary
        let list = TISCreateInputSourceList(filter, false)?.takeRetainedValue() as? [TISInputSource] ?? []
        guard let source = list.first else { throw StepError(code: .inputSource, message: "input source not found: \(words[1])") }
        let status = TISSelectInputSource(source)
        wait(ms: 200)
        let now = currentSourceID()
        info["status"] = Int(status)
        info["source_now"] = now ?? NSNull()
        guard status == noErr, now == words[1] else {
            throw StepError(code: .inputSource, message: "switch failed: status \(status), current \(now ?? "nil")")
        }
    case ("key", 3):
        let keycode = try parseInt(words[1], "keycode")
        guard keycode <= Int(UInt16.max) else { throw StepError(code: .usage, message: "bad keycode: \(words[1])") }
        let code = CGKeyCode(keycode)
        switch words[2] {
        case "press": try postKey(code, down: true)
        case "release": try postKey(code, down: false)
        case "click": try postKey(code, down: true); try postKey(code, down: false)
        default: throw StepError(code: .usage, message: "bad direction: \(words[2])")
        }
        info["flags_after"] = heldFlags.rawValue
    case ("sleep", 2):
        wait(ms: try parseInt(words[1], "ms"))
    case ("delay", 2):
        delayMs = try parseInt(words[1], "ms")
    case ("activate", 2):
        let pid = try parseInt(words[1], "pid")
        guard let app = NSRunningApplication(processIdentifier: pid_t(pid)) else {
            throw StepError(code: .failure, message: "no running application with pid \(pid)")
        }
        info["activated"] = app.activate(options: [.activateAllWindows])
        wait(ms: 300)
    case ("require_front", 2):
        let pid = try parseInt(words[1], "pid")
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
        info["front_pid"] = front.map { Int($0) } ?? NSNull()
        guard front == pid_t(pid) else {
            throw StepError(code: .frontMismatch, message: "frontmost pid \(front.map { String($0) } ?? "nil") is not \(pid)")
        }
    default:
        throw StepError(code: .usage, message: "unknown or malformed command: \(words.joined(separator: " "))")
    }
    return info
}


for (lineNo, words) in lines {
    var step: [String: Any] = ["line": lineNo, "command": words.joined(separator: " ")]
    do {
        step.merge(try run(words)) { _, new in new }
        step["ok"] = true
        steps.append(step)
    } catch let error as StepError {
        step["ok"] = false
        step["error"] = error.message
        steps.append(step)
        // 눌린 채로 남은 수식키를 떼어 다음 입력이 수식키를 물고 들어가지 않게 한다.
        for (keycode, flag) in modifierFlags.sorted(by: { $0.key < $1.key }) where heldFlags.contains(flag) {
            try? postKey(keycode, down: false)
        }
        finish(error.code, "line \(lineNo): \(error.message)")
    } catch {
        finish(.failure, "line \(lineNo): \(error)")
    }
}
finish(.ok)
