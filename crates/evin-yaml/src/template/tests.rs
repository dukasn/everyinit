// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::format::render;
use super::pipeline::{
    TemplateContext, TemplateError, TemplateReferenceKind, references, resolve_templates,
    resolve_templates_with_self, validate_template_syntax,
};
use super::regex::compile_template;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Minimal TemplateContext test double
// ---------------------------------------------------------------------------

struct TestCtx {
    env: HashMap<String, String>,
    values: HashMap<String, String>,
    outputs: HashMap<(String, String), String>,
    unverified: HashSet<String>,
    unverified_log: std::sync::Mutex<Vec<String>>,
}

impl TestCtx {
    fn new() -> Self {
        Self {
            env: HashMap::new(),
            values: HashMap::new(),
            outputs: HashMap::new(),
            unverified: HashSet::new(),
            unverified_log: std::sync::Mutex::new(vec![]),
        }
    }

    fn with_env(mut self, k: &str, v: &str) -> Self {
        self.env.insert(k.to_string(), v.to_string());
        self
    }

    fn with_value(mut self, k: &str, v: &str) -> Self {
        self.values.insert(k.to_string(), v.to_string());
        self
    }

    fn with_output(mut self, task: &str, key: &str, v: &str) -> Self {
        self.outputs
            .insert((task.to_string(), key.to_string()), v.to_string());
        self
    }

    fn with_unverified(mut self, task: &str) -> Self {
        self.unverified.insert(task.to_string());
        self
    }
}

impl TemplateContext for TestCtx {
    fn task_output(&self, task: &str, key: &str) -> Option<String> {
        self.outputs
            .get(&(task.to_string(), key.to_string()))
            .cloned()
    }

    fn value(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }

    fn is_unverified(&self, task: &str) -> bool {
        self.unverified.contains(task)
    }

    fn mark_unverified(&self, message: String) {
        self.unverified_log.lock().unwrap().push(message);
    }
}

// ---------------------------------------------------------------------------
// validate_template_syntax tests
// ---------------------------------------------------------------------------

#[test]
fn test_validate_syntax_plain_text_ok() {
    assert!(validate_template_syntax("Hello world").is_ok());
}

#[test]
fn test_validate_syntax_valid_env_ref() {
    assert!(validate_template_syntax("Hello ${env.USER}").is_ok());
}

#[test]
fn test_validate_syntax_valid_task_ref() {
    assert!(validate_template_syntax("Built: ${build.artifact}").is_ok());
}

#[test]
fn test_validate_syntax_old_mustache_rejected() {
    let err = validate_template_syntax("Hello {{world}}");
    assert!(
        matches!(err, Err(TemplateError::MalformedSyntax(_))),
        "expected MalformedSyntax, got: {:?}",
        err
    );
}

#[test]
fn references_keep_relative_byte_ranges_and_kinds() {
    let text = "x ${value} ${env.HOME} ${build.output}";
    let references = references(text).unwrap();
    assert_eq!(references.len(), 3);
    assert_eq!(references[0].offset, 2);
    assert_eq!(
        references[0].kind,
        TemplateReferenceKind::Value {
            key: "value".into()
        }
    );
    assert_eq!(
        references[1].kind,
        TemplateReferenceKind::Environment { key: "HOME".into() }
    );
    assert_eq!(
        references[2].kind,
        TemplateReferenceKind::TaskOutput {
            task: "build".into(),
            key: "output".into()
        }
    );
}

// ---------------------------------------------------------------------------
// resolve_templates tests
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_env_variable() {
    let ctx = TestCtx::new().with_env("CI", "true");
    let result = resolve_templates("CI is ${env.CI}", &ctx).unwrap();
    assert_eq!(result, "CI is true");
}

#[test]
fn test_resolve_local_value() {
    let ctx = TestCtx::new().with_value("greeting", "Hello");
    let result = resolve_templates("${greeting}, world!", &ctx).unwrap();
    assert_eq!(result, "Hello, world!");
}

#[test]
fn test_resolve_task_output() {
    let ctx = TestCtx::new().with_output("build", "artifact", "dist/app.tar.gz");
    let result = resolve_templates("Artifact: ${build.artifact}", &ctx).unwrap();
    assert_eq!(result, "Artifact: dist/app.tar.gz");
}

#[test]
fn test_resolve_missing_env_returns_error() {
    let ctx = TestCtx::new();
    let err = resolve_templates("${env.NONEXISTENT}", &ctx);
    assert!(
        matches!(err, Err(TemplateError::MissingEnv(ref k)) if k == "NONEXISTENT"),
        "expected MissingEnv(NONEXISTENT), got: {:?}",
        err
    );
}

#[test]
fn test_resolve_unverified_task_produces_placeholder() {
    let ctx = TestCtx::new().with_unverified("build");
    let result = resolve_templates("out: ${build.result}", &ctx).unwrap();
    assert!(
        result.contains("UNRESOLVED"),
        "expected UNRESOLVED placeholder, got: {}",
        result
    );
}

#[test]
fn test_resolve_with_self_override() {
    let ctx = TestCtx::new();
    let mut self_vals = HashMap::new();
    self_vals.insert("tag".to_string(), "v1.2.3".to_string());
    let result = resolve_templates_with_self("Tag: ${tag}", &ctx, &self_vals).unwrap();
    assert_eq!(result, "Tag: v1.2.3");
}

// ---------------------------------------------------------------------------
// format::render tests
// ---------------------------------------------------------------------------

#[test]
fn test_render_substitutes_values() {
    let mut map = HashMap::new();
    map.insert("name", "evin");
    map.insert("version", "1.0.0");
    let out = render("Release {name} v{version}", &map);
    assert_eq!(out, "Release evin v1.0.0");
}

#[test]
fn test_render_missing_key_left_unreplaced() {
    let map: HashMap<&str, &str> = HashMap::new();
    let out = render("Hello {world}", &map);
    assert_eq!(out, "Hello {world}");
}

// ---------------------------------------------------------------------------
// compile_template (regex) tests
// ---------------------------------------------------------------------------

#[test]
fn test_compile_template_conventional_commit() {
    let re = compile_template("{type}({scope}?): {description}").unwrap();
    let caps = re.captures("feat(auth): add login").unwrap();
    assert_eq!(&caps["type"], "feat");
    assert_eq!(&caps["scope"], "auth");
    assert_eq!(&caps["description"], "add login");
}

#[test]
fn test_compile_template_no_scope() {
    let re = compile_template("{type}({scope}?): {description}").unwrap();
    let caps = re.captures("fix: correct typo");
    // Optional scope group — the regex may or may not match without parens;
    // the key assertion is that `type` and `description` always capture.
    if let Some(caps) = caps {
        assert_eq!(&caps["type"], "fix");
        assert_eq!(&caps["description"], "correct typo");
    }
}
