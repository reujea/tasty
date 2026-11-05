/**
 * Landing-page copy, one set per site language. Chrome strings live in `strings.ts`.
 *
 * Describe the terminal first, then extension and agent integration, in concrete terms.
 */
export interface Card {
  /** A name from the design system's icon registry. */
  icon: string;
  title: string;
  body: string;
  /** Guide page this card leads to, relative to the guide root. */
  href: string;
}
export interface GuideLink {
  label: string;
  /** Guide page, relative to the guide root. */
  href: string;
}
export interface Copy {
  badge: string;
  tagline: string;
  titleLead: string;
  titleAccent: string;
  titleTail: string;
  body: string;
  ctaPrimary: string;
  ctaSecondary: string;
  ctaOther: string;
  /** Primary button label once the visitor's OS is known; `{os}` is replaced. */
  dlFor: string;
  installNote: string;
  shellCaption: string;

  layoutTitle: string;
  layoutBody: string;
  layoutCaption: string;
  layoutLink: GuideLink;

  featuresTitle: string;
  featuresBody: string;
  cards: Card[];
  moreLinks: GuideLink[];

  agentsTitle: string;
  /** Holds inline `<code>`. */
  agentsBody: string;
  agentsIntegration: string;
  agentsNote: string;
  agentsLinks: GuideLink[];
  agentsCaption: string;

  platformTitle: string;
  platformBody: string;

  ctaTitle: string;
  ctaBody: string;
  ctaDownload: string;
  ctaInstall: string;

  /** Strings rendered inside the app mockup itself. */
  shellHeading: string;
  shellTask: string;
}

export const KO: Copy = {
  badge: "Windows · macOS · Linux",
  tagline: "내 입맛에 맞는 터미널",
  titleLead: "작업을 나누고 배치하는",
  titleAccent: "GPU 가속 터미널",
  titleTail: "",
  body: "Tasty는 프로젝트별로 워크스페이스를 나누고, 여러 터미널과 문서를 한 화면에 배치합니다. Windows, macOS, Linux에서 사용할 수 있습니다.",
  ctaPrimary: "다운로드",
  ctaSecondary: "설치 가이드",
  ctaOther: "모든 설치 파일",
  dlFor: "{os}용 다운로드",
  installNote: "OS와 아키텍처에 맞는 파일을 설치 가이드에서 확인하세요.",
  shellCaption: "Tasty의 화면 배치 예시입니다. 탭과 메뉴를 살펴볼 수 있으며, 표시된 명령은 실행되지 않습니다.",

  layoutTitle: "탭을 따로 바꾸거나, 함께 바꾸는 두 단계 분할",
  layoutBody: "페인은 각각의 탭 바를 가집니다. 한쪽에서 탭을 바꿔도 다른 페인의 화면은 남습니다. 탭 안에서는 여러 서피스를 나란히 배치하고, 탭을 바꿀 때 함께 전환할 수 있습니다.",
  layoutCaption: "왼쪽에는 작업 터미널을, 오른쪽에는 테스트 로그나 문서를 둡니다.",
  layoutLink: { label: "작업 배치 만들기", href: "using/workflow-layouts/" },

  featuresTitle: "작업에 맞춰 구성하는 터미널",
  featuresBody: "작업별 배치를 저장하고, 터미널 옆에서 파일을 확인하고, 자주 쓰는 조작을 설정할 수 있습니다.",
  cards: [
    { icon: "layers", title: "프로젝트별 워크스페이스", body: "터미널과 배치를 프로젝트별로 관리합니다. 이름과 카테고리로 작업을 구분할 수 있습니다.", href: "using/workspaces/" },
    { icon: "layoutGrid", title: "다시 쓰는 배치", body: "자주 쓰는 배치와 시작 설정을 프리셋으로 저장해 새 작업에 적용합니다.", href: "using/workflow-layouts/" },
    { icon: "folderOpen", title: "파일 관리와 보기", body: "탐색기에서 파일을 관리하고, 마크다운·이미지·HTML을 터미널 옆에서 확인합니다. 뷰어는 플러그인으로 제공합니다.", href: "using/files/" },
    { icon: "textLeft", title: "출력 검색과 복사", body: "터미널 기록을 검색하고 필요한 출력을 복사합니다. 셸 통합이 있으면 CLI로 명령별 출력도 확인할 수 있습니다.", href: "using/terminal/" },
    { icon: "keyboard", title: "내 손에 맞는 단축키", body: "프리셋을 고르거나 자주 쓰는 동작의 키 조합을 바꿉니다.", href: "customize/keybindings/" },
    { icon: "theme", title: "밝고 어두운 테마", body: "밝은 Latte와 어두운 Mocha를 고르고, TOML 파일로 테마를 만들 수 있습니다.", href: "customize/themes/" },
  ],
  moreLinks: [
    { label: "반복 작업을 Lua로 설정하기", href: "customize/scripts/" },
    { label: "원격 작업 연결하기", href: "remote/attach/" },
    { label: "필요한 플러그인 관리하기", href: "plugins/" },
  ],

  agentsTitle: "CLI로 터미널을 조작하고 에이전트를 연결합니다",
  agentsBody: "스크립트와 AI 에이전트는 <code>tasty</code> CLI로 대상 ID를 지정해 터미널을 만들고, 명령을 보내고, 출력을 읽습니다. CLI로 새 탭을 열어도 사용자가 보던 탭과 포커스·선택·스크롤 위치는 바뀌지 않습니다.",
  agentsIntegration: "Claude Code와 Codex CLI는 별도로 설치합니다. Tasty 플러그인과 훅을 설정하면 에이전트의 상태를 확인할 수 있습니다. 자식 에이전트의 응답을 부모에게 전달하는 절차는 연동 가이드에서 설명합니다.",
  agentsNote: "응답이 끝났다는 알림은 작업이 성공했다는 판정과 다릅니다.",
  agentsLinks: [
    { label: "CLI 시작하기", href: "agents/cli/" },
    { label: "Claude·Codex 연결하기", href: "agents/claude-codex/" },
    { label: "작업 순서 관리", href: "agents/tasks/" },
  ],
  agentsCaption: "실행 중인 Tasty의 연결 정보와 작업 대상을 조회합니다.",

  platformTitle: "Windows · macOS · Linux에서",
  platformBody: "워크스페이스, 탭, 화면 분할로 작업을 구성합니다. macOS는 Apple Silicon을 지원합니다. OS와 아키텍처별 설치 파일과 실행 조건은 설치 가이드에 정리했습니다.",

  ctaTitle: "Tasty 설치하기",
  ctaBody: "내 컴퓨터에 맞는 설치 파일을 고르고, 첫 워크스페이스를 열어보세요.",
  ctaDownload: "다운로드",
  ctaInstall: "설치 가이드",

  shellHeading: "워크스페이스",
  shellTask: "다음 릴리스 준비",
};

export const EN: Copy = {
  badge: "Windows · macOS · Linux",
  tagline: "A terminal to your taste.",
  titleLead: "A",
  titleAccent: "GPU-accelerated terminal",
  titleTail: "for organizing your work",
  body: "Tasty organizes projects into workspaces and arranges terminals and documents side by side. Available on Windows, macOS, and Linux.",
  ctaPrimary: "Download",
  ctaSecondary: "Installation guide",
  ctaOther: "All installation files",
  dlFor: "Download for {os}",
  installNote: "Check the installation guide for the file that matches your OS and architecture.",
  shellCaption: "An example Tasty layout. Explore tabs and menus; the displayed commands do not run.",

  layoutTitle: "Two levels of splitting, with independent or shared tab switching",
  layoutBody: "Each pane has its own tab bar, so switching tabs in one pane leaves the others in place. Within a tab, arrange several surfaces side by side and switch them together.",
  layoutCaption: "Keep a working terminal on the left, with test logs or documents on the right.",
  layoutLink: { label: "Build a working layout", href: "using/workflow-layouts/" },

  featuresTitle: "A terminal you can configure around your work",
  featuresBody: "Save layouts for your tasks, inspect files beside the terminal, and configure the actions you use often.",
  cards: [
    { icon: "layers", title: "Project workspaces", body: "Keep each project’s terminals and layout together. Use names and categories to organize your work.", href: "using/workspaces/" },
    { icon: "layoutGrid", title: "Reusable layouts", body: "Save layouts and startup settings as presets, then apply them to new work.", href: "using/workflow-layouts/" },
    { icon: "folderOpen", title: "File management and viewers", body: "Manage files in the Explorer and view Markdown, images, and HTML beside the terminal. Viewers are provided by plugins.", href: "using/files/" },
    { icon: "textLeft", title: "Search and copy output", body: "Search terminal history and copy the output you need. With shell integration, the CLI can also read output by command.", href: "using/terminal/" },
    { icon: "keyboard", title: "Keybindings that fit", body: "Choose a preset or change shortcuts for the actions you use often.", href: "customize/keybindings/" },
    { icon: "theme", title: "Light and dark themes", body: "Choose light Latte or dark Mocha, or create a theme in a TOML file.", href: "customize/themes/" },
  ],
  moreLinks: [
    { label: "Automate repeated actions with Lua", href: "customize/scripts/" },
    { label: "Connect to remote work", href: "remote/attach/" },
    { label: "Manage plugins", href: "plugins/" },
  ],

  agentsTitle: "Control terminals through the CLI and connect your agents",
  agentsBody: "Scripts and AI agents use the <code>tasty</code> CLI to address terminals by ID, create them, send commands, and read output. Opening a tab through the CLI keeps your current tab, focus, selection, and scroll position.",
  agentsIntegration: "Install Claude Code and Codex CLI separately. Configure the Tasty plugins and hooks to follow agent status. The integration guide explains how to deliver a child agent’s response to its parent.",
  agentsNote: "A notification that a response ended does not mean the task succeeded.",
  agentsLinks: [
    { label: "Start with the CLI", href: "agents/cli/" },
    { label: "Connect Claude and Codex", href: "agents/claude-codex/" },
    { label: "Task workflows", href: "agents/tasks/" },
  ],
  agentsCaption: "Inspect the running Tasty instance and find your targets.",

  platformTitle: "On Windows, macOS, and Linux",
  platformBody: "Organize your work with workspaces, tabs, and splits. macOS support is for Apple Silicon. The installation guide lists files and requirements for each OS and architecture.",

  ctaTitle: "Install Tasty",
  ctaBody: "Choose the installation file for your computer and open your first workspace.",
  ctaDownload: "Download",
  ctaInstall: "Installation guide",

  shellHeading: "Workspaces",
  shellTask: "prepare the next release",
};

export const copyFor = (lang: "ko" | "en"): Copy => (lang === "ko" ? KO : EN);
