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

use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
use std::sync::{Mutex, OnceLock};
use unic_langid::langid;

static BUNDLE: OnceLock<Mutex<FluentBundle<FluentResource>>> = OnceLock::new();

fn init_bundle() -> Mutex<FluentBundle<FluentResource>> {
    let locale_env = std::env::var("EVIN_LOCALE").unwrap_or_else(|_| "en".to_string());
    let lang_id = locale_env.parse().unwrap_or_else(|_| langid!("en"));

    let mut bundle = FluentBundle::new_concurrent(vec![lang_id]);
    bundle.set_use_isolating(false);

    let ftl_resources = evin_io::load_ftl_resources(&locale_env);

    for res_item in ftl_resources {
        match FluentResource::try_new(res_item.content) {
            Ok(res) => {
                bundle.add_resource_overriding(res);
            }
            Err((_, errs)) => {
                eprintln!(
                    "Warning: failed to parse fluent resource '{}': {:?}",
                    res_item.path, errs
                );
            }
        }
    }

    Mutex::new(bundle)
}

fn get_bundle() -> &'static Mutex<FluentBundle<FluentResource>> {
    BUNDLE.get_or_init(init_bundle)
}

pub fn get(id: &str) -> String {
    let bundle = get_bundle().lock().unwrap();
    if let Some(msg) = bundle.get_message(id)
        && let Some(pattern) = msg.value()
    {
        let mut errors = vec![];
        let value = bundle.format_pattern(pattern, None, &mut errors);
        return value.to_string();
    }
    format!("<missing locale key: {}>", id)
}

pub fn getf(id: &str, args: &FluentArgs) -> String {
    let bundle = get_bundle().lock().unwrap();
    if let Some(msg) = bundle.get_message(id)
        && let Some(pattern) = msg.value()
    {
        let mut errors = vec![];
        let value = bundle.format_pattern(pattern, Some(args), &mut errors);
        return value.to_string();
    }
    format!("<missing locale key: {}>", id)
}

pub fn get_optional(id: &str) -> Option<String> {
    let bundle = get_bundle().lock().unwrap();
    if let Some(msg) = bundle.get_message(id)
        && let Some(pattern) = msg.value()
    {
        let mut errors = vec![];
        let value = bundle.format_pattern(pattern, None, &mut errors);
        return Some(value.to_string());
    }
    None
}

#[derive(Debug, Clone)]
pub struct LocaleDiagnostics {
    pub locale_env: String,
    pub resolved_lang: String,
    pub loaded_resources: usize,
    pub fallback_occurred: bool,
}

pub fn get_diagnostics() -> LocaleDiagnostics {
    let raw_env = std::env::var("EVIN_LOCALE").ok();
    let locale_env = raw_env.clone().unwrap_or_else(|| "en".to_string());
    let parsed_res: Result<unic_langid::LanguageIdentifier, _> = locale_env.parse();

    let (lang_id, fallback_occurred) = match parsed_res {
        Ok(parsed) => {
            let is_fallback =
                raw_env.is_some() && parsed.language.as_str() == "en" && locale_env != "en";
            (parsed, is_fallback)
        }
        Err(_) => (langid!("en"), raw_env.is_some()),
    };

    let ftl_resources = evin_io::load_ftl_resources(&locale_env);

    LocaleDiagnostics {
        locale_env,
        resolved_lang: lang_id.language.to_string(),
        loaded_resources: ftl_resources.len(),
        fallback_occurred,
    }
}
