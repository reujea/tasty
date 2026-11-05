/**
 * Landing-page copy, one set per site language. Chrome strings live in `strings.ts`.
 *
 * Describe terminal features and agent integration in concrete, everyday terms.
 */
export interface Point {
  key: string;
  title: string;
  body: string;
}
export interface Card {
  /** A name from the design system's icon registry (`src/ds/core/Icon.jsx`). */
  icon: string;
  title: string;
  body: string;
  /** Guide page this card leads to, relative to the guide root. */
  href: string;
}
export interface Copy {
  badge: string;
  /** The wordmark line: the pun only lands in English. */
  tagline: string;
  titleLead: string;
  titleAccent: string;
  titleTail: string;
  ctaPrimary: string;
  ctaSecondary: string;
  ctaOther: string;
  /** Primary button label once the visitor's OS is known; `{os}` is replaced. */
  dlFor: string;
  installNote: string;
  installNoteLink: string;

  whyTitle: string;
  whyBody: string;
  whyPoints: Point[];

  featuresKicker: string;
  featuresTitle: string;
  featuresBody: string;
  cards: Card[];

  agentsKicker: string;
  agentsTitle: string;
  agentsBody: string;
  agentsCaption: string;
  agentsTab: string;

  platformKicker: string;
  platformTitle: string;
  platformBody: string;
  stats: [string, string][];

  ctaTitle: string;
  ctaBody: string;
  ctaDocs: string;
  ctaDownload: string;

  /** Strings rendered inside the app mockup itself. */
  shellHeading: string;
  shellTask: string;
}

export const KO: Copy = {
  badge: "Windows · macOS · Linux",
  tagline: "맛있는 터미널.",
  titleLead: "입맛에 맞게 쓰는",
  titleAccent: "터미널,",
  titleTail: "Tasty.",
  ctaPrimary: "다운로드",
  ctaSecondary: "가이드 보기",
  ctaOther: "다른 플랫폼",
  dlFor: "{os}용 다운로드",
  installNote: "설치가 처음이라면",
  installNoteLink: "설치 가이드를 참고하세요 →",

  whyTitle: "에이전트가 터미널을 직접 조작합니다",
  whyBody:
    "AI 에이전트가 tasty CLI로 터미널을 만들고, 명령을 실행하고, 출력을 읽을 수 있습니다. 사용자가 열어둔 터미널과 에이전트의 작업용 터미널을 함께 관리합니다.",
  whyPoints: [
    { key: "01", title: "보고 있던 탭 유지", body: "에이전트가 새 탭을 열어도 보고 있던 탭의 포커스, 선택한 텍스트, 스크롤 위치는 바뀌지 않습니다." },
    { key: "02", title: "ID로 터미널 지정", body: "각 터미널에는 고유한 ID가 있습니다. 에이전트는 이 ID로 작업할 터미널을 지정합니다." },
    { key: "03", title: "화면 분할과 출력 확인", body: "tasty CLI로 화면을 나누고, 명령을 보내고, 실행 결과를 읽습니다." },
    { key: "04", title: "서버에서 실행", body: "윈도우 없이 실행하는 헤드리스 모드를 지원합니다. 서버나 CI에서도 CLI로 터미널을 만들고 작업을 실행할 수 있습니다." },
  ],

  featuresKicker: "기능",
  featuresTitle: "화면 배치부터 파일 보기까지",
  featuresBody: "프로젝트마다 작업 공간을 나누고, 터미널 옆에 문서와 이미지를 열어둘 수 있습니다. 자주 쓰는 배치는 저장하고, 반복하는 동작은 스크립트로 실행합니다.",
  cards: [
    { icon: "layoutGrid", title: "GPU 가속 터미널", body: "GPU로 터미널 화면을 그립니다. 여러 터미널을 나란히 배치할 수 있습니다.", href: "using/workflow-layouts/" },
    { icon: "layers", title: "프로젝트별 작업 공간", body: "워크스페이스마다 터미널과 화면 배치를 따로 관리합니다. 자주 쓰는 배치는 프리셋으로 저장해 다시 불러옵니다.", href: "using/workspaces/" },
    { icon: "folderOpen", title: "문서와 이미지 보기", body: "마크다운, 이미지, 웹 페이지를 터미널 옆에 열어둘 수 있습니다. 탐색기에서는 파일을 찾고 복사하거나 옮깁니다.", href: "using/files/" },
    { icon: "rocket", title: "Claude와 Codex 연동", body: "여러 에이전트를 각각의 터미널에서 실행합니다. 연동을 설정하면 작업 완료와 입력 요청을 알림으로 받습니다.", href: "agents/claude-codex/" },
    { icon: "gitTree", title: "작업 순서 관리", body: "먼저 끝나야 할 작업을 연결하면 그 순서에 맞춰 실행합니다. 진행 상황은 그래프로 표시합니다.", href: "agents/tasks/" },
    { icon: "terminal", title: "터미널 조작용 CLI", body: "터미널 생성, 화면 분할, 명령 실행, 출력 확인을 CLI로 처리합니다. 스크립트와 AI 에이전트에서 사용할 수 있습니다.", href: "agents/cli/" },
    { icon: "textLeft", title: "명령별 출력 확인", body: "셸 연동으로 각 명령의 출력을 구분합니다. 긴 로그에서 필요한 명령의 결과를 찾아볼 수 있습니다.", href: "using/terminal/" },
    { icon: "bell", title: "명령 완료 알림", body: "명령이 끝나거나 출력에 특정 문구가 나타날 때 알림을 받도록 설정할 수 있습니다.", href: "agents/hooks-notifications/" },
    { icon: "scriptFile", title: "Lua 스크립트", body: "반복하는 동작을 스크립트로 작성합니다. 단축키로 실행하거나 윈도우와 탭이 열리고 닫힐 때 실행하도록 설정할 수 있습니다.", href: "customize/scripts/" },
    { icon: "remote", title: "SSH 원격 연결", body: "다른 컴퓨터의 Tasty에 SSH로 연결합니다. 그곳에서 실행 중인 워크스페이스를 보고 조작할 수 있습니다.", href: "remote/attach/" },
    { icon: "plug", title: "플러그인", body: "마크다운과 이미지 뷰어, Git 보기 등을 플러그인으로 제공합니다. 필요한 기능을 켜고 권한을 관리합니다.", href: "plugins/" },
    { icon: "theme", title: "테마 설정", body: "어두운 Mocha와 밝은 Latte 테마를 제공합니다. TOML 파일로 직접 테마를 만들 수도 있습니다.", href: "customize/themes/" },
  ],

  agentsKicker: "AI 에이전트 연동",
  agentsTitle: "Claude Code와 Codex를 같은 작업 공간에서",
  agentsBody:
    "여러 에이전트를 각각의 터미널에서 실행하고 상태를 확인합니다. 연동 가이드에 따라 알림을 설정하면 작업이 끝나거나 입력이 필요할 때 알려줍니다.",
  agentsCaption: "Claude Code와 Codex CLI는 별도로 설치해야 합니다. 에이전트끼리 작업을 맡기고 결과를 받는 방법은 연동 가이드에서 설명합니다.",
  agentsTab: "작업 관리",

  platformKicker: "플랫폼",
  platformTitle: "Windows · macOS · Linux 지원",
  platformBody: "세 운영체제에서 워크스페이스, 탭, 화면 분할을 같은 방식으로 사용합니다. 운영체제별 설치 방법과 지원 아키텍처는 설치 가이드에 정리했습니다.",
  stats: [["3", "운영체제"], ["GPU", "화면 렌더링"], ["3", "UI 언어"], ["MIT", "라이선스"]],

  ctaTitle: "Tasty 다운로드",
  ctaBody: "설치 파일은 릴리스 페이지에서 받을 수 있습니다. 설치 방법과 기본 사용법은 가이드에서 설명합니다.",
  ctaDocs: "가이드 읽기",
  ctaDownload: "다운로드",

  shellHeading: "워크스페이스",
  shellTask: "다음 릴리스 준비",
};

export const EN: Copy = {
  badge: "Windows · macOS · Linux",
  tagline: "Tasty terminal.",
  titleLead: "A terminal",
  titleAccent: "to your taste.",
  titleTail: "",
  ctaPrimary: "Download",
  ctaSecondary: "Read the guide",
  ctaOther: "Other platforms",
  dlFor: "Download for {os}",
  installNote: "Setting up for the first time?",
  installNoteLink: "Follow the installation guide →",

  whyTitle: "Agents can operate terminals directly",
  whyBody:
    "AI agents can use the tasty CLI to create terminals, run commands, and read output. Tasty manages the terminals you use alongside those your agents work in.",
  whyPoints: [
    { key: "01", title: "Keep the current tab", body: "When an agent opens a tab, your focus, selected text, and scroll position stay unchanged." },
    { key: "02", title: "Address terminals by ID", body: "Each terminal has its own ID. Agents use that ID to select the terminal for a task." },
    { key: "03", title: "Create splits and read output", body: "The tasty CLI can split the screen, send commands, and read their results." },
    { key: "04", title: "Run on a server", body: "Headless mode runs without a window. The CLI can create terminals and run tasks on servers or in CI." },
  ],

  featuresKicker: "Features",
  featuresTitle: "Layouts, files, and terminal tools",
  featuresBody: "Each project can have its own workspace, with documents and images beside the terminal. Layouts can be saved for reuse, and scripts can run repeated actions.",
  cards: [
    { icon: "layoutGrid", title: "GPU-accelerated terminal", body: "Tasty uses the GPU to draw the terminal. Multiple terminals can be arranged side by side.", href: "using/workflow-layouts/" },
    { icon: "layers", title: "Workspaces for each project", body: "Each workspace has its own terminals and layout. Layouts can be saved as presets and loaded again.", href: "using/workspaces/" },
    { icon: "folderOpen", title: "Documents and images", body: "Markdown, images, and web pages can open beside the terminal. The file explorer supports browsing, copying, and moving files.", href: "using/files/" },
    { icon: "rocket", title: "Claude and Codex integration", body: "Multiple agents can run in separate terminals. Once configured, the integration notifies you when work finishes or input is needed.", href: "agents/claude-codex/" },
    { icon: "gitTree", title: "Task dependencies", body: "Tasks run in the order set by their dependencies. A graph shows their progress.", href: "agents/tasks/" },
    { icon: "terminal", title: "Terminal CLI", body: "The CLI can create terminals, split the screen, run commands, and read output. Scripts and AI agents can use these commands.", href: "agents/cli/" },
    { icon: "textLeft", title: "Output by command", body: "Shell integration identifies each command’s output, making individual results easier to find in a long log.", href: "using/terminal/" },
    { icon: "bell", title: "Command notifications", body: "Notifications can be configured for a command finishing or a specific message appearing in its output.", href: "agents/hooks-notifications/" },
    { icon: "scriptFile", title: "Lua scripts", body: "Scripts can automate repeated actions. They can run from a shortcut or when windows and tabs open or close.", href: "customize/scripts/" },
    { icon: "remote", title: "Remote access over SSH", body: "Connect to Tasty on another computer over SSH to view and operate the workspace running there.", href: "remote/attach/" },
    { icon: "plug", title: "Plugins", body: "Plugins provide Markdown and image viewers, Git views, and other tools. Features can be enabled individually, with permissions managed for each plugin.", href: "plugins/" },
    { icon: "theme", title: "Themes", body: "Tasty includes the dark Mocha and light Latte themes. Custom themes can be written in TOML files.", href: "customize/themes/" },
  ],

  agentsKicker: "AI agent integration",
  agentsTitle: "Claude Code and Codex in the same workspace",
  agentsBody:
    "Run multiple agents in separate terminals and check their status. After notifications are set up using the integration guide, Tasty lets you know when work finishes or an agent needs input.",
  agentsCaption: "Claude Code and Codex CLI require separate installation. The integration guide also covers how agents delegate tasks and receive results.",
  agentsTab: "task coordinator",

  platformKicker: "Platforms",
  platformTitle: "Available on Windows, macOS, and Linux",
  platformBody: "Workspaces, tabs, and splits work the same way across all three operating systems. The installation guide covers setup and supported architectures for each OS.",
  stats: [["3", "operating systems"], ["GPU", "rendering"], ["3", "UI languages"], ["MIT", "license"]],

  ctaTitle: "Download Tasty",
  ctaBody: "Installation files are available on the releases page. The guide covers installation and basic use.",
  ctaDocs: "Read the guide",
  ctaDownload: "Download",

  shellHeading: "Workspaces",
  shellTask: "prepare the next release",
};

export const copyFor = (lang: "ko" | "en"): Copy => (lang === "ko" ? KO : EN);
