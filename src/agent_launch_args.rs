//! Remembering the options an agent CLI was started with.
//!
//! Herdr resumes a native agent session by running the agent again with its own
//! session reference. The rest of the original command line matters too: a pane
//! started with `claude --permission-mode bypassPermissions` must come back with
//! that permission mode, not with plain `claude --resume <id>`.
//!
//! The captured argument list comes from the live agent process, so it can also
//! contain the session reference Herdr appended on a previous resume, the prompt
//! the user typed at launch, or a subcommand. Replaying those would either point
//! the agent at a stale conversation or restart work the user never asked for,
//! so this module decides which arguments survive a resume:
//!
//! - Session-selection, continuation, and non-interactive options are dropped.
//! - Bare tokens are dropped. They are prompts, subcommands, or positional
//!   paths, none of which belong in a resume command.
//! - Known options keep their values, so `--model opus` survives intact.
//! - An unlisted option followed by a bare token is dropped together with that
//!   token. Herdr cannot tell a value from a prompt there, and replaying the
//!   option without its value would break the resume command.
//!
//! Every outcome is therefore either "the option came back" or "the option was
//! forgotten". An unlisted option never turns into a broken resume command, and
//! adding an option to [`option_rules`] is all it takes to remember it.

/// How many separate values an agent CLI option consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionValues {
    /// Boolean flag.
    None,
    /// Exactly one value.
    One,
    /// One value when the next token is not another option.
    Optional,
    /// Every following token up to the next option.
    Many,
}

/// Per-agent option knowledge used to rebuild a resume command line.
struct AgentOptionRules {
    /// Options that select or continue a conversation, or switch the agent to a
    /// non-interactive run. Herdr supplies its own session reference, so these
    /// are never replayed.
    dropped: &'static [(&'static str, OptionValues)],
    /// Options Herdr can replay, with the number of values each one consumes.
    /// [`OptionValues::None`] entries are boolean flags that must survive even
    /// when a prompt follows them on the command line.
    known: &'static [(&'static str, OptionValues)],
}

/// Options every agent shares: `--help` and `--version` never belong in a
/// resume command, whatever the agent does with them.
const COMMON_DROPPED: &[(&str, OptionValues)] = &[
    ("-h", OptionValues::None),
    ("--help", OptionValues::None),
    ("-V", OptionValues::None),
    ("--version", OptionValues::None),
];

/// Return the arguments worth replaying when resuming `agent`.
///
/// `args` is the agent process command line with the executable removed.
pub fn replayable(agent: &str, args: &[String]) -> Vec<String> {
    let rules = option_rules(agent);
    let mut kept: Vec<String> = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            // Everything after the separator is positional.
            break;
        }
        let Some(name) = option_name(arg) else {
            index += 1;
            continue;
        };
        let inline_value = name.len() < arg.len();

        if let Some(values) = lookup(rules.dropped, name).or_else(|| lookup(COMMON_DROPPED, name)) {
            index += 1 + separate_value_count(args, index + 1, inline_value, values);
            continue;
        }

        match lookup(rules.known, name) {
            Some(values) => {
                let count = separate_value_count(args, index + 1, inline_value, values);
                kept.extend_from_slice(&args[index..index + 1 + count]);
                index += 1 + count;
            }
            None if !inline_value && next_is_bare(args, index + 1) => {
                // Ambiguous: the bare token is either this option's value or a
                // prompt. Dropping both keeps the resume command valid.
                tracing::debug!(
                    agent,
                    option = name,
                    "dropping unrecognized agent option with an ambiguous value from resume command"
                );
                index += 2;
            }
            None => {
                kept.push(args[index].clone());
                index += 1;
            }
        }
    }

    kept
}

fn option_name(arg: &str) -> Option<&str> {
    if !arg.starts_with('-') || arg == "-" || arg == "--" {
        return None;
    }
    Some(arg.split('=').next().unwrap_or(arg))
}

fn lookup(rules: &'static [(&'static str, OptionValues)], name: &str) -> Option<OptionValues> {
    rules
        .iter()
        .find(|(option, _)| *option == name)
        .map(|(_, values)| *values)
}

fn separate_value_count(
    args: &[String],
    start: usize,
    inline_value: bool,
    values: OptionValues,
) -> usize {
    if inline_value {
        return 0;
    }
    match values {
        OptionValues::None => 0,
        OptionValues::One | OptionValues::Optional => usize::from(next_is_bare(args, start)),
        OptionValues::Many => args[start.min(args.len())..]
            .iter()
            .take_while(|arg| is_bare(arg))
            .count(),
    }
}

fn next_is_bare(args: &[String], index: usize) -> bool {
    args.get(index).is_some_and(|arg| is_bare(arg))
}

fn is_bare(arg: &str) -> bool {
    !arg.starts_with('-')
}

fn option_rules(agent: &str) -> AgentOptionRules {
    match agent {
        "claude" => AgentOptionRules {
            dropped: &[
                ("-r", OptionValues::Optional),
                ("--resume", OptionValues::Optional),
                ("-c", OptionValues::None),
                ("--continue", OptionValues::None),
                ("-p", OptionValues::None),
                ("--print", OptionValues::None),
                ("--session-id", OptionValues::One),
                ("--fork-session", OptionValues::None),
                ("--cloud", OptionValues::Optional),
                ("--teleport", OptionValues::Optional),
                ("--from-pr", OptionValues::Optional),
                ("--bg", OptionValues::None),
                ("--background", OptionValues::None),
            ],
            known: &[
                ("--allow-dangerously-skip-permissions", OptionValues::None),
                ("--ax-screen-reader", OptionValues::None),
                ("--bare", OptionValues::None),
                ("--brief", OptionValues::None),
                ("--chrome", OptionValues::None),
                ("--dangerously-skip-permissions", OptionValues::None),
                ("--disable-slash-commands", OptionValues::None),
                (
                    "--exclude-dynamic-system-prompt-sections",
                    OptionValues::None,
                ),
                ("--forward-subagent-text", OptionValues::None),
                ("--ide", OptionValues::None),
                ("--include-hook-events", OptionValues::None),
                ("--include-partial-messages", OptionValues::None),
                ("--no-chrome", OptionValues::None),
                ("--no-session-persistence", OptionValues::None),
                ("--replay-user-messages", OptionValues::None),
                ("--safe-mode", OptionValues::None),
                ("--strict-mcp-config", OptionValues::None),
                ("--tmux", OptionValues::None),
                ("--verbose", OptionValues::None),
                ("--add-dir", OptionValues::Many),
                ("--agent", OptionValues::One),
                ("--agents", OptionValues::One),
                ("--allowed-tools", OptionValues::Many),
                ("--allowedTools", OptionValues::Many),
                ("--append-system-prompt", OptionValues::One),
                ("--autocompact", OptionValues::One),
                ("--betas", OptionValues::Many),
                ("-d", OptionValues::Optional),
                ("--debug", OptionValues::Optional),
                ("--debug-file", OptionValues::One),
                ("--disallowed-tools", OptionValues::Many),
                ("--disallowedTools", OptionValues::Many),
                ("--effort", OptionValues::One),
                ("--environment", OptionValues::One),
                ("--fallback-model", OptionValues::One),
                ("--file", OptionValues::Many),
                ("--input-format", OptionValues::One),
                ("--json-schema", OptionValues::One),
                ("--max-budget-usd", OptionValues::One),
                ("--mcp-config", OptionValues::Many),
                ("--model", OptionValues::One),
                ("-n", OptionValues::One),
                ("--name", OptionValues::One),
                ("--output-format", OptionValues::One),
                ("--permission-mode", OptionValues::One),
                ("--plugin-dir", OptionValues::One),
                ("--plugin-url", OptionValues::One),
                ("--remote-control", OptionValues::Optional),
                ("--remote-control-session-name-prefix", OptionValues::One),
                ("--setting-sources", OptionValues::One),
                ("--settings", OptionValues::One),
                ("--system-prompt", OptionValues::One),
                ("--tools", OptionValues::Many),
                ("-w", OptionValues::Optional),
                ("--worktree", OptionValues::Optional),
            ],
        },
        "codex" => AgentOptionRules {
            // Codex selects a session through the `resume` subcommand and a
            // positional id, both of which are bare tokens.
            dropped: &[("--last", OptionValues::None)],
            known: &[
                ("--approve-for-me", OptionValues::None),
                (
                    "--dangerously-bypass-approvals-and-sandbox",
                    OptionValues::None,
                ),
                ("--dangerously-bypass-hook-trust", OptionValues::None),
                ("--full-auto", OptionValues::None),
                ("--no-alt-screen", OptionValues::None),
                ("--oss", OptionValues::None),
                ("--search", OptionValues::None),
                ("--strict-config", OptionValues::None),
                ("-a", OptionValues::One),
                ("--add-dir", OptionValues::One),
                ("--ask-for-approval", OptionValues::One),
                ("-C", OptionValues::One),
                ("-c", OptionValues::One),
                ("--cd", OptionValues::One),
                ("--config", OptionValues::One),
                ("--disable", OptionValues::One),
                ("--enable", OptionValues::One),
                ("-i", OptionValues::Many),
                ("--image", OptionValues::Many),
                ("--local-provider", OptionValues::One),
                ("-m", OptionValues::One),
                ("--model", OptionValues::One),
                ("-p", OptionValues::One),
                ("--profile", OptionValues::One),
                ("--remote", OptionValues::One),
                ("--remote-auth-token-env", OptionValues::One),
                ("-s", OptionValues::One),
                ("--sandbox", OptionValues::One),
            ],
        },
        "copilot" => AgentOptionRules {
            dropped: &[
                ("-r", OptionValues::Optional),
                ("--resume", OptionValues::Optional),
                ("--continue", OptionValues::None),
                ("-p", OptionValues::One),
                ("--prompt", OptionValues::One),
                ("-i", OptionValues::One),
                ("--interactive", OptionValues::One),
                ("--session-id", OptionValues::One),
                ("--acp", OptionValues::None),
                ("--connect", OptionValues::Optional),
            ],
            known: &[
                ("--allow-all", OptionValues::None),
                ("--allow-all-mcp-server-instructions", OptionValues::None),
                ("--allow-all-paths", OptionValues::None),
                ("--allow-all-tools", OptionValues::None),
                ("--allow-all-urls", OptionValues::None),
                ("--autopilot", OptionValues::None),
                ("--banner", OptionValues::None),
                ("--disable-builtin-mcps", OptionValues::None),
                ("--disallow-temp-dir", OptionValues::None),
                ("--enable-all-github-mcp-tools", OptionValues::None),
                ("--enable-memory", OptionValues::None),
                ("--enable-reasoning-summaries", OptionValues::None),
                ("--no-ask-user", OptionValues::None),
                ("--no-auto-update", OptionValues::None),
                ("--no-bash-env", OptionValues::None),
                ("--no-color", OptionValues::None),
                ("--no-custom-instructions", OptionValues::None),
                ("--no-mouse", OptionValues::None),
                ("--no-remote", OptionValues::None),
                ("--no-remote-export", OptionValues::None),
                ("--plain-diff", OptionValues::None),
                ("--plan", OptionValues::None),
                ("--screen-reader", OptionValues::None),
                ("--share-gist", OptionValues::None),
                ("-s", OptionValues::None),
                ("--silent", OptionValues::None),
                ("--yolo", OptionValues::None),
                ("--add-dir", OptionValues::One),
                ("--add-github-mcp-tool", OptionValues::One),
                ("--add-github-mcp-toolset", OptionValues::One),
                ("--additional-mcp-config", OptionValues::One),
                ("--agent", OptionValues::One),
                ("--allow-tool", OptionValues::Optional),
                ("--allow-url", OptionValues::Optional),
                ("--attachment", OptionValues::One),
                ("--available-tools", OptionValues::Optional),
                ("--bash-env", OptionValues::Optional),
                ("--context", OptionValues::One),
                ("--deny-tool", OptionValues::Optional),
                ("--deny-url", OptionValues::Optional),
                ("--disable-mcp-server", OptionValues::One),
                ("--excluded-tools", OptionValues::Optional),
                ("--extension-sdk-path", OptionValues::One),
                ("--log-dir", OptionValues::One),
                ("--log-level", OptionValues::One),
                ("--max-ai-credits", OptionValues::One),
                ("--max-autopilot-continues", OptionValues::One),
                ("--mode", OptionValues::One),
                ("--model", OptionValues::One),
                ("--mouse", OptionValues::Optional),
                ("-n", OptionValues::One),
                ("--name", OptionValues::One),
                ("--output-format", OptionValues::One),
                ("--plugin-dir", OptionValues::One),
                ("--secret-env-vars", OptionValues::Optional),
                ("--stream", OptionValues::One),
            ],
        },
        "grok" => AgentOptionRules {
            dropped: &[
                ("-r", OptionValues::Optional),
                ("--resume", OptionValues::Optional),
                ("-c", OptionValues::None),
                ("--continue", OptionValues::None),
                ("-p", OptionValues::One),
                ("--single", OptionValues::One),
                ("-s", OptionValues::One),
                ("--session-id", OptionValues::One),
                ("--fork-session", OptionValues::None),
                ("--prompt-file", OptionValues::One),
                ("--prompt-json", OptionValues::One),
            ],
            known: &[
                ("--always-approve", OptionValues::None),
                ("--debug", OptionValues::None),
                ("--disable-web-search", OptionValues::None),
                ("--experimental-memory", OptionValues::None),
                ("--fullscreen", OptionValues::None),
                ("--include-partial-messages", OptionValues::None),
                ("--minimal", OptionValues::None),
                ("--no-alt-screen", OptionValues::None),
                ("--no-memory", OptionValues::None),
                ("--no-plan", OptionValues::None),
                ("--no-subagents", OptionValues::None),
                ("--oauth", OptionValues::None),
                ("--restore-code", OptionValues::None),
                ("--verbatim", OptionValues::None),
                ("--agent", OptionValues::One),
                ("--agents", OptionValues::One),
                ("--allow", OptionValues::One),
                ("--cwd", OptionValues::One),
                ("--debug-file", OptionValues::One),
                ("--deny", OptionValues::One),
                ("--disallowed-tools", OptionValues::One),
                ("--json-schema", OptionValues::One),
                ("--leader-socket", OptionValues::One),
                ("-m", OptionValues::One),
                ("--max-turns", OptionValues::One),
                ("--model", OptionValues::One),
                ("--output-format", OptionValues::One),
                ("--permission-mode", OptionValues::One),
                ("--reasoning-effort", OptionValues::One),
                ("--rules", OptionValues::One),
                ("--sandbox", OptionValues::One),
                ("--system-prompt-override", OptionValues::One),
                ("--tools", OptionValues::One),
                ("-w", OptionValues::Optional),
                ("--worktree", OptionValues::Optional),
                ("--worktree-ref", OptionValues::One),
            ],
        },
        "opencode" => AgentOptionRules {
            dropped: &[
                ("-s", OptionValues::One),
                ("--session", OptionValues::One),
                ("-c", OptionValues::None),
                ("--continue", OptionValues::None),
                ("--fork", OptionValues::None),
                ("--prompt", OptionValues::One),
            ],
            known: &[
                ("--mdns", OptionValues::None),
                ("--print-logs", OptionValues::None),
                ("--pure", OptionValues::None),
                ("--agent", OptionValues::One),
                ("--cors", OptionValues::Many),
                ("--hostname", OptionValues::One),
                ("--log-level", OptionValues::One),
                ("--mdns-domain", OptionValues::One),
                ("-m", OptionValues::One),
                ("--model", OptionValues::One),
                ("--port", OptionValues::One),
            ],
        },
        "pi" => AgentOptionRules {
            dropped: &[
                ("-r", OptionValues::None),
                ("--resume", OptionValues::None),
                ("-c", OptionValues::None),
                ("--continue", OptionValues::None),
                ("-p", OptionValues::None),
                ("--print", OptionValues::None),
                ("--session", OptionValues::One),
                ("--session-id", OptionValues::One),
                ("--fork", OptionValues::One),
                ("--no-session", OptionValues::None),
                ("--export", OptionValues::One),
                ("--list-models", OptionValues::Optional),
            ],
            known: &[
                ("-a", OptionValues::None),
                ("--approve", OptionValues::None),
                ("-na", OptionValues::None),
                ("--no-approve", OptionValues::None),
                ("-nbt", OptionValues::None),
                ("--no-builtin-tools", OptionValues::None),
                ("-nc", OptionValues::None),
                ("--no-context-files", OptionValues::None),
                ("-ne", OptionValues::None),
                ("--no-extensions", OptionValues::None),
                ("-np", OptionValues::None),
                ("--no-prompt-templates", OptionValues::None),
                ("-ns", OptionValues::None),
                ("--no-skills", OptionValues::None),
                ("--no-themes", OptionValues::None),
                ("-nt", OptionValues::None),
                ("--no-tools", OptionValues::None),
                ("--offline", OptionValues::None),
                ("--verbose", OptionValues::None),
                ("--api-key", OptionValues::One),
                ("--append-system-prompt", OptionValues::One),
                ("-e", OptionValues::One),
                ("--exclude-tools", OptionValues::One),
                ("--extension", OptionValues::One),
                ("--mode", OptionValues::One),
                ("--model", OptionValues::One),
                ("--models", OptionValues::One),
                ("-n", OptionValues::One),
                ("--name", OptionValues::One),
                ("--prompt-template", OptionValues::One),
                ("--provider", OptionValues::One),
                ("--session-dir", OptionValues::One),
                ("--skill", OptionValues::One),
                ("--system-prompt", OptionValues::One),
                ("-t", OptionValues::One),
                ("--theme", OptionValues::One),
                ("--thinking", OptionValues::One),
                ("--tools", OptionValues::One),
                ("--tui-mode", OptionValues::One),
                ("-xt", OptionValues::One),
            ],
        },
        // OMP takes every value inline as `--option=value`, so only its
        // session-selection flags need listing.
        "omp" => AgentOptionRules {
            dropped: &[
                ("-r", OptionValues::None),
                ("--resume", OptionValues::None),
                ("-c", OptionValues::None),
                ("--continue", OptionValues::None),
                ("-p", OptionValues::None),
                ("--print", OptionValues::None),
                ("--from-claude", OptionValues::None),
                ("--from-codex", OptionValues::None),
                ("--no-session", OptionValues::None),
                ("--alias", OptionValues::One),
                ("--export", OptionValues::One),
            ],
            known: &[
                ("--advisor", OptionValues::None),
                ("--allow-home", OptionValues::None),
                ("--auto-approve", OptionValues::None),
                ("--hide-thinking", OptionValues::None),
                ("--no-extensions", OptionValues::None),
                ("--no-lsp", OptionValues::None),
                ("--no-prewalk", OptionValues::None),
                ("--no-pty", OptionValues::None),
                ("--no-rules", OptionValues::None),
                ("--no-skills", OptionValues::None),
                ("--no-title", OptionValues::None),
                ("--no-tools", OptionValues::None),
                ("--plan-yolo", OptionValues::None),
                ("--prewalk", OptionValues::None),
                ("--print-thoughts", OptionValues::None),
            ],
        },
        "cursor" => AgentOptionRules {
            dropped: &[
                ("--resume", OptionValues::One),
                ("-p", OptionValues::None),
                ("--print", OptionValues::None),
            ],
            known: &[("-m", OptionValues::One), ("--model", OptionValues::One)],
        },
        "devin" => AgentOptionRules {
            dropped: &[("--resume", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        "droid" => AgentOptionRules {
            dropped: &[
                ("--resume", OptionValues::One),
                ("-p", OptionValues::None),
                ("--print", OptionValues::None),
            ],
            known: &[("-m", OptionValues::One), ("--model", OptionValues::One)],
        },
        "kimi" => AgentOptionRules {
            dropped: &[("--session", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        "mastracode" => AgentOptionRules {
            dropped: &[("--thread", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        "hermes" => AgentOptionRules {
            dropped: &[("--resume", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        "qodercli" => AgentOptionRules {
            dropped: &[("--resume", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        "kilo" => AgentOptionRules {
            dropped: &[("--session", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        "agy" => AgentOptionRules {
            dropped: &[("--conversation", OptionValues::One)],
            known: &[("--model", OptionValues::One)],
        },
        _ => AgentOptionRules {
            dropped: &[],
            known: &[],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn keeps_boolean_flags_and_known_option_values() {
        assert_eq!(
            replayable(
                "claude",
                &args(&["--permission-mode", "bypassPermissions", "--verbose"])
            ),
            args(&["--permission-mode", "bypassPermissions", "--verbose"])
        );
        assert_eq!(
            replayable("codex", &args(&["-s", "danger-full-access", "-a", "never"])),
            args(&["-s", "danger-full-access", "-a", "never"])
        );
        assert_eq!(
            replayable("claude", &args(&["--dangerously-skip-permissions"])),
            args(&["--dangerously-skip-permissions"])
        );
    }

    #[test]
    fn drops_session_selection_from_an_earlier_resume() {
        assert_eq!(
            replayable(
                "claude",
                &args(&[
                    "--permission-mode",
                    "bypassPermissions",
                    "--resume",
                    "c1893fd1-3b1a-46d0-9b4f-a09cd2a42c8b",
                ])
            ),
            args(&["--permission-mode", "bypassPermissions"])
        );
        assert_eq!(
            replayable(
                "codex",
                &args(&[
                    "resume",
                    "01997f1e-4b4a-7c31-9c0e-2f1f0a3f0f11",
                    "--full-auto"
                ])
            ),
            args(&["--full-auto"])
        );
        assert_eq!(
            replayable("copilot", &args(&["--allow-all-tools", "--resume=abc123"])),
            args(&["--allow-all-tools"])
        );
        assert_eq!(
            replayable("omp", &args(&["--model=opus", "--resume=abc123"])),
            args(&["--model=opus"])
        );
    }

    #[test]
    fn drops_prompts_and_non_interactive_options() {
        assert_eq!(
            replayable(
                "claude",
                &args(&["--dangerously-skip-permissions", "fix the failing test"])
            ),
            args(&["--dangerously-skip-permissions"])
        );
        assert_eq!(
            replayable("claude", &args(&["-p", "summarize this repo"])),
            Vec::<String>::new()
        );
        assert_eq!(
            replayable("grok", &args(&["--always-approve", "fix the bug"])),
            args(&["--always-approve"])
        );
        assert_eq!(
            replayable("claude", &args(&["--verbose", "--", "trailing prompt"])),
            args(&["--verbose"])
        );
    }

    #[test]
    fn drops_unrecognized_options_that_may_carry_a_value() {
        assert_eq!(
            replayable(
                "claude",
                &args(&["--future-option", "value", "--dangerously-skip-permissions"])
            ),
            args(&["--dangerously-skip-permissions"])
        );
        assert_eq!(
            replayable("claude", &args(&["--future-flag", "--verbose"])),
            args(&["--future-flag", "--verbose"])
        );
        assert_eq!(
            replayable("claude", &args(&["--future-option=value"])),
            args(&["--future-option=value"])
        );
    }

    #[test]
    fn keeps_every_value_of_a_repeated_option() {
        assert_eq!(
            replayable(
                "claude",
                &args(&["--add-dir", "/one", "/two", "--permission-mode", "plan"])
            ),
            args(&["--add-dir", "/one", "/two", "--permission-mode", "plan"])
        );
    }

    #[test]
    fn drops_help_and_version_for_every_agent() {
        assert_eq!(
            replayable("hermes", &args(&["--help", "--model", "sonnet"])),
            args(&["--model", "sonnet"])
        );
        assert_eq!(replayable("codex", &args(&["-V"])), Vec::<String>::new());
    }

    #[test]
    fn unknown_agents_keep_only_unambiguous_options() {
        assert_eq!(
            replayable("something-else", &args(&["--flag", "value", "--other"])),
            args(&["--other"])
        );
    }
}
