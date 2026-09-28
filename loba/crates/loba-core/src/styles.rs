//! `styles.json` — how the destination app changes the voice of the cleanup.
//!
//! Resolution order: app name or bundle id (exact, case-insensitive) → window
//! title substring (for web apps in a browser: "Gmail", "Slack") → `default`.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub struct Style {
    /// Tone guidance appended to the cleanup prompt.
    #[serde(default)]
    pub tone: String,
    /// `false` = literal: no LLM at all, only dictionary rules. For editors
    /// and terminals, where a "helpful" rewrite breaks commands and code.
    #[serde(default = "yes")]
    pub rewrite: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub struct Styles {
    #[serde(default = "default_style_id")]
    pub default: String,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    /// App name ("WeChat") or bundle id ("com.tencent.xinWeChat") → style id.
    #[serde(default)]
    pub apps: BTreeMap<String, String>,
    /// Window-title substring → style id. Consulted when the app itself has
    /// no mapping, which is how Gmail-in-Safari reads as formal.
    #[serde(default)]
    pub window_titles: BTreeMap<String, String>,
}

fn default_style_id() -> String {
    "neutral".into()
}

/// What was frontmost when the user started speaking.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Type)]
pub struct AppContext {
    pub name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
pub struct Resolved {
    pub id: String,
    pub style: Style,
    /// Which key matched, for the history panel ("Slack", "title: Gmail").
    pub matched_by: Option<String>,
}

impl Default for Styles {
    fn default() -> Self {
        Self::seed()
    }
}

impl Styles {
    pub fn seed() -> Self {
        let style = |tone: &str, rewrite: bool| Style {
            tone: tone.into(),
            rewrite,
        };
        let styles = BTreeMap::from([
            (
                "neutral".into(),
                style("Clear and natural. Keep the speaker's register.", true),
            ),
            (
                "casual".into(),
                style(
                    "Chat message. Relaxed and short. Keep contractions, slang and emoji the speaker said. \
                     No greeting or sign-off added. A trailing period on a one-line message may be dropped.",
                    true,
                ),
            ),
            (
                "formal".into(),
                style(
                    "Email. Complete sentences, professional register, paragraphs where the speaker paused \
                     between ideas. Do not add a greeting or signature the speaker didn't say.",
                    true,
                ),
            ),
            ("literal".into(), style("", false)),
        ]);
        let map = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        Self {
            default: default_style_id(),
            styles,
            apps: map(&[
                ("WeChat", "casual"),
                ("微信", "casual"),
                ("Slack", "casual"),
                ("Messages", "casual"),
                ("信息", "casual"),
                ("Telegram", "casual"),
                ("WhatsApp", "casual"),
                ("Discord", "casual"),
                ("Mail", "formal"),
                ("邮件", "formal"),
                ("Microsoft Outlook", "formal"),
                ("Spark", "formal"),
                ("Superhuman", "formal"),
                ("Code", "literal"),
                ("Visual Studio Code", "literal"),
                ("com.microsoft.VSCode", "literal"),
                ("Cursor", "literal"),
                ("Zed", "literal"),
                ("Xcode", "literal"),
                ("Terminal", "literal"),
                ("终端", "literal"),
                ("iTerm2", "literal"),
                ("Ghostty", "literal"),
                ("Warp", "literal"),
            ]),
            window_titles: map(&[
                ("Gmail", "formal"),
                ("Outlook", "formal"),
                ("Slack", "casual"),
                ("WhatsApp", "casual"),
            ]),
        }
    }

    /// Drop mappings to styles that don't exist (a typo in a hand edit should
    /// degrade to the default, not to "no tone"), and guarantee a default.
    pub fn normalized(mut self) -> Self {
        if self.styles.is_empty() {
            self.styles = Self::seed().styles;
        }
        if !self.styles.contains_key(&self.default) {
            self.default = self
                .styles
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(default_style_id);
        }
        let known = |id: &String| self.styles.contains_key(id);
        let apps = self
            .apps
            .clone()
            .into_iter()
            .filter(|(_, v)| known(v))
            .collect();
        let titles = self
            .window_titles
            .clone()
            .into_iter()
            .filter(|(_, v)| known(v))
            .collect();
        self.apps = apps;
        self.window_titles = titles;
        self
    }

    pub fn resolve(&self, ctx: &AppContext) -> Resolved {
        let hit = |id: &str, by: String| {
            self.styles.get(id).map(|style| Resolved {
                id: id.to_string(),
                style: style.clone(),
                matched_by: Some(by),
            })
        };

        for key in [&ctx.name, &ctx.bundle_id].into_iter().flatten() {
            if let Some((k, id)) = self.apps.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
                if let Some(r) = hit(id, k.clone()) {
                    return r;
                }
            }
        }

        if let Some(title) = &ctx.window_title {
            let title = title.to_lowercase();
            // Longest key first: "Outlook Calendar" beats "Outlook".
            let mut keys: Vec<_> = self.window_titles.iter().collect();
            keys.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
            for (k, id) in keys {
                if !k.is_empty() && title.contains(&k.to_lowercase()) {
                    if let Some(r) = hit(id, format!("title: {k}")) {
                        return r;
                    }
                }
            }
        }

        Resolved {
            id: self.default.clone(),
            style: self.styles.get(&self.default).cloned().unwrap_or(Style {
                tone: String::new(),
                rewrite: true,
            }),
            matched_by: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> AppContext {
        AppContext {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    #[test]
    fn maps_apps_from_the_spec() {
        let s = Styles::seed();
        assert_eq!(s.resolve(&app("WeChat")).id, "casual");
        assert_eq!(s.resolve(&app("Slack")).id, "casual");
        assert_eq!(s.resolve(&app("Mail")).id, "formal");
        assert_eq!(s.resolve(&app("Terminal")).id, "literal");
        assert!(!s.resolve(&app("Terminal")).style.rewrite);
        assert_eq!(s.resolve(&app("Code")).id, "literal");
        assert_eq!(s.resolve(&app("Notes")).id, "neutral");
    }

    #[test]
    fn bundle_id_and_case_insensitive() {
        let s = Styles::seed();
        let ctx = AppContext {
            name: Some("Visual Studio Code - Insiders".into()),
            bundle_id: Some("COM.MICROSOFT.VSCODE".into()),
            window_title: None,
        };
        assert_eq!(s.resolve(&ctx).id, "literal");
    }

    #[test]
    fn gmail_in_a_browser_reads_formal() {
        let s = Styles::seed();
        let ctx = AppContext {
            name: Some("Safari".into()),
            bundle_id: Some("com.apple.Safari".into()),
            window_title: Some("Inbox (3) - me@example.com - Gmail".into()),
        };
        let r = s.resolve(&ctx);
        assert_eq!(r.id, "formal");
        assert_eq!(r.matched_by.as_deref(), Some("title: Gmail"));
    }

    #[test]
    fn app_mapping_beats_title() {
        // Editing gmail.ts in VS Code is still code.
        let s = Styles::seed();
        let ctx = AppContext {
            name: Some("Code".into()),
            bundle_id: None,
            window_title: Some("gmail.ts — inbox".into()),
        };
        assert_eq!(s.resolve(&ctx).id, "literal");
    }

    #[test]
    fn dangling_mapping_falls_back_to_default() {
        let mut s = Styles::seed();
        s.apps.insert("Notion".into(), "fancy".into());
        s.default = "missing".into();
        let s = s.normalized();
        assert!(!s.apps.contains_key("Notion"));
        assert!(s.styles.contains_key(&s.default));
    }

    #[test]
    fn hand_written_file_parses() {
        let s: Styles = serde_json::from_str(
            r#"{
                "default": "neutral",
                "styles": { "neutral": {"tone": "x"}, "literal": {"rewrite": false} },
                "apps": { "Ghostty": "literal" }
            }"#,
        )
        .unwrap();
        assert!(s.styles["neutral"].rewrite);
        assert!(!s.resolve(&app("ghostty")).style.rewrite);
    }
}
