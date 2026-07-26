//! Per-model request capabilities.
//!
//! Anthropic keeps *removing* request parameters as new generations ship, and a
//! request that carries a parameter the target model no longer knows about is
//! rejected with a 400 rather than ignored. The rules that matter to ClewdR:
//!
//! * `temperature`, `top_p` and `top_k` are gone from Claude 4.7 and later
//!   (Opus 4.7/4.8, Opus 5, Sonnet 5, Fable 5). Setting any of them is a 400.
//! * On Claude 4.6 those three are rejected whenever thinking is active, and on
//!   4.5 and earlier whenever extended thinking is active.
//! * From Claude 4.1 onwards `temperature` and `top_p` are mutually exclusive.
//! * Extended thinking (`thinking.type: "enabled"` + `budget_tokens`) is
//!   deprecated on 4.6 and rejected from 4.7 onwards; adaptive thinking
//!   (`thinking.type: "adaptive"`) is rejected on 4.5 and earlier.
//! * Fable/Mythos always think and reject both `enabled` and `disabled`.
//! * Thinking is off by default on every 4.x model, and on by default on the
//!   5 series -- where `display` then defaults to `omitted`, so the reasoning
//!   is billed but never sent to the client.
//! * `output_config.effort` only exists from Opus 4.5 onwards, and the `xhigh`
//!   rung only from 4.7 onwards.
//! * Claude Opus 5 and later reject `thinking.type: "disabled"` at `xhigh` or
//!   `max` effort.
//!
//! Frontends such as SillyTavern send whatever their preset holds, so ClewdR
//! rewrites the request to the nearest shape the target model accepts instead
//! of forwarding a request that is guaranteed to fail.

use crate::types::claude::{CreateMessageParams, OutputEffort, Thinking};

/// Budget used when a request has to be expressed in legacy extended-thinking
/// terms.
pub const DEFAULT_THINKING_BUDGET: u64 = 4096;

/// Smallest `budget_tokens` the API accepts; anything lower is a 400.
const MIN_THINKING_BUDGET: u64 = 1024;

/// Model-name suffix that turns thinking on for clients that can only pick a
/// model string.
pub const THINKING_SUFFIX: &str = "-thinking";

/// Which thinking modes a model accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThinkingSupport {
    /// Thinking is always on; both `enabled` and `disabled` are rejected.
    AlwaysOn,
    /// Only `adaptive`; `enabled` is rejected (Claude 4.7 and later).
    AdaptiveOnly,
    /// Both modes work, `enabled` being deprecated (the Claude 4.6 generation).
    AdaptiveOrExtended,
    /// Only `enabled` + `budget_tokens`; `adaptive` is rejected (4.5 and older).
    ExtendedOnly,
}

/// Model line, used to separate the always-thinking research models from the
/// regular Opus/Sonnet/Haiku ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelLine {
    Opus,
    Sonnet,
    Haiku,
    /// Fable and Mythos, which share the same request surface.
    Fable,
    Other,
}

/// What a given model will accept in a `/v1/messages` request.
#[derive(Debug, Clone, Copy)]
pub struct ModelTraits {
    /// `(major, minor)` of the model generation, e.g. `claude-opus-4-6` is `(4, 6)`.
    generation: (u32, u32),
    thinking: ThinkingSupport,
    /// Whether `temperature`/`top_p`/`top_k` exist on this model at all.
    sampling: bool,
    /// Effort rungs the model accepts, empty when `output_config.effort` is unsupported.
    efforts: &'static [OutputEffort],
}

const EFFORT_BASIC: &[OutputEffort] =
    &[OutputEffort::Low, OutputEffort::Medium, OutputEffort::High];
const EFFORT_MAX: &[OutputEffort] = &[
    OutputEffort::Low,
    OutputEffort::Medium,
    OutputEffort::High,
    OutputEffort::Max,
];
const EFFORT_FULL: &[OutputEffort] = &[
    OutputEffort::Low,
    OutputEffort::Medium,
    OutputEffort::High,
    OutputEffort::XHigh,
    OutputEffort::Max,
];

impl ModelTraits {
    /// Look up what `model` accepts. Returns `None` for names ClewdR cannot
    /// place, in which case the request is forwarded untouched.
    #[must_use]
    pub fn of(model: &str) -> Option<Self> {
        let name = model
            .strip_suffix(THINKING_SUFFIX)
            .unwrap_or(model)
            .to_ascii_lowercase();
        let line = model_line(&name)?;
        let generation = generation(&name)?;

        // Fable/Mythos always think and expose no sampling knobs.
        if line == ModelLine::Fable {
            return Some(Self {
                generation,
                thinking: ThinkingSupport::AlwaysOn,
                sampling: false,
                efforts: EFFORT_FULL,
            });
        }

        let (thinking, sampling, efforts) = match generation {
            // Claude 4.7 and later: sampling parameters and extended thinking removed.
            g if g >= (4, 7) => (ThinkingSupport::AdaptiveOnly, false, EFFORT_FULL),
            // The 4.6 generation: both thinking modes, sampling only without thinking.
            (4, 6) => (ThinkingSupport::AdaptiveOrExtended, true, EFFORT_MAX),
            // Opus 4.5 is the only extended-thinking-only model with effort.
            (4, 5) if line == ModelLine::Opus => {
                (ThinkingSupport::ExtendedOnly, true, EFFORT_BASIC)
            }
            _ => (ThinkingSupport::ExtendedOnly, true, &[][..]),
        };

        Some(Self {
            generation,
            thinking,
            sampling,
            efforts,
        })
    }

    /// Thinking configuration meant by the `-thinking` model-name suffix.
    ///
    /// The suffix exists for clients that can only pick a model string. It
    /// does two different jobs depending on the target:
    ///
    /// * On the 4.x models thinking is off until asked for, so this turns it
    ///   on -- as `adaptive` from 4.6, as a token budget below that.
    /// * On the 5 series thinking is already on, but `display` defaults to
    ///   `omitted`, so the reasoning is billed and then discarded. Here the
    ///   suffix only makes it visible.
    ///
    /// Both cases emit the same `adaptive` + `summarized` config, which is why
    /// they share an arm; only the reason differs.
    #[must_use]
    pub fn thinking_for_suffix(&self) -> Thinking {
        match self.thinking {
            ThinkingSupport::AlwaysOn
            | ThinkingSupport::AdaptiveOnly
            | ThinkingSupport::AdaptiveOrExtended => Thinking::adaptive_summarized(),
            ThinkingSupport::ExtendedOnly => Thinking::new(DEFAULT_THINKING_BUDGET),
        }
    }

    /// Rewrite `params` into the nearest shape this model accepts.
    ///
    /// Preserves intent where it can (a budget-based thinking request becomes
    /// an adaptive one rather than being dropped) and discards parameters the
    /// model would reject outright.
    pub fn sanitize(&self, params: &mut CreateMessageParams) {
        self.coerce_thinking(params);
        clamp_thinking_budget(params);
        self.clamp_effort(params);
        self.strip_sampling(params);
    }

    /// Map the requested thinking mode onto one this model implements.
    fn coerce_thinking(&self, params: &mut CreateMessageParams) {
        let Some(requested) = params.thinking.take() else {
            return;
        };
        params.thinking = match (self.thinking, requested) {
            (ThinkingSupport::AlwaysOn, Thinking::Disabled) => None,
            // Neither line takes a budget, but the request still means "think",
            // so it becomes adaptive rather than being dropped.
            (
                ThinkingSupport::AlwaysOn | ThinkingSupport::AdaptiveOnly,
                Thinking::Enabled { .. },
            ) => Some(Thinking::adaptive_summarized()),
            // 4.5 and earlier never learned adaptive.
            (ThinkingSupport::ExtendedOnly, Thinking::Adaptive { .. }) => {
                Some(Thinking::new(DEFAULT_THINKING_BUDGET))
            }
            (_, other) => Some(other),
        };
    }

    /// Drop or step down `output_config.effort` to a rung this model has.
    ///
    /// Also resolves the Opus 5 rule that thinking cannot be switched off at
    /// `xhigh` or `max`, by lowering the effort so the client's explicit
    /// `disabled` survives.
    fn clamp_effort(&self, params: &mut CreateMessageParams) {
        let Some(config) = params.output_config.as_mut() else {
            return;
        };
        if let Some(requested) = config.effort {
            let ceiling = if self.generation >= (5, 0)
                && matches!(params.thinking, Some(Thinking::Disabled))
            {
                OutputEffort::High
            } else {
                OutputEffort::Max
            };
            config.effort = self
                .efforts
                .iter()
                .copied()
                .filter(|level| *level <= requested && *level <= ceiling)
                .max();
        }
        if config.effort.is_none() && config.format.is_none() {
            params.output_config = None;
        }
    }

    /// Remove sampling parameters the model would reject.
    fn strip_sampling(&self, params: &mut CreateMessageParams) {
        let blocked = !self.sampling
            || match self.thinking {
                // Any active thinking mode locks out sampling on 4.6.
                ThinkingSupport::AdaptiveOrExtended => matches!(
                    params.thinking,
                    Some(Thinking::Adaptive { .. } | Thinking::Enabled { .. })
                ),
                // Only the budget mode does on 4.5 and earlier.
                ThinkingSupport::ExtendedOnly => {
                    matches!(params.thinking, Some(Thinking::Enabled { .. }))
                }
                // Unreachable: these lines have no sampling parameters at all.
                ThinkingSupport::AlwaysOn | ThinkingSupport::AdaptiveOnly => true,
            };
        if blocked {
            params.temperature = None;
            params.top_p = None;
            params.top_k = None;
        } else if params.temperature.is_some() && self.generation >= (4, 1) {
            // `temperature` and `top_p` cannot both be specified from 4.1 on.
            params.top_p = None;
        }
    }
}

/// Keep `budget_tokens` inside the window the API allows.
///
/// It has to be at least 1024 and strictly below `max_tokens`, since thinking
/// tokens are drawn from the same allowance as the reply. When `max_tokens`
/// leaves no room at all, thinking is dropped rather than sent as a 400.
fn clamp_thinking_budget(params: &mut CreateMessageParams) {
    let Some(Thinking::Enabled { budget_tokens }) = params.thinking else {
        return;
    };
    let ceiling = u64::from(params.max_tokens).saturating_sub(1);
    if ceiling < MIN_THINKING_BUDGET {
        params.thinking = None;
        return;
    }
    params.thinking = Some(Thinking::new(
        budget_tokens.clamp(MIN_THINKING_BUDGET, ceiling),
    ));
}

/// Which product line `name` belongs to.
fn model_line(name: &str) -> Option<ModelLine> {
    // Ordered so that the always-thinking research line wins over substrings.
    for (needle, line) in [
        ("fable", ModelLine::Fable),
        ("mythos", ModelLine::Fable),
        ("opus", ModelLine::Opus),
        ("sonnet", ModelLine::Sonnet),
        ("haiku", ModelLine::Haiku),
    ] {
        if name.contains(needle) {
            return Some(line);
        }
    }
    name.starts_with("claude").then_some(ModelLine::Other)
}

/// Parse `(major, minor)` out of a model id.
///
/// Handles the dateless 4.6-and-later ids (`claude-opus-4-6`, `claude-opus-5`),
/// the dated older ones (`claude-sonnet-4-5-20250929`) and the legacy
/// family-last spelling (`claude-3-7-sonnet-20250219`) by taking the first one
/// or two short numeric segments and ignoring date stamps.
fn generation(name: &str) -> Option<(u32, u32)> {
    let mut parts = name
        .split('-')
        .filter(|part| part.len() <= 2 && part.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|part| part.parse::<u32>().ok());
    let major = parts.next()?;
    Some((major, parts.next().unwrap_or(0)))
}

/// Base ids advertised by `/v1/models`, newest first.
///
/// Retired ids are omitted: requests to them fail upstream, so listing them
/// only misleads clients. Kept in sync with Anthropic's model deprecation page.
pub const ACTIVE_MODELS: [&str; 10] = [
    "claude-fable-5",
    "claude-opus-5",
    "claude-opus-4-8",
    "claude-opus-4-7",
    "claude-opus-4-6",
    "claude-opus-4-5-20251101",
    "claude-sonnet-5",
    "claude-sonnet-4-6",
    "claude-sonnet-4-5-20250929",
    "claude-haiku-4-5-20251001",
];

/// Every advertised id, each base model followed by its `-thinking` variant.
pub fn advertised_models() -> impl Iterator<Item = String> {
    ACTIVE_MODELS
        .into_iter()
        .flat_map(|model| [model.to_string(), format!("{model}{THINKING_SUFFIX}")])
}

/// Strip the `-thinking` suffix, reporting whether it was present.
#[must_use]
pub fn split_thinking_suffix(model: &str) -> (&str, bool) {
    match model.strip_suffix(THINKING_SUFFIX) {
        Some(base) => (base, true),
        None => (model, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::claude::{Message, OutputConfig, Role};

    fn params(model: &str) -> CreateMessageParams {
        CreateMessageParams {
            model: model.to_string(),
            messages: vec![Message::new_text(Role::User, "hi")],
            max_tokens: 32_000,
            ..Default::default()
        }
    }

    fn budget(p: &CreateMessageParams) -> Option<u64> {
        match p.thinking {
            Some(Thinking::Enabled { budget_tokens }) => Some(budget_tokens),
            _ => None,
        }
    }

    #[test]
    fn parses_every_id_spelling() {
        assert_eq!(generation("claude-opus-4-6"), Some((4, 6)));
        assert_eq!(generation("claude-opus-5"), Some((5, 0)));
        assert_eq!(generation("claude-sonnet-4-5-20250929"), Some((4, 5)));
        assert_eq!(generation("claude-3-7-sonnet-20250219"), Some((3, 7)));
        assert_eq!(generation("claude-haiku-4-5-20251001"), Some((4, 5)));
        assert_eq!(generation("claude-fable-5"), Some((5, 0)));
    }

    #[test]
    fn unknown_models_are_forwarded_untouched() {
        assert!(ModelTraits::of("gpt-5.5").is_none());
        assert!(ModelTraits::of("claude-opus").is_none());
    }

    #[test]
    fn thinking_suffix_matches_the_generation() {
        let adaptive = Thinking::adaptive_summarized();
        for model in ["claude-opus-4-8", "claude-sonnet-4-6", "claude-fable-5"] {
            let got = ModelTraits::of(model).unwrap().thinking_for_suffix();
            assert_eq!(
                serde_json::to_value(&got).unwrap(),
                serde_json::to_value(&adaptive).unwrap(),
                "{model}"
            );
        }
        // Opus 5 already thinks; the suffix is what reveals the text.
        let already_thinking = ModelTraits::of("claude-opus-5")
            .unwrap()
            .thinking_for_suffix();
        assert!(matches!(
            already_thinking,
            Thinking::Adaptive { display: Some(_) }
        ));

        let legacy = ModelTraits::of("claude-sonnet-4-5-20250929")
            .unwrap()
            .thinking_for_suffix();
        assert!(matches!(legacy, Thinking::Enabled { .. }));
    }

    #[test]
    fn strips_sampling_params_from_4_7_onwards() {
        for model in ["claude-opus-4-7", "claude-opus-5", "claude-fable-5"] {
            let mut p = params(model);
            p.temperature = Some(0.7);
            p.top_p = Some(0.9);
            p.top_k = Some(40);
            ModelTraits::of(model).unwrap().sanitize(&mut p);
            assert_eq!(p.temperature, None, "{model}");
            assert_eq!(p.top_p, None, "{model}");
            assert_eq!(p.top_k, None, "{model}");
        }
    }

    #[test]
    fn keeps_temperature_but_drops_top_p_when_both_given() {
        let mut p = params("claude-sonnet-4-6");
        p.temperature = Some(0.7);
        p.top_p = Some(0.9);
        ModelTraits::of("claude-sonnet-4-6")
            .unwrap()
            .sanitize(&mut p);
        assert_eq!(p.temperature, Some(0.7));
        assert_eq!(p.top_p, None);
    }

    #[test]
    fn adaptive_thinking_locks_out_sampling_on_4_6() {
        let mut p = params("claude-opus-4-6");
        p.temperature = Some(0.7);
        p.thinking = Some(Thinking::adaptive_summarized());
        ModelTraits::of("claude-opus-4-6").unwrap().sanitize(&mut p);
        assert_eq!(p.temperature, None);

        // Extended thinking on 4.5 blocks it too, plain sampling does not.
        let mut p = params("claude-sonnet-4-5-20250929");
        p.temperature = Some(0.7);
        let traits = ModelTraits::of("claude-sonnet-4-5-20250929").unwrap();
        traits.sanitize(&mut p);
        assert_eq!(p.temperature, Some(0.7));
        p.thinking = Some(Thinking::new(DEFAULT_THINKING_BUDGET));
        traits.sanitize(&mut p);
        assert_eq!(p.temperature, None);
    }

    #[test]
    fn converts_thinking_mode_to_what_the_model_implements() {
        let mut p = params("claude-opus-4-8");
        p.thinking = Some(Thinking::new(8192));
        ModelTraits::of("claude-opus-4-8").unwrap().sanitize(&mut p);
        assert!(matches!(p.thinking, Some(Thinking::Adaptive { .. })));

        let mut p = params("claude-haiku-4-5-20251001");
        p.thinking = Some(Thinking::adaptive_summarized());
        ModelTraits::of("claude-haiku-4-5-20251001")
            .unwrap()
            .sanitize(&mut p);
        assert!(matches!(p.thinking, Some(Thinking::Enabled { .. })));

        // Fable rejects `disabled` outright.
        let mut p = params("claude-fable-5");
        p.thinking = Some(Thinking::Disabled);
        ModelTraits::of("claude-fable-5").unwrap().sanitize(&mut p);
        assert!(p.thinking.is_none());
    }

    #[test]
    fn keeps_the_thinking_budget_inside_max_tokens() {
        let model = "claude-sonnet-4-5-20250929";
        let traits = ModelTraits::of(model).unwrap();

        // Budget must stay strictly below max_tokens.
        let mut p = params(model);
        p.max_tokens = 4096;
        p.thinking = Some(Thinking::new(16384));
        traits.sanitize(&mut p);
        assert_eq!(budget(&p), Some(4095));

        // ...and at or above the API floor of 1024.
        let mut p = params(model);
        p.thinking = Some(Thinking::new(256));
        traits.sanitize(&mut p);
        assert_eq!(budget(&p), Some(1024));

        // No room for both thinking and a reply: drop thinking instead of 400ing.
        let mut p = params(model);
        p.max_tokens = 512;
        p.thinking = Some(Thinking::new(4096));
        traits.sanitize(&mut p);
        assert!(p.thinking.is_none());
    }

    #[test]
    fn clamps_effort_to_the_supported_ladder() {
        let effort = |model: &str, requested| {
            let mut p = params(model);
            p.output_config = Some(OutputConfig {
                effort: Some(requested),
                format: None,
            });
            ModelTraits::of(model).unwrap().sanitize(&mut p);
            p.output_config.and_then(|c| c.effort)
        };

        // xhigh only exists from 4.7 on.
        assert_eq!(
            effort("claude-opus-4-7", OutputEffort::XHigh),
            Some(OutputEffort::XHigh)
        );
        assert_eq!(
            effort("claude-sonnet-4-6", OutputEffort::XHigh),
            Some(OutputEffort::High)
        );
        // Opus 4.5 tops out at high.
        assert_eq!(
            effort("claude-opus-4-5-20251101", OutputEffort::Max),
            Some(OutputEffort::High)
        );
        // Sonnet 4.5 has no effort parameter at all.
        assert_eq!(
            effort("claude-sonnet-4-5-20250929", OutputEffort::Low),
            None
        );
    }

    #[test]
    fn lowers_effort_so_opus_5_can_disable_thinking() {
        let mut p = params("claude-opus-5");
        p.thinking = Some(Thinking::Disabled);
        p.output_config = Some(OutputConfig {
            effort: Some(OutputEffort::Max),
            format: None,
        });
        ModelTraits::of("claude-opus-5").unwrap().sanitize(&mut p);
        assert!(matches!(p.thinking, Some(Thinking::Disabled)));
        assert_eq!(
            p.output_config.and_then(|c| c.effort),
            Some(OutputEffort::High)
        );
    }

    #[test]
    fn advertises_a_thinking_variant_per_model() {
        let all = advertised_models().collect::<Vec<_>>();
        assert_eq!(all.len(), ACTIVE_MODELS.len() * 2);
        assert!(all.iter().all(|m| ModelTraits::of(m).is_some()));
    }
}
