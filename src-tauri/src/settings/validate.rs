use super::schema::Settings;
use super::FieldError;
use crate::hotkeys;

const INTERFACE_LANGUAGES: &[&str] = &["en"];

pub fn validate(s: &Settings) -> Vec<FieldError> {
    let mut errors = Vec::new();
    let mut range = |path: &str, v: f64, min: f64, max: f64| {
        if !(min..=max).contains(&v) {
            errors.push(FieldError::new(
                path,
                format!("Must be between {min} and {max}"),
            ));
        }
    };

    let b = &s.buddy;
    range("buddy.size", b.size as f64, 16.0, 128.0);
    range("buddy.opacity", b.opacity, 0.2, 1.0);
    range("buddy.offsetX", b.offset_x as f64, -200.0, 200.0);
    range("buddy.offsetY", b.offset_y as f64, -200.0, 200.0);
    range("buddy.smoothness", b.smoothness, 0.0, 0.95);
    range("buddy.idleSeconds", b.idle_seconds as f64, 2.0, 3600.0);

    let g = &s.guidance;
    range(
        "guidance.highlightThickness",
        g.highlight_thickness as f64,
        1.0,
        8.0,
    );
    range("guidance.dim", g.dim, 0.0, 0.7);
    range(
        "guidance.annotationSeconds",
        g.annotation_seconds as f64,
        0.0,
        600.0,
    );
    range("guidance.animationSpeed", g.animation_speed, 0.5, 2.0);
    range("guidance.maxSteps", g.max_steps as f64, 1.0, 30.0);
    if !is_hex_color(&g.highlight_color) {
        errors.push(FieldError::new(
            "guidance.highlightColor",
            "Use a colour like #e5484d",
        ));
    }

    let a = &s.agents;
    check_range(
        &mut errors,
        "agents.maxRunning",
        a.max_running as f64,
        1.0,
        10.0,
    );
    check_range(
        &mut errors,
        "agents.backupDays",
        a.backup_days as f64,
        1.0,
        365.0,
    );
    check_range(
        &mut errors,
        "agents.timeLimitMinutes",
        a.time_limit_minutes as f64,
        1.0,
        1440.0,
    );
    check_range(
        &mut errors,
        "agents.agentTokenBudget",
        a.agent_token_budget as f64,
        0.0,
        1e9,
    );
    check_range(
        &mut errors,
        "agents.batchTokenBudget",
        a.batch_token_budget as f64,
        0.0,
        1e9,
    );
    check_range(
        &mut errors,
        "agents.maxSteps",
        a.max_steps as f64,
        1.0,
        500.0,
    );
    check_range(
        &mut errors,
        "agents.maxToolCalls",
        a.max_tool_calls as f64,
        1.0,
        1000.0,
    );
    check_range(
        &mut errors,
        "agents.repeatThreshold",
        a.repeat_threshold as f64,
        2.0,
        20.0,
    );
    check_range(
        &mut errors,
        "agents.noProgressSteps",
        a.no_progress_steps as f64,
        2.0,
        100.0,
    );
    check_range(
        &mut errors,
        "agents.contextTokens",
        a.context_tokens as f64,
        8000.0,
        1_000_000.0,
    );
    check_range(
        &mut errors,
        "agents.doneSeconds",
        a.done_seconds as f64,
        0.0,
        3600.0,
    );
    check_range(
        &mut errors,
        "agents.historyDays",
        a.history_days as f64,
        1.0,
        365.0,
    );
    for (path, v) in [
        ("agents.agentCostBudget", a.agent_cost_budget),
        ("agents.batchCostBudget", a.batch_cost_budget),
    ] {
        if v.is_some_and(|v| !(v > 0.0 && v <= 10_000.0)) {
            errors.push(FieldError::new(
                path,
                "Must be more than $0 and at most $10,000",
            ));
        }
    }
    for f in a
        .approved_folders
        .iter()
        .chain(Some(&a.projects_folder).filter(|p| !p.is_empty()))
    {
        if !std::path::Path::new(f).is_absolute() {
            let path = if f == &a.projects_folder {
                "agents.projectsFolder"
            } else {
                "agents.approvedFolders"
            };
            errors.push(FieldError::new(
                path,
                format!("\"{f}\" isn't a full folder path"),
            ));
        }
    }
    if !a.searxng_url.is_empty() && !is_http_url(&a.searxng_url) {
        errors.push(FieldError::new(
            "agents.searxngUrl",
            "Use an address starting with http:// or https://",
        ));
    }

    let builtin: Vec<String> = crate::agents::templates::builtins()
        .into_iter()
        .map(|t| t.id)
        .collect();
    let mut ids: Vec<&str> = Vec::new();
    for t in &a.templates {
        if let Err(e) = crate::agents::templates::check(t) {
            errors.push(FieldError::new("agents.templates", e));
        } else if ids.contains(&t.id.as_str()) || builtin.contains(&t.id) {
            errors.push(FieldError::new(
                "agents.templates",
                format!("{}: two templates have the same id", t.name),
            ));
        }
        ids.push(&t.id);
    }

    let mut trigger_ids: Vec<&str> = Vec::new();
    let templates: Vec<String> = crate::agents::templates::all(s)
        .into_iter()
        .map(|t| t.id)
        .collect();
    for t in &a.triggers {
        let name = if t.name.trim().is_empty() {
            "A trigger"
        } else {
            t.name.as_str()
        };
        let problem = if t.id.is_empty() || trigger_ids.contains(&t.id.as_str()) {
            Some("every trigger needs its own id".to_string())
        } else if t.name.trim().is_empty() {
            Some("give it a name".to_string())
        } else if !t.template.is_empty() && !templates.contains(&t.template) {
            Some("its template doesn't exist".to_string())
        } else if t.template.is_empty() && t.goal.trim().is_empty() {
            Some("say what it should do".to_string())
        } else {
            match &t.when {
                crate::settings::schema::TriggerWhen::Schedule { cron } => {
                    crate::agents::triggers::check_schedule(cron).err()
                }
                crate::settings::schema::TriggerWhen::Folder { path, .. } => {
                    (!std::path::Path::new(path).is_absolute())
                        .then(|| format!("\"{path}\" isn't a full folder path"))
                }
            }
        };
        if let Some(p) = problem {
            errors.push(FieldError::new("agents.triggers", format!("{name}: {p}")));
        }
        trigger_ids.push(&t.id);
    }

    validate_mcp(&s.connectors.mcp, &mut errors);

    if s.circle.translate_to == "auto" || !is_language_tag(&s.circle.translate_to) {
        errors.push(FieldError::new(
            "circle.translateTo",
            "Use a language code such as en, de or pt-BR",
        ));
    }

    if !INTERFACE_LANGUAGES.contains(&s.general.interface_language.as_str()) {
        errors.push(FieldError::new(
            "general.interfaceLanguage",
            "Only English is available right now",
        ));
    }
    if !is_language_tag(&s.general.response_language) {
        errors.push(FieldError::new(
            "general.responseLanguage",
            "Use \"auto\" or a language code such as en, de or pt-BR",
        ));
    }

    let mut seen: Vec<(&str, u32)> = Vec::new();
    for (key, label, accel) in s.hotkeys.actions() {
        if accel.is_empty() {
            continue;
        }
        let path = format!("hotkeys.{key}");
        match hotkeys::parse(accel) {
            Err(message) => errors.push(FieldError::new(path, message)),
            Ok(shortcut) => {
                if let Some((other, _)) = seen.iter().find(|(_, id)| *id == shortcut.id()) {
                    errors.push(FieldError::new(path, format!("Already used by {other}")));
                } else {
                    seen.push((label, shortcut.id()));
                }
            }
        }
    }

    validate_ai(s, &mut errors);
    errors
}

fn check_range(errors: &mut Vec<FieldError>, path: &str, v: f64, min: f64, max: f64) {
    if !(min..=max).contains(&v) {
        errors.push(FieldError::new(
            path,
            format!("Must be between {min} and {max}"),
        ));
    }
}

fn validate_ai(s: &Settings, errors: &mut Vec<FieldError>) {
    let ai = &s.ai;
    let mut ids: Vec<&str> = Vec::new();
    for p in &ai.providers {
        let path = "ai.providers";
        if p.id.trim().is_empty() {
            errors.push(FieldError::new(path, "Every provider needs an id"));
        } else if ids.contains(&p.id.as_str()) {
            errors.push(FieldError::new(
                path,
                format!("Two providers share the id \"{}\"", p.id),
            ));
        }
        ids.push(&p.id);
        if p.name.trim().is_empty() {
            errors.push(FieldError::new(path, "Every provider needs a name"));
        }
        if !is_http_url(&p.base_url) {
            errors.push(FieldError::new(
                path,
                format!(
                    "{}: the address must start with http:// or https://",
                    display_name(p)
                ),
            ));
        }
        let mut models: Vec<&str> = Vec::new();
        for m in &p.models {
            if m.id.trim().is_empty() {
                errors.push(FieldError::new(
                    path,
                    format!("{}: a model has no name", display_name(p)),
                ));
            } else if models.contains(&m.id.as_str()) {
                errors.push(FieldError::new(
                    path,
                    format!("{}: {} is listed twice", display_name(p), m.id),
                ));
            }
            models.push(&m.id);
            for price in [m.input_price, m.output_price].into_iter().flatten() {
                if !(0.0..=10_000.0).contains(&price) {
                    errors.push(FieldError::new(
                        path,
                        format!("{}: prices must be between 0 and 10000", m.id),
                    ));
                }
            }
        }
    }

    for (key, r) in ai.routing.entries() {
        if let Some(r) = r {
            if ai.model(r).is_none() {
                errors.push(FieldError::new(
                    "ai.routing",
                    format!("{key}: {} isn't set up", describe(r)),
                ));
            }
        }
    }
    for r in &ai.fallback_chain {
        if ai.model(r).is_none() {
            errors.push(FieldError::new(
                "ai.fallbackChain",
                format!("{} isn't set up", describe(r)),
            ));
        }
    }

    check_range(errors, "ai.temperature", ai.temperature, 0.0, 2.0);
    check_range(
        errors,
        "ai.maxResponseTokens",
        ai.max_response_tokens as f64,
        256.0,
        128_000.0,
    );
    check_range(errors, "ai.timeoutSecs", ai.timeout_secs as f64, 5.0, 600.0);
    if ai.custom_instructions.len() > 4000 {
        errors.push(FieldError::new(
            "ai.customInstructions",
            "Keep it under 4000 characters",
        ));
    }

    let vi = &s.voice_input;
    check_range(
        errors,
        "voiceInput.silenceSeconds",
        vi.silence_seconds,
        0.0,
        10.0,
    );
    if vi.language != "auto" && !is_language_tag(&vi.language) {
        errors.push(FieldError::new(
            "voiceInput.language",
            "Use \"auto\" or a language code such as en or de",
        ));
    }
    if vi.wake_word && !(2..=40).contains(&vi.wake_phrase.trim().chars().count()) {
        errors.push(FieldError::new(
            "voiceInput.wakePhrase",
            "Use a short phrase of 2 to 40 characters",
        ));
    }
    if vi.whisper_model.trim().is_empty() {
        errors.push(FieldError::new("voiceInput.whisperModel", "Choose a model"));
    }
    let vo = &s.voice_output;
    check_range(errors, "voiceOutput.speed", vo.speed, 0.5, 2.0);
    check_range(errors, "voiceOutput.volume", vo.volume, 0.0, 1.0);
    // Cloud speech borrows the key of an OpenAI-style provider.
    for (path, id) in [
        ("voiceInput.openaiProviderId", &vi.openai_provider_id),
        ("voiceOutput.openaiProviderId", &vo.openai_provider_id),
    ] {
        if let Some(id) = id {
            match ai.provider(id) {
                None => errors.push(FieldError::new(path, "That provider isn't set up any more")),
                Some(p)
                    if !matches!(
                        p.kind,
                        super::schema::ProviderKind::OpenAi
                            | super::schema::ProviderKind::OpenAiCompatible
                    ) =>
                {
                    errors.push(FieldError::new(
                        path,
                        "Pick an OpenAI or OpenAI-compatible provider",
                    ))
                }
                Some(_) => {}
            }
        }
    }

    let l = &s.limits;
    check_range(errors, "limits.maxRetries", l.max_retries as f64, 0.0, 10.0);
    check_range(
        errors,
        "limits.backoffBaseMs",
        l.backoff_base_ms as f64,
        100.0,
        60_000.0,
    );
    check_range(
        errors,
        "limits.backoffMaxMs",
        l.backoff_max_ms as f64,
        100.0,
        600_000.0,
    );
    if l.backoff_max_ms < l.backoff_base_ms {
        errors.push(FieldError::new(
            "limits.backoffMaxMs",
            "Must be at least the base delay",
        ));
    }
    if let Some(c) = l.daily_cost_budget {
        check_range(errors, "limits.dailyCostBudget", c, 0.0, 100_000.0);
    }
}

fn display_name(p: &super::schema::ProviderConfig) -> &str {
    if p.name.trim().is_empty() {
        &p.id
    } else {
        &p.name
    }
}

fn describe(r: &super::schema::ModelRef) -> String {
    format!("{} on {}", r.model, r.provider_id)
}

fn is_http_url(s: &str) -> bool {
    reqwest::Url::parse(s)
        .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some())
}

fn validate_mcp(servers: &[crate::settings::schema::McpServer], errors: &mut Vec<FieldError>) {
    use crate::settings::schema::McpTransport;
    let path = "connectors.mcp";
    let mut ids: Vec<&str> = Vec::new();
    for m in servers {
        let name = if m.name.trim().is_empty() {
            m.id.as_str()
        } else {
            m.name.as_str()
        };
        if m.id.trim().is_empty() || ids.contains(&m.id.as_str()) {
            errors.push(FieldError::new(
                path,
                format!("{name}: every MCP server needs its own id"),
            ));
        }
        ids.push(&m.id);
        if m.name.trim().is_empty() {
            errors.push(FieldError::new(path, "Every MCP server needs a name"));
        }
        // A server that's off may be half filled in.
        match m.transport {
            McpTransport::Stdio if m.enabled && m.command.trim().is_empty() => {
                errors.push(FieldError::new(
                    path,
                    format!("{name}: give the command that starts it"),
                ));
            }
            McpTransport::Http if m.enabled && !is_http_url(&m.url) => {
                errors.push(FieldError::new(
                    path,
                    format!("{name}: the address must start with http:// or https://"),
                ));
            }
            _ => {}
        }
        let token = |k: &str| {
            !k.is_empty()
                && k.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_!#$%&'*+.^`|~".contains(&b))
        };
        // Rows without a name yet are skipped when the server starts.
        for h in m.headers.iter().filter(|h| !h.key.is_empty()) {
            if !token(&h.key) {
                errors.push(FieldError::new(
                    path,
                    format!("{name}: \"{}\" isn't a valid header name", h.key),
                ));
            }
        }
        for e in &m.env {
            if e.key.trim() != e.key || e.key.contains('=') {
                errors.push(FieldError::new(
                    path,
                    format!("{name}: \"{}\" isn't a valid variable name", e.key),
                ));
            }
        }
    }
}

fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// "auto", or a simple BCP 47 tag: 2-3 letter language, optional region/script.
fn is_language_tag(tag: &str) -> bool {
    if tag == "auto" {
        return true;
    }
    let mut parts = tag.split('-');
    let lang_ok = parts
        .next()
        .is_some_and(|l| (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_lowercase()));
    lang_ok
        && parts.all(|p| (2..=4).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert_eq!(validate(&Settings::default()), vec![]);
    }

    #[test]
    fn out_of_range_values_are_reported_by_path() {
        let mut s = Settings::default();
        s.buddy.size = 4;
        s.buddy.opacity = 0.0;
        let paths: Vec<_> = validate(&s).into_iter().map(|e| e.path).collect();
        assert_eq!(paths, ["buddy.size", "buddy.opacity"]);
    }

    #[test]
    fn duplicate_hotkeys_name_the_other_action() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = s.hotkeys.voice_ask.clone();
        let e = validate(&s);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "hotkeys.textAsk");
        assert_eq!(e[0].message, "Already used by Voice ask");
    }

    #[test]
    fn duplicate_detection_ignores_modifier_order_and_case() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = "shift+alt+space".into();
        assert_eq!(validate(&s)[0].path, "hotkeys.textAsk");
    }

    #[test]
    fn unparseable_hotkey_is_an_error_and_empty_is_unbound() {
        let mut s = Settings::default();
        s.hotkeys.text_ask = "Alt+Banana".into();
        s.hotkeys.circle_to_explain = String::new();
        let e = validate(&s);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].path, "hotkeys.textAsk");
    }

    fn provider(id: &str) -> super::super::schema::ProviderConfig {
        use super::super::schema::*;
        ProviderConfig {
            id: id.into(),
            kind: ProviderKind::Ollama,
            name: "Local".into(),
            base_url: "http://localhost:11434/v1".into(),
            models: vec![ModelConfig {
                id: "llama3.2".into(),
                ..Default::default()
            }],
        }
    }

    #[test]
    fn routing_must_point_at_a_configured_model() {
        use super::super::schema::ModelRef;
        let mut s = Settings::default();
        s.ai.providers.push(provider("local"));
        s.ai.routing.ask = Some(ModelRef {
            provider_id: "local".into(),
            model: "llama3.2".into(),
        });
        assert_eq!(validate(&s), vec![]);
        s.ai.routing.ask = Some(ModelRef {
            provider_id: "local".into(),
            model: "gone".into(),
        });
        assert_eq!(validate(&s)[0].path, "ai.routing");
        s.ai.routing.ask = None;
        s.ai.fallback_chain.push(ModelRef {
            provider_id: "nope".into(),
            model: "x".into(),
        });
        assert_eq!(validate(&s)[0].path, "ai.fallbackChain");
    }

    #[test]
    fn provider_urls_ids_and_models_are_checked() {
        let mut s = Settings::default();
        let mut p = provider("a");
        p.base_url = "localhost:11434".into();
        s.ai.providers.push(p);
        s.ai.providers.push(provider("a"));
        let dup = s.ai.providers[1].models[0].clone();
        s.ai.providers[1].models.push(dup);
        let messages: Vec<_> = validate(&s).into_iter().map(|e| e.message).collect();
        assert!(
            messages.iter().any(|m| m.contains("http://")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("share the id")),
            "{messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("listed twice")),
            "{messages:?}"
        );
    }

    #[test]
    fn backoff_max_cannot_be_below_base() {
        let mut s = Settings::default();
        s.limits.backoff_max_ms = 1000;
        assert_eq!(validate(&s)[0].path, "limits.backoffMaxMs");
    }

    #[test]
    fn guidance_colour_and_ranges() {
        let mut s = Settings::default();
        s.guidance.highlight_color = "red".into();
        s.guidance.max_steps = 0;
        s.guidance.dim = 0.9;
        let paths: Vec<_> = validate(&s).into_iter().map(|e| e.path).collect();
        assert_eq!(
            paths,
            [
                "guidance.dim",
                "guidance.maxSteps",
                "guidance.highlightColor"
            ]
        );
        assert!(is_hex_color("#0aF3c9") && !is_hex_color("#abc"));
    }

    #[test]
    fn translation_needs_a_real_language() {
        let mut s = Settings::default();
        for bad in ["auto", "German", ""] {
            s.circle.translate_to = bad.into();
            assert_eq!(validate(&s)[0].path, "circle.translateTo", "{bad}");
        }
        s.circle.translate_to = "pt-BR".into();
        assert_eq!(validate(&s), vec![]);
    }

    #[test]
    fn language_tags() {
        for ok in ["auto", "en", "de", "pt-BR", "zh-Hant"] {
            assert!(is_language_tag(ok), "{ok}");
        }
        for bad in ["", "English", "EN", "e", "en_US", "en-"] {
            assert!(!is_language_tag(bad), "{bad}");
        }
    }
}
