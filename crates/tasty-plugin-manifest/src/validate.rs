//! 매니페스트를 읽고 선언의 형식·참조·권한을 검사한다.
//! 문자열 형식 검사는 validators 모듈을 사용한다.

use std::collections::HashSet;
use std::path::Path;

use super::gates::ContributesGate;
use super::types::{
    AutoWaitDecl, BindingMode, CliCommandDecl, CliSubcommandDecl, CommandDecl, CommandScope,
    HOOK_TIMEOUT_MS_MAX, HOST_API_VERSION, HookMode, MANIFEST_VERSION, Manifest, Permission,
    PopupTrigger, PresetFieldInputType, SettingsItemDecl, SettingsPageContribute, SurfaceKindDecl,
    ToolAction, ToolContribute,
};
use super::validators::{
    event_pattern_covers, event_pattern_namespace, is_reserved_event_namespace,
    is_reserved_hook_event_key, is_reserved_ipc_prefix, is_valid_cli_name, is_valid_command_id,
    is_valid_completion_strategy_id, is_valid_event_key, is_valid_event_pattern,
    is_valid_hook_event_key, is_valid_hook_handler_id, is_valid_ipc_prefix, is_valid_kind,
    is_valid_plugin_id, is_valid_settings_id, is_valid_simple_id, is_valid_tool_id,
    validate_cli_output, validate_webview_defaults,
};

impl Manifest {
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let manifest = Self::read(dir)?;
        manifest.validate()?;
        // 호스트 타입이 필요한 감지기·핸들러 본문 검사는 호출자가 추가로 수행한다.
        Ok(manifest)
    }

    /// `dir`의 `tasty-plugin.toml`을 읽고 파싱만 한다. 선언 검사는 [`Manifest::validate`]가 맡는다.
    /// 파일이 없거나 TOML·스키마로 읽을 수 없을 때만 실패한다.
    pub fn read(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join("tasty-plugin.toml");
        let s = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {}", path.display(), e))?;
        toml::from_str(&s)
            .map_err(|e| anyhow::anyhow!("invalid manifest at {}: {}", path.display(), e))
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.manifest_version != MANIFEST_VERSION {
            anyhow::bail!(
                "unsupported manifest_version: {} (expected {})",
                self.manifest_version,
                MANIFEST_VERSION
            );
        }
        if self.api_version != HOST_API_VERSION {
            anyhow::bail!(
                "plugin api_version '{}' incompatible with host '{}'",
                self.api_version,
                HOST_API_VERSION
            );
        }
        if !is_valid_plugin_id(&self.id) {
            anyhow::bail!(
                "invalid plugin id: '{}' (must be lowercase reverse-domain like com.example.x)",
                self.id
            );
        }
        for kind in &self.surface_kinds {
            if !is_valid_kind(&kind.kind) {
                anyhow::bail!(
                    "invalid surface kind: '{}' (must be lowercase ascii + '_' + digits)",
                    kind.kind
                );
            }
            self.validate_preset_fields(kind)?;
            validate_webview_defaults(kind)?;
        }
        for raw in &self.permissions {
            if Permission::from_token(raw).is_none() {
                anyhow::bail!(
                    "unknown permission '{}' in manifest (host may be older than plugin)",
                    raw
                );
            }
        }
        self.validate_contributes()?;
        self.validate_event_patterns()?;
        self.validate_events_emitted()?;
        self.validate_hook_events()?;
        self.contributes.strip_inherited_env.validate()?;
        crate::caller_context::validate(&self.contributes)?;
        crate::remote_attach::validate(&self.contributes)?;
        self.validate_extends()?;
        Ok(())
    }

    /// 프리셋 필드의 ID·필수 문자열·중복·경로 파생 조건을 확인한다.
    /// required_params도 있으면 그 각 항목이 required=true 필드에 포함돼야 한다.
    fn validate_preset_fields(&self, kind: &SurfaceKindDecl) -> anyhow::Result<()> {
        let mut seen_ids = HashSet::new();
        let mut required_param_keys = HashSet::new();
        for field in &kind.preset_fields {
            if !is_valid_settings_id(&field.id) {
                anyhow::bail!(
                    "invalid surface_kinds '{}' preset_field id '{}': must be lowercase ascii + digits + '_' + '-', length 1..=64",
                    kind.kind,
                    field.id
                );
            }
            if !seen_ids.insert(field.id.clone()) {
                anyhow::bail!(
                    "surface_kinds '{}' preset_field id '{}' declared twice",
                    kind.kind,
                    field.id
                );
            }
            if field.label_key.is_empty() {
                anyhow::bail!(
                    "surface_kinds '{}' preset_field '{}': label_key must not be empty",
                    kind.kind,
                    field.id
                );
            }
            if field.param_key.is_empty() {
                anyhow::bail!(
                    "surface_kinds '{}' preset_field '{}': param_key must not be empty",
                    kind.kind,
                    field.id
                );
            }
            if field.derive_cwd && field.input_type != PresetFieldInputType::FilePath {
                anyhow::bail!(
                    "surface_kinds '{}' preset_field '{}': derive_cwd is only valid for input_type = file_path",
                    kind.kind,
                    field.id
                );
            }
            if field.required {
                required_param_keys.insert(field.param_key.clone());
            }
        }
        // preset_fields를 선언한 경우에만 required_params와 대조한다.
        if !kind.preset_fields.is_empty() {
            for rp in &kind.required_params {
                if !required_param_keys.contains(rp) {
                    anyhow::bail!(
                        "surface_kinds '{}': required_params entry '{}' does not match any preset_field with required = true (preset_fields is the single source of truth)",
                        kind.kind,
                        rp
                    );
                }
            }
        }
        Ok(())
    }

    /// 확장 대상 ID·버전·프로토콜·권한 선언과 훅을 검사한다.
    /// 훅은 최소 하나 필요하고 timeout_ms는 1..=HOOK_TIMEOUT_MS_MAX여야 한다.
    /// 이벤트는 정확한 키여야 하고 IPC 메서드는 비어 있거나 *를 포함하면 안 된다.
    /// 대상 플러그인의 실제 선언과 사용자 권한 승인은 호스트가 추가로 확인한다.
    fn validate_extends(&self) -> anyhow::Result<()> {
        let Some(decl) = &self.extends else {
            return Ok(());
        };
        if !is_valid_plugin_id(&decl.plugin_id) {
            anyhow::bail!(
                "invalid extends.plugin_id '{}': must be lowercase reverse-domain",
                decl.plugin_id
            );
        }
        if decl.plugin_id == self.id {
            anyhow::bail!("extends.plugin_id must differ from this plugin's own id");
        }
        if let Err(e) = semver::VersionReq::parse(&decl.version_req) {
            anyhow::bail!("invalid extends.version_req '{}': {}", decl.version_req, e);
        }
        if decl.api_version != HOST_API_VERSION {
            anyhow::bail!(
                "extends.api_version '{}' incompatible with host '{}'",
                decl.api_version,
                HOST_API_VERSION
            );
        }
        let required_token = ContributesGate::Extends.required(&decl.plugin_id);
        if !self.permissions.iter().any(|p| p == &required_token) {
            anyhow::bail!(
                "[extends] requires permission '{required_token}' to be declared in manifest permissions[]"
            );
        }
        let total =
            decl.pre_event.len() + decl.post_event.len() + decl.pre_ipc.len() + decl.post_ipc.len();
        if total == 0 {
            anyhow::bail!(
                "[extends] block must declare at least one hook (pre_event / post_event / pre_ipc / post_ipc)"
            );
        }

        for h in decl.pre_event.iter().chain(decl.post_event.iter()) {
            validate_hook_timeout(h.timeout_ms, &h.event)?;
            if !is_valid_event_key(&h.event) {
                anyhow::bail!(
                    "extends event hook key '{}' must be a concrete event key (no '*')",
                    h.event
                );
            }
            validate_hook_mode_modifies(h.mode, &h.modifies, &h.event)?;
        }
        for h in decl.pre_ipc.iter().chain(decl.post_ipc.iter()) {
            validate_hook_timeout(h.timeout_ms, &h.method)?;
            if h.method.is_empty() || h.method.contains('*') {
                anyhow::bail!(
                    "extends ipc hook method '{}' must be a concrete method name",
                    h.method
                );
            }
            validate_hook_mode_modifies(h.mode, &h.modifies, &h.method)?;
        }
        Ok(())
    }

    /// 이벤트 키의 형식·예약 이름·발행 허용 패턴·중복을 검사한다.
    fn validate_events_emitted(&self) -> anyhow::Result<()> {
        let mut seen: HashSet<&str> = HashSet::new();
        for decl in &self.events_emitted {
            if !is_valid_event_key(&decl.key) {
                anyhow::bail!(
                    "invalid events_emitted key '{}': must be a concrete event key (no '*')",
                    decl.key
                );
            }
            let ns = event_pattern_namespace(&decl.key);
            if is_reserved_event_namespace(ns) {
                anyhow::bail!(
                    "events_emitted key '{}' uses reserved namespace '{}' — \
                     only the host may publish in this namespace",
                    decl.key,
                    ns
                );
            }
            let covered = self
                .event_publish
                .iter()
                .any(|p| event_pattern_covers(p, &decl.key));
            if !covered {
                anyhow::bail!(
                    "events_emitted key '{}' is not covered by any event_publish pattern",
                    decl.key
                );
            }
            if !seen.insert(decl.key.as_str()) {
                anyhow::bail!("events_emitted key '{}' declared twice", decl.key);
            }
        }
        Ok(())
    }

    /// 훅 키의 형식·내장 이벤트와의 충돌·중복을 검사한다.
    fn validate_hook_events(&self) -> anyhow::Result<()> {
        let mut seen: HashSet<&str> = HashSet::new();
        for decl in &self.contributes.hook_events {
            if !is_valid_hook_event_key(&decl.key) {
                anyhow::bail!(
                    "invalid contributes.hook_events key '{}': must be lowercase ascii + digits + '-', \
                     start with a letter, length ≤ 64, no '*'/':'/'.'",
                    decl.key
                );
            }
            if is_reserved_hook_event_key(&decl.key) {
                anyhow::bail!(
                    "contributes.hook_events key '{}' collides with a built-in hook event — \
                     built-in events (process-exit, bell, notification, output-match:, idle-timeout:) \
                     cannot be declared by plugins",
                    decl.key
                );
            }
            if !seen.insert(decl.key.as_str()) {
                anyhow::bail!("contributes.hook_events key '{}' declared twice", decl.key);
            }
        }
        Ok(())
    }

    /// 정확한 이벤트 키 또는 마지막 부분만 *인 패턴을 허용한다.
    /// 각 부분은 알파벳으로 시작하는 영문 소문자·숫자·밑줄이다.
    /// 호스트 전용 namespace는 발행할 수 없지만 구독은 허용한다.
    fn validate_event_patterns(&self) -> anyhow::Result<()> {
        for p in &self.event_subscribe {
            if !is_valid_event_pattern(p) {
                anyhow::bail!(
                    "invalid event_subscribe pattern '{}': must be a key or '<ns>.*' \
                     (segments: lowercase ascii + digits + '_', start with a letter)",
                    p
                );
            }
        }
        for p in &self.event_publish {
            if !is_valid_event_pattern(p) {
                anyhow::bail!(
                    "invalid event_publish pattern '{}': must be a key or '<ns>.*'",
                    p
                );
            }
            let ns = event_pattern_namespace(p);
            if is_reserved_event_namespace(ns) {
                anyhow::bail!(
                    "event_publish pattern '{}' uses reserved namespace '{}' — \
                     only the host may publish in this namespace",
                    p,
                    ns
                );
            }
        }
        Ok(())
    }

    fn validate_contributes(&self) -> anyhow::Result<()> {
        let seen_prefixes = self.validate_contributed_ipc_namespaces()?;
        self.validate_contributed_cli(&seen_prefixes)?;
        self.validate_contributed_tools()?;
        self.validate_contributed_commands()?;
        self.validate_contributed_popups()?;
        self.validate_contributed_banners()?;
        self.validate_contributed_windows()?;
        self.validate_contributed_settings_pages()?;
        self.validate_contributed_detectors()?;
        self.validate_contributed_handlers()?;
        self.validate_contributed_detector_permissions()?;
        self.validate_contributed_hook_handlers()?;
        self.validate_contributed_completion_strategies(&seen_prefixes)?;
        Ok(())
    }

    fn validate_contributed_ipc_namespaces(&self) -> anyhow::Result<HashSet<String>> {
        let mut seen_prefixes = HashSet::new();
        for ns in &self.contributes.ipc_namespace {
            if !is_valid_ipc_prefix(&ns.prefix) {
                anyhow::bail!(
                    "invalid ipc_namespace prefix '{}': must be lowercase ascii + digits + '_', \
                     start with a letter, length ≤ 32, no '.'",
                    ns.prefix
                );
            }
            if is_reserved_ipc_prefix(&ns.prefix) {
                anyhow::bail!(
                    "ipc_namespace prefix '{}' is reserved by the host",
                    ns.prefix
                );
            }
            if !seen_prefixes.insert(ns.prefix.clone()) {
                anyhow::bail!(
                    "ipc_namespace prefix '{}' declared twice in this manifest",
                    ns.prefix
                );
            }
        }
        Ok(seen_prefixes)
    }

    fn validate_contributed_cli(&self, seen_prefixes: &HashSet<String>) -> anyhow::Result<()> {
        let mut seen_cli_names = HashSet::new();
        for cli in &self.contributes.cli {
            if !is_valid_cli_name(&cli.name) {
                anyhow::bail!(
                    "invalid cli name '{}': must be lowercase ascii + digits + '-', \
                     start with a letter, length ≤ 32",
                    cli.name
                );
            }
            // 실제 호스트 CLI 이름과의 충돌은 tasty-cli의 build_augmented_cli가 처리한다.
            if !seen_cli_names.insert(cli.name.clone()) {
                anyhow::bail!("cli name '{}' declared twice in this manifest", cli.name);
            }
            Self::validate_cli_subcommands(cli, seen_prefixes, &self.id)?;
            Self::validate_cli_arg_groups(cli)?;
        }
        Ok(())
    }

    fn validate_cli_subcommands(
        cli: &CliCommandDecl,
        seen_prefixes: &HashSet<String>,
        plugin_id: &str,
    ) -> anyhow::Result<()> {
        let mut seen_sub_names = HashSet::new();
        for sub in &cli.subcommands {
            if !is_valid_cli_name(&sub.name) {
                anyhow::bail!(
                    "invalid cli subcommand name '{}' under '{}'",
                    sub.name,
                    cli.name
                );
            }
            if !seen_sub_names.insert(sub.name.clone()) {
                anyhow::bail!(
                    "cli subcommand name '{}' declared twice under '{}'",
                    sub.name,
                    cli.name
                );
            }
            if !cli.arg_groups.contains_key(&sub.args) {
                anyhow::bail!(
                    "cli subcommand '{} {}' references unknown arg group '{}'",
                    cli.name,
                    sub.name,
                    sub.args
                );
            }
            if sub.polling.is_some() && sub.auto_wait.is_some() {
                anyhow::bail!(
                    "cli subcommand '{} {}' declares both 'polling' and 'auto_wait' \
                         — choose one (polling = self-poll, auto_wait = chain to another method)",
                    cli.name,
                    sub.name
                );
            }
            validate_cli_output(cli, sub)?;
            if let Some(aw) = &sub.auto_wait {
                Self::validate_auto_wait_strategy(cli, sub, aw, plugin_id)?;
            }
            // ipc_method는 plugin 자기 namespace로 시작해야 한다.
            let Some(dot) = sub.ipc_method.find('.') else {
                anyhow::bail!(
                    "cli subcommand '{} {}' ipc_method '{}' has no namespace prefix",
                    cli.name,
                    sub.name,
                    sub.ipc_method
                );
            };
            let prefix = &sub.ipc_method[..dot];
            if !seen_prefixes.contains(prefix) {
                anyhow::bail!(
                    "cli subcommand '{} {}' ipc_method '{}' uses prefix '{}' \
                         which is not declared in this plugin's ipc_namespace",
                    cli.name,
                    sub.name,
                    sub.ipc_method,
                    prefix
                );
            }
        }
        Ok(())
    }

    /// polling과 strategy 중 하나만 있어야 한다.
    /// 이름으로 참조한 전략은 이 플러그인 소유여야 하며 이름 형식도 검사한다.
    fn validate_auto_wait_strategy(
        cli: &CliCommandDecl,
        sub: &CliSubcommandDecl,
        aw: &AutoWaitDecl,
        plugin_id: &str,
    ) -> anyhow::Result<()> {
        match (&aw.polling, &aw.strategy) {
            (Some(_), Some(_)) => anyhow::bail!(
                "cli subcommand '{} {}' auto_wait declares both 'polling' and 'strategy' \
                     — choose one",
                cli.name,
                sub.name
            ),
            (None, None) => anyhow::bail!(
                "cli subcommand '{} {}' auto_wait declares neither 'polling' nor 'strategy'",
                cli.name,
                sub.name
            ),
            (Some(_), None) => Ok(()),
            (None, Some(strategy)) => {
                let Some(slash) = strategy.find('/') else {
                    anyhow::bail!(
                        "cli subcommand '{} {}' auto_wait.strategy '{}' has no owner prefix \
                             (expected '<plugin_id>/<short-name>')",
                        cli.name,
                        sub.name,
                        strategy
                    );
                };
                let prefix = &strategy[..slash];
                if prefix != plugin_id {
                    anyhow::bail!(
                        "cli subcommand '{} {}' auto_wait.strategy '{}' prefix '{}' does not \
                             match this plugin's id '{}' — a plugin can only reference its own \
                             completion strategies",
                        cli.name,
                        sub.name,
                        strategy,
                        prefix,
                        plugin_id
                    );
                }
                let short_name = &strategy[slash + 1..];
                if !is_valid_hook_handler_id(short_name) {
                    anyhow::bail!(
                        "cli subcommand '{} {}' auto_wait.strategy '{}' short-name '{}' is \
                             invalid (must match [a-z0-9-]{{1,32}})",
                        cli.name,
                        sub.name,
                        strategy,
                        short_name
                    );
                }
                Ok(())
            }
        }
    }

    fn validate_cli_arg_groups(cli: &CliCommandDecl) -> anyhow::Result<()> {
        // arg group 내부 정합성: flag는 flags에만, positional은 flag 필드 없음.
        for (group_name, group) in &cli.arg_groups {
            for arg in &group.positional {
                if arg.flag.is_some() {
                    anyhow::bail!(
                        "arg group '{}.{}' positional arg '{}' must not have a 'flag' field",
                        cli.name,
                        group_name,
                        arg.name
                    );
                }
            }
            for arg in &group.flags {
                let Some(flag) = &arg.flag else {
                    anyhow::bail!(
                        "arg group '{}.{}' flag arg '{}' is missing 'flag' field",
                        cli.name,
                        group_name,
                        arg.name
                    );
                };
                if !flag.starts_with("--") {
                    anyhow::bail!(
                        "arg group '{}.{}' flag '{}' must start with '--'",
                        cli.name,
                        group_name,
                        flag
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_contributed_tools(&self) -> anyhow::Result<()> {
        if !self.contributes.tool.is_empty() {
            let token = ContributesGate::Tool.required("");
            if !self.permissions.iter().any(|p| p == &token) {
                anyhow::bail!(
                    "[[contributes.tool]] requires permission '{token}' to be declared in manifest permissions[]"
                );
            }
            let mut seen_tool_ids = HashSet::new();
            let surface_kinds: HashSet<&str> =
                self.surface_kinds.iter().map(|k| k.kind.as_str()).collect();
            for tool in &self.contributes.tool {
                if !is_valid_tool_id(&tool.id) {
                    anyhow::bail!(
                        "invalid contributes.tool id '{}': must be lowercase ascii + digits + '-', \
                         start with a letter, length ≤ 64",
                        tool.id
                    );
                }
                if !seen_tool_ids.insert(tool.id.clone()) {
                    anyhow::bail!(
                        "contributes.tool id '{}' declared twice in this manifest",
                        tool.id
                    );
                }
                if tool.label_i18n_key.is_empty() {
                    anyhow::bail!(
                        "contributes.tool '{}': label_i18n_key must not be empty",
                        tool.id
                    );
                }
                Self::validate_tool_action(tool, &surface_kinds)?;
            }
        }
        Ok(())
    }

    fn validate_tool_action(
        tool: &ToolContribute,
        surface_kinds: &HashSet<&str>,
    ) -> anyhow::Result<()> {
        Self::validate_action("contributes.tool", &tool.id, &tool.action, surface_kinds)
    }

    /// 도구·단축키가 공유하는 액션 형식을 검사한다. owner_label은 오류에 표시할 선언 종류다.
    fn validate_action(
        owner_label: &str,
        id: &str,
        action: &ToolAction,
        surface_kinds: &HashSet<&str>,
    ) -> anyhow::Result<()> {
        match action {
            ToolAction::Event { event_key } => {
                if !is_valid_event_key(event_key) {
                    anyhow::bail!(
                        "{} '{}': action.event_key '{}' must be a concrete event key",
                        owner_label,
                        id,
                        event_key
                    );
                }
            }
            ToolAction::OpenSurface { surface_kind } => {
                if !surface_kinds.contains(surface_kind.as_str()) {
                    anyhow::bail!(
                        "{} '{}': action.surface_kind '{}' is not declared in this plugin's [[surface_kinds]]",
                        owner_label,
                        id,
                        surface_kind
                    );
                }
            }
            ToolAction::OpenPopup { popup_id } => {
                // 여기서는 빈 값만 검사하고 아래에서 이 플러그인의 팝업 참조를 확인한다.
                if popup_id.is_empty() {
                    anyhow::bail!(
                        "{} '{}': action.popup_id must not be empty",
                        owner_label,
                        id
                    );
                }
            }
        }
        Ok(())
    }

    /// 명령 ID·중복·상속 대상·전역 단축키·액션 형식을 검사한다.
    fn validate_contributed_commands(&self) -> anyhow::Result<()> {
        if self.contributes.commands.is_empty() {
            return Ok(());
        }
        let has_popup_action = self
            .contributes
            .commands
            .iter()
            .any(|c| matches!(&c.action, Some(ToolAction::OpenPopup { .. })));
        let popup_token = ContributesGate::CommandOpenPopup.required("");
        if has_popup_action && !self.permissions.iter().any(|p| p == &popup_token) {
            anyhow::bail!(
                "[[contributes.commands]] with action.kind = 'open_popup' requires permission \
                 '{popup_token}' to be declared in manifest permissions[]"
            );
        }
        let surface_kinds: HashSet<&str> =
            self.surface_kinds.iter().map(|k| k.kind.as_str()).collect();
        let mut seen_command_ids = HashSet::new();
        for cmd in &self.contributes.commands {
            Self::validate_command(cmd, &surface_kinds)?;
            if !seen_command_ids.insert(cmd.id.clone()) {
                anyhow::bail!(
                    "contributes.commands id '{}' declared twice in this manifest",
                    cmd.id
                );
            }
        }
        Ok(())
    }

    fn validate_command(cmd: &CommandDecl, surface_kinds: &HashSet<&str>) -> anyhow::Result<()> {
        if !is_valid_command_id(&cmd.id) {
            anyhow::bail!(
                "invalid contributes.commands id '{}': must be '.'-separated lowercase ascii + \
                 digits + '_' segments (e.g. 'explorer.refresh'), length ≤ 64",
                cmd.id
            );
        }
        if cmd.title_i18n_key.is_empty() {
            anyhow::bail!(
                "contributes.commands '{}': title_i18n_key must not be empty",
                cmd.id
            );
        }
        if let BindingMode::InheritHost(action) = &cmd.binding_mode
            && !crate::host_actions::is_inheritable(action)
        {
            anyhow::bail!(
                "contributes.commands '{}': binding_mode 'inherit:{}' refers to a host action \
                 not in the inherit whitelist",
                cmd.id,
                action
            );
        }
        if cmd.scope == CommandScope::Global
            && let Some(kb) = &cmd.default_keybinding
            && !kb.is_empty()
            && !kb.contains('+')
        {
            anyhow::bail!(
                "contributes.commands '{}': scope 'global' commands must bind a combination key \
                 (default_keybinding '{}' has no modifier) — single keys are reserved for scope 'surface'",
                cmd.id,
                kb
            );
        }
        if let Some(action) = &cmd.action {
            Self::validate_action("contributes.commands", &cmd.id, action, surface_kinds)?;
        }
        Ok(())
    }

    fn validate_contributed_popups(&self) -> anyhow::Result<()> {
        if !self.contributes.popup.is_empty() {
            let token = ContributesGate::Popup.required("");
            if !self.permissions.iter().any(|p| p == &token) {
                anyhow::bail!(
                    "[[contributes.popup]] requires permission '{token}' to be declared in manifest permissions[]"
                );
            }
            self.validate_popup_defs()?;
            self.validate_tool_popup_refs()?;
        }
        Ok(())
    }

    fn validate_popup_defs(&self) -> anyhow::Result<()> {
        let mut seen_popup_ids = HashSet::new();
        for popup in &self.contributes.popup {
            if !is_valid_tool_id(&popup.id) {
                anyhow::bail!(
                    "invalid contributes.popup id '{}': must be lowercase ascii + digits + '-', \
                         start with a letter, length ≤ 64",
                    popup.id
                );
            }
            if !seen_popup_ids.insert(popup.id.clone()) {
                anyhow::bail!(
                    "contributes.popup id '{}' declared twice in this manifest",
                    popup.id
                );
            }
            if let PopupTrigger::Event { event_key } = &popup.trigger
                && !is_valid_event_key(event_key)
            {
                anyhow::bail!(
                    "contributes.popup '{}': trigger.event_key '{}' must be a concrete event key",
                    popup.id,
                    event_key
                );
            }
            if let Some(sz) = &popup.size_hint
                && (sz.width == 0 || sz.height == 0)
            {
                anyhow::bail!(
                    "contributes.popup '{}': size_hint width/height must be > 0",
                    popup.id
                );
            }
        }
        Ok(())
    }

    fn validate_tool_popup_refs(&self) -> anyhow::Result<()> {
        // [[contributes.tool]]/[[contributes.commands]] action.open_popup이 이 plugin의
        // popup id를 가리킬 때 해당 id가 실제로 존재해야 한다.
        let popup_ids: HashSet<&str> = self
            .contributes
            .popup
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        for tool in &self.contributes.tool {
            Self::validate_popup_ref(
                "contributes.tool",
                &tool.id,
                &tool.action,
                &self.id,
                &popup_ids,
            )?;
        }
        for cmd in &self.contributes.commands {
            if let Some(action) = &cmd.action {
                Self::validate_popup_ref(
                    "contributes.commands",
                    &cmd.id,
                    action,
                    &self.id,
                    &popup_ids,
                )?;
            }
        }
        Ok(())
    }

    fn validate_popup_ref(
        owner_label: &str,
        id: &str,
        action: &ToolAction,
        plugin_id: &str,
        popup_ids: &HashSet<&str>,
    ) -> anyhow::Result<()> {
        if let ToolAction::OpenPopup { popup_id } = action
            && let Some(local_id) = popup_id.strip_prefix(&format!("{plugin_id}/"))
            && !popup_ids.contains(local_id)
        {
            anyhow::bail!(
                "{} '{}': action.popup_id '{}' references unknown popup in this plugin",
                owner_label,
                id,
                popup_id
            );
        }
        Ok(())
    }

    fn validate_contributed_banners(&self) -> anyhow::Result<()> {
        if !self.contributes.banner.is_empty() {
            let token = ContributesGate::Banner.required("");
            if !self.permissions.iter().any(|p| p == &token) {
                anyhow::bail!(
                    "[[contributes.banner]] requires permission '{token}' to be declared in manifest permissions[]"
                );
            }
            let mut seen_banner_ids = HashSet::new();
            for banner in &self.contributes.banner {
                if !is_valid_tool_id(&banner.id) {
                    anyhow::bail!(
                        "invalid contributes.banner id '{}': must be lowercase ascii + digits + '-', \
                         start with a letter, length ≤ 64",
                        banner.id
                    );
                }
                if !seen_banner_ids.insert(banner.id.clone()) {
                    anyhow::bail!(
                        "contributes.banner id '{}' declared twice in this manifest",
                        banner.id
                    );
                }
                if let Some(sz) = &banner.size_hint
                    && sz.height == 0
                {
                    anyhow::bail!(
                        "contributes.banner '{}': size_hint height must be > 0",
                        banner.id
                    );
                }
                if let Some(ttl) = banner.ttl_seconds
                    && ttl == 0
                {
                    anyhow::bail!(
                        "contributes.banner '{}': ttl_seconds must be > 0 (omit for a persistent banner)",
                        banner.id
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_contributed_windows(&self) -> anyhow::Result<()> {
        if !self.contributes.window.is_empty() {
            let token = ContributesGate::Window.required("");
            if !self.permissions.iter().any(|p| p == &token) {
                anyhow::bail!(
                    "[[contributes.window]] requires permission '{token}' to be declared in manifest permissions[]"
                );
            }
            let mut seen_window_ids = HashSet::new();
            for w in &self.contributes.window {
                if !is_valid_kind(&w.id) {
                    anyhow::bail!(
                        "invalid contributes.window id '{}': must be lowercase ascii + '_' + digits",
                        w.id
                    );
                }
                if !seen_window_ids.insert(w.id.clone()) {
                    anyhow::bail!(
                        "contributes.window id '{}' declared twice in this manifest",
                        w.id
                    );
                }
                if w.display_name_i18n_key.is_empty() {
                    anyhow::bail!(
                        "contributes.window '{}': display_name_i18n_key must not be empty",
                        w.id
                    );
                }
                if let Some(sz) = &w.default_size
                    && (sz.width == 0 || sz.height == 0)
                {
                    anyhow::bail!(
                        "contributes.window '{}': default_size width/height must be > 0",
                        w.id
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_contributed_settings_pages(&self) -> anyhow::Result<()> {
        if !self.contributes.settings_pages.is_empty() {
            let token = ContributesGate::SettingsPage.required("");
            if !self.permissions.iter().any(|p| p == &token) {
                anyhow::bail!("contributes.settings_pages requires the '{token}' permission");
            }
            let mut seen_page_ids = HashSet::new();
            for page in &self.contributes.settings_pages {
                if !is_valid_settings_id(&page.id) {
                    anyhow::bail!(
                        "invalid contributes.settings_pages id '{}': must be lowercase ascii + digits + '_' + '-', length 1..=64",
                        page.id
                    );
                }
                if !seen_page_ids.insert(page.id.clone()) {
                    anyhow::bail!(
                        "contributes.settings_pages id '{}' declared twice in this manifest",
                        page.id
                    );
                }
                if page.title_key.is_empty() {
                    anyhow::bail!(
                        "contributes.settings_pages '{}': title_key must not be empty",
                        page.id
                    );
                }
                Self::validate_settings_page_items(page)?;
            }
        }
        Ok(())
    }

    fn validate_settings_page_items(page: &SettingsPageContribute) -> anyhow::Result<()> {
        let mut seen_item_ids = HashSet::new();
        for item in &page.items {
            // 모든 항목의 ID·저장 키 형식, 라벨, 중복을 검사한다.
            let (id, label_key, storage_key) = item.common();
            if !is_valid_settings_id(id) {
                anyhow::bail!(
                    "invalid contributes.settings_pages '{}' item id '{}': must be lowercase ascii + digits + '_' + '-', length 1..=64",
                    page.id,
                    id
                );
            }
            if !seen_item_ids.insert(id.to_string()) {
                anyhow::bail!(
                    "contributes.settings_pages '{}' item id '{}' declared twice",
                    page.id,
                    id
                );
            }
            if label_key.is_empty() {
                anyhow::bail!(
                    "contributes.settings_pages '{}' item '{}': label_key must not be empty",
                    page.id,
                    id
                );
            }
            if !is_valid_settings_id(storage_key) {
                anyhow::bail!(
                    "invalid contributes.settings_pages '{}' item '{}' storage_key '{}': must be lowercase ascii + digits + '_' + '-', length 1..=64",
                    page.id,
                    id,
                    storage_key
                );
            }
            Self::validate_settings_item_variant(page, id, item)?;
        }
        Ok(())
    }

    fn validate_settings_item_variant(
        page: &SettingsPageContribute,
        id: &str,
        item: &SettingsItemDecl,
    ) -> anyhow::Result<()> {
        match item {
            SettingsItemDecl::FontOverride { .. } | SettingsItemDecl::Toggle { .. } => {}
            SettingsItemDecl::Select {
                options, default, ..
            } => {
                if !options.iter().any(|o| &o.value == default) {
                    anyhow::bail!(
                        "contributes.settings_pages '{}' item '{}': select default '{}' is not among options",
                        page.id,
                        id,
                        default
                    );
                }
            }
            SettingsItemDecl::Number {
                default, min, max, ..
            } => {
                if let (Some(mn), Some(mx)) = (min, max)
                    && mn > mx
                {
                    anyhow::bail!(
                        "contributes.settings_pages '{}' item '{}': number min ({}) > max ({})",
                        page.id,
                        id,
                        mn,
                        mx
                    );
                }
                if let Some(mn) = min
                    && default < mn
                {
                    anyhow::bail!(
                        "contributes.settings_pages '{}' item '{}': number default ({}) < min ({})",
                        page.id,
                        id,
                        default,
                        mn
                    );
                }
                if let Some(mx) = max
                    && default > mx
                {
                    anyhow::bail!(
                        "contributes.settings_pages '{}' item '{}': number default ({}) > max ({})",
                        page.id,
                        id,
                        default,
                        mx
                    );
                }
            }
        }
        Ok(())
    }

    fn validate_contributed_detectors(&self) -> anyhow::Result<()> {
        // 감지기 본문의 구체 규칙은 호스트의 manifest_validate에서 검사한다.
        let mut seen_detector_ids = HashSet::new();
        for v in &self.contributes.detector {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if id.is_empty() {
                anyhow::bail!("contributes.detector entry missing required 'id' string field");
            }
            if !is_valid_simple_id(id) {
                anyhow::bail!(
                    "invalid contributes.detector id '{id}': must be lowercase ascii + digits + '-', length ≤ 64"
                );
            }
            if id.starts_with('$') {
                anyhow::bail!(
                    "contributes.detector '{id}': plugin cannot define reserved ($-prefixed) detector ids"
                );
            }
            if !seen_detector_ids.insert(id.to_string()) {
                anyhow::bail!("contributes.detector id '{id}' declared twice in this manifest");
            }
        }
        Ok(())
    }

    fn validate_contributed_handlers(&self) -> anyhow::Result<()> {
        // 액션과 감지기 참조의 구체 검사는 호스트의 설치 단계에서 한다.
        let mut seen_handler_ids = HashSet::new();
        for v in &self.contributes.handler {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if id.is_empty() {
                anyhow::bail!("contributes.handler entry missing required 'id' string field");
            }
            if !seen_handler_ids.insert(id.to_string()) {
                anyhow::bail!("contributes.handler id '{id}' declared twice in this manifest");
            }
            let detector = v.get("detector").and_then(|x| x.as_str()).unwrap_or("");
            if detector.is_empty() {
                anyhow::bail!(
                    "contributes.handler '{id}' missing required 'detector' string field"
                );
            }
            let needs = ContributesGate::Handler.required(detector);
            if !self.permissions.iter().any(|p| p == &needs) {
                anyhow::bail!(
                    "contributes.handler '{id}' on detector '{detector}' requires permission '{needs}'"
                );
            }
        }
        Ok(())
    }

    fn validate_contributed_detector_permissions(&self) -> anyhow::Result<()> {
        // 기존 감지기 목록을 모르므로 정의 권한 또는 해당 ID 확장 권한 중 하나를 요구한다.
        // 호스트는 설치 때 신규·기존 여부를 구분해 다시 검사한다.
        let define = ContributesGate::DetectorDefine.required("");
        for v in &self.contributes.detector {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let has_define = self.permissions.iter().any(|p| p == &define);
            let needs_extend = ContributesGate::DetectorExtend.required(id);
            let has_extend = self.permissions.iter().any(|p| p == &needs_extend);
            if !has_define && !has_extend {
                anyhow::bail!(
                    "contributes.detector '{id}' requires either 'file_handler.define' (new id) \
                     or '{needs_extend}' (extending existing id)"
                );
            }
        }

        Ok(())
    }

    fn validate_contributed_hook_handlers(&self) -> anyhow::Result<()> {
        // ID·중복·선언 권한만 검사한다. 액션 본문은 호스트가 설치 때 해석·검증한다.
        let hook_define = ContributesGate::HookHandler.required("");
        let has_define = self.permissions.iter().any(|p| p == &hook_define);
        let mut seen_ids = HashSet::new();
        for v in &self.contributes.hook_handler {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if id.is_empty() {
                anyhow::bail!("contributes.hook_handler entry missing required 'id' string field");
            }
            if !is_valid_hook_handler_id(id) {
                anyhow::bail!(
                    "invalid contributes.hook_handler id '{id}': must be lowercase ascii + digits + '-', length ≤ 32"
                );
            }
            if !seen_ids.insert(id.to_string()) {
                anyhow::bail!("contributes.hook_handler id '{id}' declared twice in this manifest");
            }
            if !has_define {
                anyhow::bail!(
                    "contributes.hook_handler '{id}' requires permission '{hook_define}'"
                );
            }
        }
        Ok(())
    }

    fn validate_contributed_completion_strategies(
        &self,
        seen_prefixes: &HashSet<String>,
    ) -> anyhow::Result<()> {
        // ID·중복·선언 권한과 메서드 namespace를 검사한다. 구체 전략은 호스트가 검사한다.
        // namespace는 플러그인 ID가 아니라 ipc_namespace에 선언한 접두어다.
        let strategy_define = ContributesGate::CompletionStrategy.required("");
        let has_define = self.permissions.iter().any(|p| p == &strategy_define);
        let mut seen_ids = HashSet::new();
        for v in &self.contributes.completion_strategy {
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if id.is_empty() {
                anyhow::bail!(
                    "contributes.completion_strategy entry missing required 'id' string field"
                );
            }
            if !is_valid_completion_strategy_id(id) {
                anyhow::bail!(
                    "invalid contributes.completion_strategy id '{id}': must be lowercase ascii + digits + '-', length ≤ 32"
                );
            }
            if !seen_ids.insert(id.to_string()) {
                anyhow::bail!(
                    "contributes.completion_strategy id '{id}' declared twice in this manifest"
                );
            }
            if !has_define {
                anyhow::bail!(
                    "contributes.completion_strategy '{id}' requires permission '{strategy_define}'"
                );
            }
            // poll 메서드는 이 플러그인이 선언한 namespace에 속해야 한다.
            if v.get("spec")
                .and_then(|s| s.get("kind"))
                .and_then(|k| k.as_str())
                == Some("poll")
                && let Some(poll_method) = v
                    .get("spec")
                    .and_then(|s| s.get("poll_method"))
                    .and_then(|m| m.as_str())
            {
                let prefix = poll_method.split('.').next().unwrap_or("");
                if !seen_prefixes.contains(prefix) {
                    anyhow::bail!(
                        "contributes.completion_strategy '{id}' poll_method '{poll_method}' \
                         uses namespace '{prefix}' which is not declared in this plugin's \
                         ipc_namespace"
                    );
                }
            }
            // 기본 적용 메서드도 같은 namespace 제한을 따른다.
            if let Some(methods) = v.get("default_for_methods").and_then(|m| m.as_array()) {
                for m in methods {
                    let Some(method) = m.as_str() else {
                        anyhow::bail!(
                            "contributes.completion_strategy '{id}' default_for_methods entries must be strings"
                        );
                    };
                    let prefix = method.split('.').next().unwrap_or("");
                    if !seen_prefixes.contains(prefix) {
                        anyhow::bail!(
                            "contributes.completion_strategy '{id}' default_for_methods '{method}' \
                             uses namespace '{prefix}' which is not declared in this plugin's \
                             ipc_namespace"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// 권한 문자열을 집합으로 변환한다. 알 수 없는 토큰이 있으면 오류다.
    pub fn parsed_permissions(&self) -> anyhow::Result<HashSet<Permission>> {
        let mut out = HashSet::with_capacity(self.permissions.len());
        for raw in &self.permissions {
            match Permission::from_token(raw) {
                Some(p) => {
                    out.insert(p);
                }
                None => anyhow::bail!("unknown permission '{}'", raw),
            }
        }
        Ok(out)
    }
}

fn validate_hook_timeout(timeout_ms: u32, target: &str) -> anyhow::Result<()> {
    if timeout_ms == 0 {
        anyhow::bail!("extends hook for '{target}': timeout_ms must be > 0");
    }
    if timeout_ms > HOOK_TIMEOUT_MS_MAX {
        anyhow::bail!(
            "extends hook for '{target}': timeout_ms {timeout_ms} exceeds maximum {HOOK_TIMEOUT_MS_MAX}"
        );
    }
    Ok(())
}

fn validate_hook_mode_modifies(
    mode: HookMode,
    modifies: &[String],
    target: &str,
) -> anyhow::Result<()> {
    match mode {
        HookMode::Transform => {
            if modifies.is_empty() {
                anyhow::bail!(
                    "extends hook for '{target}': mode=transform requires non-empty 'modifies'"
                );
            }
        }
        HookMode::Filter | HookMode::Observe => {
            // filter와 observe는 값을 바꾸지 않으므로 modifies는 경고 후 무시한다.
            if !modifies.is_empty() {
                tracing::warn!(
                    "extends hook for '{target}': mode={:?} ignores 'modifies' field",
                    mode
                );
            }
        }
    }
    Ok(())
}
